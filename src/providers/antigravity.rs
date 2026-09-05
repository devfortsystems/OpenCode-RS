//! Antigravity IDE provider — gRPC-Web do language_server.exe
//!
//! Antigravity IDE używa lokalnego gRPC-Web server (language_server.exe) do komunikacji z modelami.
//! Ten provider łączy się bezpośrednio z language server, omijając IDE.
//!
//! Wymaga uruchomionego Antigravity IDE (language_server.exe musi nasłuchiwać).
//! Provider automatycznie wykrywa port i CSRF token.
//!
//! 32 modele: Gemini 3.x, Claude 4.6, GPT-OSS — wszystkie darmowe (free-tier).

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc::Sender;

use super::{ChatMessage, Provider};

const SERVICE: &str = "exa.language_server_pb.LanguageServerService";

/// Model Antigravity z GetAvailableModels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntigravityModel {
    pub id: String,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(rename = "maxTokens", default)]
    pub max_tokens: Option<u64>,
    #[serde(rename = "modelProvider", default)]
    pub model_provider: Option<String>,
    #[serde(rename = "model", default)]
    pub model_enum: Option<String>,
}

/// Odpowiedź z GetAvailableModels
#[derive(Debug, Deserialize)]
struct GetAvailableModelsResponse {
    #[serde(default)]
    response: Option<ModelsResponse>,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default)]
    models: std::collections::HashMap<String, serde_json::Value>,
}

/// Provider Antigravity — łączy się z language_server.exe via gRPC-Web
pub struct AntigravityProvider {
    port: u16,
    csrf_token: String,
    client: reqwest::Client,
}

impl AntigravityProvider {
    /// Tworzy provider z konkretnym portem i tokenem (do testów)
    pub fn new(port: u16, csrf_token: String) -> Self {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(120))
            .build()
            .expect("reqwest client");
        Self {
            port,
            csrf_token,
            client,
        }
    }

    /// Automatyczne wykrywanie uruchomionego Antigravity IDE.
    /// Szuka language_server.exe, znajduje port HTTPS, pobiera CSRF token.
    /// Jeśli nie znajdzie — uruchamia language_server.exe w trybie standalone.
    pub async fn discover() -> Result<Self> {
        // 1. Spróbuj wykryć już uruchomiony LS (z IDE)
        if let Ok((port, csrf_token)) = discover_antigravity().await {
            return Ok(Self::new(port, csrf_token));
        }
        // 2. Fallback — uruchom language_server.exe standalone (jak IDE to robi)
        let (port, csrf_token) = start_standalone_ls().await?;
        Ok(Self::new(port, csrf_token))
    }

    /// Sprawdza czy Antigravity IDE jest uruchomione (lub można uruchomić LS)
    pub async fn is_available() -> bool {
        Self::discover().await.is_ok()
    }

    /// Pobiera listę dostępnych modeli z language server
    pub async fn get_available_models(&self) -> Result<Vec<AntigravityModel>> {
        let resp = self
            .grpc_web_json("GetAvailableModels", serde_json::json!({}))
            .await?;

        let parsed: GetAvailableModelsResponse = serde_json::from_slice(&resp)?;
        let models_map = parsed
            .response
            .ok_or_else(|| anyhow!("brak response w GetAvailableModels"))?
            .models;

        let mut models = Vec::new();
        for (id, info) in models_map {
            let display_name = info
                .get("displayName")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let max_tokens = info
                .get("maxTokens")
                .and_then(|v| v.as_u64());
            let model_provider = info
                .get("modelProvider")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let model_enum = info
                .get("model")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // Pomiń modele tab (autocomplete, nie chat)
            if id.contains("tab_") {
                continue;
            }

            models.push(AntigravityModel {
                id,
                display_name,
                max_tokens,
                model_provider,
                model_enum,
            });
        }

        // Sortuj: najpierw Gemini, potem Claude, potem GPT
        models.sort_by(|a, b| {
            let provider_order = |p: &Option<String>| match p.as_deref() {
                Some("MODEL_PROVIDER_GOOGLE") => 0,
                Some("MODEL_PROVIDER_ANTHROPIC") => 1,
                Some("MODEL_PROVIDER_OPENAI") => 2,
                _ => 3,
            };
            provider_order(&a.model_provider).cmp(&provider_order(&b.model_provider))
                .then_with(|| a.id.cmp(&b.id))
        });

        Ok(models)
    }

    /// Wywołanie gRPC-Web z JSON body (dla prostych metod jak GetAvailableModels)
    async fn grpc_web_json(&self, method: &str, body: serde_json::Value) -> Result<Vec<u8>> {
        let url = format!("https://127.0.0.1:{}/{}/{}", self.port, SERVICE, method);
        let json_bytes = serde_json::to_vec(&body)?;

        // gRPC-Web framing: 1 byte flag (0x00) + 4 bytes big-endian length + message
        let mut frame = Vec::with_capacity(5 + json_bytes.len());
        frame.push(0x00); // no compression
        frame.extend_from_slice(&(json_bytes.len() as u32).to_be_bytes());
        frame.extend_from_slice(&json_bytes);

        let resp = self
            .client
            .post(&url)
            .header("content-type", "application/grpc-web+json")
            .header("x-codeium-csrf-token", &self.csrf_token)
            .header("x-grpc-web", "1")
            .header("x-user-agent", "CONNECT_ES_USER_AGENT")
            .header("connect-protocol-version", "1")
            .body(frame)
            .send()
            .await?;

        let status = resp.status();
        let grpc_status = resp
            .headers()
            .get("grpc-status")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_default();
        let grpc_msg = resp
            .headers()
            .get("grpc-message")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .unwrap_or_default();

        let bytes = resp.bytes().await?;

        if !status.is_success() {
            bail!(
                "HTTP {} body={:?}",
                status,
                &bytes[..bytes.len().min(200)]
            );
        }

        if !grpc_status.is_empty() && grpc_status != "0" {
            bail!("gRPC status {}: {}", grpc_status, grpc_msg);
        }

        // Parsuj gRPC-Web frame z odpowiedzi
        parse_grpc_web_frame(&bytes)
    }

    /// Wywołanie gRPC-Web z binary protobuf body (dla HandleStreamingCommand)
    async fn grpc_web_proto_stream(
        &self,
        method: &str,
        proto_body: Vec<u8>,
    ) -> Result<reqwest::Response> {
        let url = format!("https://127.0.0.1:{}/{}/{}", self.port, SERVICE, method);

        // gRPC-Web framing
        let mut frame = Vec::with_capacity(5 + proto_body.len());
        frame.push(0x00);
        frame.extend_from_slice(&(proto_body.len() as u32).to_be_bytes());
        frame.extend_from_slice(&proto_body);

        let resp = self
            .client
            .post(&url)
            .header("content-type", "application/grpc-web+proto")
            .header("x-codeium-csrf-token", &self.csrf_token)
            .header("x-grpc-web", "1")
            .header("x-user-agent", "CONNECT_ES_USER_AGENT")
            .body(frame)
            .send()
            .await?;

        Ok(resp)
    }
}

#[async_trait]
impl Provider for AntigravityProvider {
    fn name(&self) -> &str {
        "antigravity"
    }

    async fn stream_chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        token_tx: Sender<String>,
    ) -> Result<()> {
        // Mapuj model ID na enum modelu
        let models = self.get_available_models().await?;
        let model_enum = models
            .iter()
            .find(|m| m.id == model || m.display_name.as_deref() == Some(model))
            .and_then(|m| m.model_enum.clone())
            .ok_or_else(|| anyhow!("Model '{}' nie znaleziony w Antigravity. Dostępne: {}", model, models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>().join(", ")))?;

        // Zbuduj prompt z messages (ostatnia user message = commandText)
        let prompt = messages
            .iter()
            .filter(|m| m.role == "user")
            .last()
            .map(|m| m.content.clone())
            .unwrap_or_default();

        if prompt.is_empty() {
            bail!("Brak promptu (ostatnia user message jest pusta)");
        }

        // Zbuduj protobuf HandleStreamingCommandRequest
        let proto_body = build_streaming_command_request(&model_enum, &prompt);

        // Wyślij żądanie streaming
        let resp = self.grpc_web_proto_stream("HandleStreamingCommand", proto_body).await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("HTTP {}: {}", status, &body[..body.len().min(300)]);
        }

        // Sprawdź grpc-status w headers
        let grpc_status = resp
            .headers()
            .get("grpc-status")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let grpc_msg = resp
            .headers()
            .get("grpc-message")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !grpc_status.is_empty() && grpc_status != "0" {
            bail!("gRPC status {}: {}", grpc_status, grpc_msg);
        }

        // Stream odpowiedzi — czytaj gRPC-Web frames i wyodrębnij tekst
        use futures_util::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buffer = Vec::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            buffer.extend_from_slice(&chunk);

            // Parsuj wszystkie kompletne frames z buffer
            while buffer.len() >= 5 {
                let msg_len = u32::from_be_bytes([buffer[1], buffer[2], buffer[3], buffer[4]]) as usize;
                if buffer.len() < 5 + msg_len {
                    break; // niekompletny frame, czekaj na więcej danych
                }

                let msg_data = &buffer[5..5 + msg_len];
                // Wyciągnij tekst z protobuf response
                if let Some(text) = extract_text_from_proto(msg_data) {
                    if !text.is_empty() {
                        token_tx.send(text).await.ok();
                    }
                }

                // Usuń przetworzony frame z buffer
                buffer.drain(..5 + msg_len);
            }
        }

        Ok(())
    }
}

// ─── Protobuf encoding helpers ──────────────────────────────────────

fn encode_varint(value: u32) -> Vec<u8> {
    let mut result = Vec::new();
    let mut v = value;
    while v > 127 {
        result.push((v & 0x7F) as u8 | 0x80);
        v >>= 7;
    }
    result.push(v as u8 & 0x7F);
    result
}

fn encode_field_string(field_num: u32, value: &str) -> Vec<u8> {
    let tag = (field_num << 3) | 2; // wire type 2 (length-delimited)
    let data = value.as_bytes();
    let mut result = encode_varint(tag);
    result.extend(encode_varint(data.len() as u32));
    result.extend_from_slice(data);
    result
}

fn encode_field_varint(field_num: u32, value: u32) -> Vec<u8> {
    let tag = (field_num << 3) | 0; // wire type 0 (varint)
    let mut result = encode_varint(tag);
    result.extend(encode_varint(value));
    result
}

fn encode_field_message(field_num: u32, message: &[u8]) -> Vec<u8> {
    let tag = (field_num << 3) | 2; // wire type 2
    let mut result = encode_varint(tag);
    result.extend(encode_varint(message.len() as u32));
    result.extend_from_slice(message);
    result
}

/// Buduje HandleStreamingCommandRequest jako binary protobuf
fn build_streaming_command_request(model_enum: &str, prompt: &str) -> Vec<u8> {
    // Metadata message (field 1)
    let mut metadata = Vec::new();
    metadata.extend(encode_field_string(3, &uuid::Uuid::new_v4().to_string())); // request_id
    metadata.extend(encode_field_string(5, "antigravity")); // ide_name
    metadata.extend(encode_field_string(6, "2.12.0")); // ide_version
    metadata.extend(encode_field_string(7, "antigravity")); // extension_name
    metadata.extend(encode_field_string(8, "2.12.0")); // extension_version

    // Document message (field 2) — użyj aktualnego pliku z forward slashes
    let current_file = std::env::current_dir()
        .map(|d| d.join("antigravity_chat.txt"))
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| "C:/tmp/antigravity_chat.txt".to_string());
    let document = encode_field_string(1, &current_file); // absolute_path

    // HandleStreamingCommandRequest
    let mut request = Vec::new();
    request.extend(encode_field_message(1, &metadata)); // metadata
    request.extend(encode_field_message(2, &document)); // document
    // requested_model_id (field 4) — Model enum jako varint
    // Mapujemy nazwę modelu na enum value (heurystyka)
    let model_val = model_enum_to_int(model_enum);
    if model_val > 0 {
        request.extend(encode_field_varint(4, model_val));
    }
    request.extend(encode_field_varint(6, 0)); // selection_start_line
    request.extend(encode_field_varint(7, 0)); // selection_end_line
    request.extend(encode_field_string(8, prompt)); // command_text
    request.extend(encode_field_varint(9, 16)); // request_source = CASCADE_CHAT

    request
}

/// Mapuje nazwę modelu (MODEL_CHAT_20706 etc.) na wartość enum.
/// TODO: znaleźć dokładne wartości enum z proto descriptor.
/// Na razie używamy heurystyki — wartości mogą być niepoprawne.
fn model_enum_to_int(model_enum: &str) -> u32 {
    // Wyciągnij liczbę z nazwy (np. MODEL_CHAT_20706 → 20706)
    let num: Option<u32> = model_enum
        .split('_')
        .filter_map(|s| s.parse().ok())
        .next();
    num.unwrap_or(0)
}

/// Wyciąga tekst z protobuf HandleStreamingCommandResponse
fn extract_text_from_proto(data: &[u8]) -> Option<String> {
    let mut offset = 0;
    let mut text = String::new();

    while offset < data.len() {
        // Czytaj tag
        let (tag, tag_len) = decode_varint(&data[offset..])?;
        offset += tag_len;

        let field_num = (tag >> 3) as u32;
        let wire_type = (tag & 0x07) as u8;

        match wire_type {
            0 => {
                // varint
                let (_, len) = decode_varint(&data[offset..])?;
                offset += len;
            }
            2 => {
                // length-delimited
                let (len, len_size) = decode_varint(&data[offset..])?;
                offset += len_size;
                let end = offset + len as usize;
                if end > data.len() {
                    return None;
                }
                let field_data = &data[offset..end];
                offset = end;

                // Pole 1 = completion_id (string), pole 2 = prompt_id (string)
                // Pole 3 = diff (UnifiedDiff message)
                // W diff: pole 1 = lines (repeated string)
                // Szukaj string fields które wyglądają jak tekst odpowiedzi
                if field_num == 3 {
                    // Diff message — szukaj tekstu wewnątrz
                    if let Some(diff_text) = extract_text_from_diff(field_data) {
                        text.push_str(&diff_text);
                    }
                } else if field_num == 1 || field_num == 2 {
                    // completion_id / prompt_id — pomiń
                }
            }
            _ => {
                // Inne wire types — pomiń
                return if text.is_empty() { None } else { Some(text) };
            }
        }
    }

    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Wyciąga tekst z Diff message (pole 3 = repeated string lines)
fn extract_text_from_diff(data: &[u8]) -> Option<String> {
    let mut offset = 0;
    let mut lines = Vec::new();

    while offset < data.len() {
        let (tag, tag_len) = decode_varint(&data[offset..])?;
        offset += tag_len;

        let field_num = (tag >> 3) as u32;
        let wire_type = (tag & 0x07) as u8;

        match wire_type {
            0 => {
                let (_, len) = decode_varint(&data[offset..])?;
                offset += len;
            }
            2 => {
                let (len, len_size) = decode_varint(&data[offset..])?;
                offset += len_size;
                let end = offset + len as usize;
                if end > data.len() {
                    return None;
                }
                let field_data = &data[offset..end];
                offset = end;

                // Pole 1 = lines (repeated string)
                if field_num == 1 {
                    if let Ok(s) = std::str::from_utf8(field_data) {
                        lines.push(s.to_string());
                    }
                }
            }
            _ => break,
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// Dekoduje varint z bytes, zwraca (wartość, liczba bajtów)
fn decode_varint(data: &[u8]) -> Option<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift = 0;
    for (i, &byte) in data.iter().enumerate() {
        result |= ((byte & 0x7F) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some((result, i + 1));
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    None
}

/// Parsuje gRPC-Web frame z odpowiedzi (pojedyncza wiadomość)
fn parse_grpc_web_frame(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 5 {
        bail!("gRPC-Web frame za krótki: {} bytes", data.len());
    }
    let _flag = data[0];
    let msg_len = u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;
    if data.len() < 5 + msg_len {
        bail!("gRPC-Web frame niekompletny: oczekiwano {} bytes, mam {}", msg_len, data.len() - 5);
    }
    Ok(data[5..5 + msg_len].to_vec())
}

// ─── Antigravity discovery (Windows) ────────────────────────────────

/// Uruchamia language_server.exe w trybie standalone (jak Antigravity IDE to robi).
/// Zwraca (port, csrf_token) gdy LS zacznie nasłuchiwać.
async fn start_standalone_ls() -> Result<(u16, String)> {
    // 1. Znajdź binary language_server.exe
    let ls_path = find_language_server_binary()?;
    eprintln!("Antigravity: uruchamiam LS standalone: {}", ls_path.display());

    // 2. Generuj CSRF token (UUID v4 bez myślników, jak IDE)
    let csrf_token = uuid::Uuid::new_v4().simple().to_string();
    let port: u16 = 13443; // stały port (jak IDE używa https_server_port)

    // 3. Argumenty — dokładnie jak languageServer.js w Antigravity IDE
    let mut cmd = tokio::process::Command::new(&ls_path);
    cmd.args([
        "--standalone",
        "--override_ide_name", "antigravity",
        "--subclient_type", "hub",
        "--override_user_agent_name", "antigravity",
        "--override_ide_version", "1.0.0",
        "--https_server_port", &port.to_string(),
        "--csrf_token", &csrf_token,
        "--app_data_dir", "antigravity-ide",
        "--api_server_url", "https://generativelanguage.googleapis.com",
        "--cloud_code_endpoint", "https://daily-cloudcode-pa.googleapis.com",
        "--enable_sidecars",
    ]);
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(false); // LS ma żyć po zakończeniu opencode-rs

    let _child = cmd.spawn()
        .map_err(|e| anyhow!("Nie można uruchomić language_server.exe: {e}"))?;

    // 4. Czekaj aż LS zacznie nasłuchiwać (max 15s)
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        if std::time::Instant::now() > deadline {
            bail!("language_server.exe nie wystartował w 15s — timeout");
        }
        if try_grpc_web_port(port, &csrf_token).await.is_ok() {
            eprintln!("Antigravity: LS nasłuchuje na porcie {port}");
            return Ok((port, csrf_token));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Znajduje ścieżkę language_server.exe w instalacjach Antigravity IDE.
fn find_language_server_binary() -> Result<std::path::PathBuf> {
    let candidates = [
        // Najnowsza instalacja (antigravity)
        r"C:\Users\Daniel\AppData\Local\Programs\antigravity\resources\bin\language_server.exe",
        // Starsza instalacja (Antigravity IDE)
        r"C:\Users\Daniel\AppData\Local\Programs\Antigravity IDE\resources\app\extensions\antigravity\bin\language_server_windows_x64.exe",
    ];

    // Sprawdź hardcoded ścieżki
    for path in &candidates {
        let p = std::path::Path::new(path);
        if p.exists() {
            return Ok(p.to_path_buf());
        }
    }

    // Fallback — szukaj dynamicznie w LOCALAPPDATA
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let base = std::path::Path::new(&local_app_data).join("Programs");
        // antigravity/resources/bin/language_server.exe
        let p1 = base.join("antigravity").join("resources").join("bin").join("language_server.exe");
        if p1.exists() {
            return Ok(p1);
        }
        // Antigravity IDE/resources/app/extensions/antigravity/bin/language_server_windows_x64.exe
        let p2 = base.join("Antigravity IDE").join("resources").join("app")
            .join("extensions").join("antigravity").join("bin").join("language_server_windows_x64.exe");
        if p2.exists() {
            return Ok(p2);
        }
    }

    bail!(
        "Nie znaleziono language_server.exe. \
         Zainstaluj Antigravity IDE z https://antigravity.google.com/"
    )
}

/// Wykrywa uruchomiony Antigravity IDE: znajduje language_server.exe, port HTTPS, CSRF token.
async fn discover_antigravity() -> Result<(u16, String)> {
    // 1. Znajdź language_server.exe i jego command line
    let cmd_output = tokio::process::Command::new("powershell")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(
            "Get-CimInstance Win32_Process | \
             Where-Object { $_.Name -eq 'language_server.exe' } | \
             Select-Object ProcessId,CommandLine | \
             Format-List",
        )
        .output()
        .await
        .map_err(|e| anyhow!("PowerShell execution failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&cmd_output.stdout);
    if stdout.trim().is_empty() {
        bail!(
            "Antigravity IDE nie jest uruchomione. \
             Uruchom Antigravity IDE i spróbuj ponownie."
        );
    }

    // 2. Wyciągnij CSRF token z command line
    let csrf_token = extract_param(&stdout, "--csrf_token")
        .ok_or_else(|| anyhow!("Nie znaleziono --csrf_token w command line language_server.exe"))?;

    // 3. Znajdź porty nasłuchujące language_server.exe
    let pid = extract_param(&stdout, "ProcessId")
        .or_else(|| {
            // PowerShell Format-List może mieć inny format
            stdout
                .lines()
                .find(|l| l.contains("ProcessId"))
                .and_then(|l| l.split(':').nth(1))
                .map(|s| s.trim().to_string())
        });

    // Znajdź porty nasłuchujące
    let port_cmd = if let Some(ref pid) = pid {
        format!(
            "Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | \
             Where-Object {{ $_.OwningProcess -eq {} }} | \
             Select-Object LocalPort | \
             Sort-Object LocalPort | \
             Format-Table -HideTableHeaders",
            pid
        )
    } else {
        // Fallback — szukaj wszystkich portów w zakresie 13000-14000
        format!(
            "Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | \
             Where-Object {{ $_.LocalPort -ge 13000 -and $_.LocalPort -le 14000 }} | \
             Select-Object LocalPort | \
             Sort-Object LocalPort | \
             Format-Table -HideTableHeaders"
        )
    };

    let port_output = tokio::process::Command::new("powershell")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(&port_cmd)
        .output()
        .await
        .map_err(|e| anyhow!("PowerShell port query failed: {}", e))?;

    let port_str = String::from_utf8_lossy(&port_output.stdout);
    let ports: Vec<u16> = port_str
        .lines()
        .filter_map(|l| l.trim().parse::<u16>().ok())
        .collect();

    if ports.is_empty() {
        bail!("Nie znaleziono portów nasłuchujących language_server.exe");
    }

    // 4. Znajdź port HTTPS (gRPC-Web) — największy port zazwyczaj
    // Sprawdź każdy port próbując połączyć się z gRPC-Web
    for &port in &ports {
        if try_grpc_web_port(port, &csrf_token).await.is_ok() {
            return Ok((port, csrf_token));
        }
    }

    // Fallback — użyj największego portu
    let port = ports.iter().max().copied().unwrap();
    Ok((port, csrf_token))
}

/// Wyciąga parametr z tekstu command line (format: --param=value)
fn extract_param(text: &str, param: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(idx) = line.find(param) {
            let after = &line[idx + param.len()..];
            // Pomiń = lub :
            let after = after.trim_start_matches(['=', ':', ' ']);
            // Wyciągnij wartość do spacji lub końca
            let value: String = after
                .chars()
                .take_while(|c| !c.is_whitespace())
                .collect();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// Próbuje połączyć się z portem via gRPC-Web (Heartbeat)
async fn try_grpc_web_port(port: u16, csrf_token: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(5))
        .build()?;

    let url = format!("https://127.0.0.1:{}/{}/Heartbeat", port, SERVICE);
    let json_bytes = b"{}";
    let mut frame = Vec::with_capacity(5 + json_bytes.len());
    frame.push(0x00);
    frame.extend_from_slice(&(json_bytes.len() as u32).to_be_bytes());
    frame.extend_from_slice(json_bytes);

    let resp = client
        .post(&url)
        .header("content-type", "application/grpc-web+json")
        .header("x-codeium-csrf-token", csrf_token)
        .header("x-grpc-web", "1")
        .body(frame)
        .send()
        .await?;

    let grpc_status = resp
        .headers()
        .get("grpc-status")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if grpc_status == "0" || grpc_status.is_empty() {
        Ok(())
    } else {
        bail!("gRPC status: {}", grpc_status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_varint() {
        assert_eq!(encode_varint(0), vec![0x00]);
        assert_eq!(encode_varint(1), vec![0x01]);
        assert_eq!(encode_varint(127), vec![0x7F]);
        assert_eq!(encode_varint(128), vec![0x80, 0x01]);
        assert_eq!(encode_varint(300), vec![0xAC, 0x02]);
    }

    #[test]
    fn test_encode_field_string() {
        let result = encode_field_string(1, "hello");
        // tag = (1 << 3) | 2 = 10, length = 5, data = "hello"
        assert_eq!(result, vec![0x0A, 0x05, b'h', b'e', b'l', b'l', b'o']);
    }

    #[test]
    fn test_decode_varint() {
        assert_eq!(decode_varint(&[0x00]), Some((0, 1)));
        assert_eq!(decode_varint(&[0x01]), Some((1, 1)));
        assert_eq!(decode_varint(&[0x7F]), Some((127, 1)));
        assert_eq!(decode_varint(&[0x80, 0x01]), Some((128, 2)));
        assert_eq!(decode_varint(&[0xAC, 0x02]), Some((300, 2)));
    }

    #[test]
    fn test_parse_grpc_web_frame() {
        // Empty message
        let frame = vec![0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(parse_grpc_web_frame(&frame).unwrap(), Vec::<u8>::new());

        // Message with "hello"
        let mut frame = vec![0x00, 0x00, 0x00, 0x00, 0x05];
        frame.extend_from_slice(b"hello");
        assert_eq!(parse_grpc_web_frame(&frame).unwrap(), b"hello".to_vec());
    }

    #[test]
    fn test_build_streaming_command_request() {
        let request = build_streaming_command_request("MODEL_CHAT_20706", "Say hello");
        // Powinien zawierać commandText "Say hello"
        let prompt_str = "Say hello";
        assert!(request
            .windows(prompt_str.len())
            .any(|w| w == prompt_str.as_bytes()));
        // Powinien zawierać CASCADE_CHAT (16) jako varint
        assert!(request.contains(&16u8));
    }

    #[test]
    fn test_model_enum_to_int() {
        assert_eq!(model_enum_to_int("MODEL_CHAT_20706"), 20706);
        assert_eq!(model_enum_to_int("MODEL_CHAT_23310"), 23310);
        assert_eq!(model_enum_to_int("unknown"), 0);
    }

    #[test]
    fn test_extract_param() {
        let text = "CommandLine : language_server.exe --csrf_token abc123 --port 13502";
        assert_eq!(extract_param(text, "--csrf_token"), Some("abc123".to_string()));
        assert_eq!(extract_param(text, "--port"), Some("13502".to_string()));
        assert_eq!(extract_param(text, "--nonexistent"), None);
    }
}
