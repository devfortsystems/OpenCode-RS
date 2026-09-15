//! Antigravity IDE provider — Connect streaming JSON do language_server.exe
//!
//! Antigravity IDE używa lokalnego gRPC-Web/Connect server (language_server.exe) do komunikacji z modelami.
//! Ten provider łączy się bezpośrednio z language server, omijając IDE.
//!
//! Wymaga uruchomionego Antigravity IDE (language_server.exe musi nasłuchiwać).
//! Provider automatycznie wykrywa port i CSRF token.
//!
//! Protokół: Connect streaming (application/connect+json) z 5-bajtowym framingiem.
//! Schema wyekstrahowana z proto descriptorów w language_server.exe + main.js:
//!   - HandleStreamingCommandRequest: metadata, document{absoluteUri}, requestedModelId (Model enum), commandText, requestSource (CommandRequestSource enum)
//!   - HandleStreamingCommandResponse: completionId, promptId, diff{lines[{text,type}]}, rawText, trajectory
//!   - Document: absoluteUri (field 12), text, editorLanguage, cursorPosition, visibleRange
//!   - Model enum: MODEL_CHAT_20706=235, MODEL_PLACEHOLDER_M26=1026 (Claude Opus 4.6), etc.
//!   - CommandRequestSource: COMMAND_REQUEST_SOURCE_CASCADE_CHAT=16

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
    /// Nazwa enum modelu z proto (np. "MODEL_CHAT_20706", "MODEL_PLACEHOLDER_M26")
    #[serde(rename = "model", default)]
    pub model_enum: Option<String>,
    /// Wartość liczbowa enum modelu (np. 235 dla MODEL_CHAT_20706, 1026 dla MODEL_PLACEHOLDER_M26)
    /// Mapowana przez model_enum_name_to_int()
    pub model_enum_value: u32,
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

            // Mapuj nazwę enum (np. "MODEL_CHAT_20706") na wartość liczbową (235)
            let model_enum_value = model_enum
                .as_deref()
                .map(model_enum_name_to_int)
                .unwrap_or(0);

            models.push(AntigravityModel {
                id,
                display_name,
                max_tokens,
                model_provider,
                model_enum,
                model_enum_value,
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

    /// Wywołanie Connect streaming (application/connect+json) z JSON body.
    /// Dla server-streaming RPC jak HandleStreamingCommand.
    /// Framing: 1 byte flags (0x00) + 4 bytes big-endian length + JSON message.
    async fn connect_stream(
        &self,
        method: &str,
        json_body: serde_json::Value,
    ) -> Result<reqwest::Response> {
        let url = format!("https://127.0.0.1:{}/{}/{}", self.port, SERVICE, method);
        let json_bytes = serde_json::to_vec(&json_body)?;

        // Connect streaming framing: 1 byte flags + 4 bytes BE length + message
        let mut frame = Vec::with_capacity(5 + json_bytes.len());
        frame.push(0x00); // no compression, not end-of-stream
        frame.extend_from_slice(&(json_bytes.len() as u32).to_be_bytes());
        frame.extend_from_slice(&json_bytes);

        let resp = self
            .client
            .post(&url)
            .header("content-type", "application/connect+json")
            .header("x-codeium-csrf-token", &self.csrf_token)
            .header("connect-protocol-version", "1")
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
        // Mapuj model ID na enum wartość
        let models = self.get_available_models().await?;
        // Strip "antigravity-" prefix (z dynamic discovery: "antigravity-MODEL_CHAT_20706")
        let clean_model = model.strip_prefix("antigravity-").unwrap_or(model);
        let model_info = models
            .iter()
            .find(|m| {
                m.id == clean_model
                    || m.id == model
                    || m.display_name.as_deref() == Some(model)
                    || m.display_name.as_deref() == Some(clean_model)
                    || m.model_enum.as_deref() == Some(clean_model)
                    || m.model_enum.as_deref() == Some(model)
            })
            .ok_or_else(|| anyhow!("Model '{}' nie znaleziony w Antigravity. Dostępne: {}", model, models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>().join(", ")))?;

        let model_enum_value = model_info.model_enum_value;
        if model_enum_value == 0 {
            bail!("Model '{}' ma nieznany enum value (model_enum: {:?})", model, model_info.model_enum);
        }

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

        // Zbuduj JSON HandleStreamingCommandRequest (Connect streaming)
        let request = build_streaming_command_json(model_enum_value, &prompt);

        // Wyślij żądanie streaming przez Connect protocol
        let resp = self.connect_stream("HandleStreamingCommand", request).await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("HTTP {}: {}", status, &body[..body.len().min(300)]);
        }

        // Stream odpowiedzi — czytaj Connect streaming frames (JSON)
        use futures_util::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut buffer = Vec::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            buffer.extend_from_slice(&chunk);

            // Parsuj wszystkie kompletne Connect frames z buffer
            // Frame format: 1 byte flags + 4 bytes BE length + JSON data
            while buffer.len() >= 5 {
                let flags = buffer[0];
                let msg_len = u32::from_be_bytes([buffer[1], buffer[2], buffer[3], buffer[4]]) as usize;
                if buffer.len() < 5 + msg_len {
                    break; // niekompletny frame, czekaj na więcej danych
                }

                let msg_data = &buffer[5..5 + msg_len];

                // Sprawdź czy to error frame (flags bit 1 = 0x02)
                if flags & 0x02 != 0 {
                    // Error frame lub end-of-stream — sprawdź czy ma error
                    if let Ok(err_resp) = serde_json::from_slice::<serde_json::Value>(msg_data) {
                        if let Some(error) = err_resp.get("error") {
                            let code = error.get("code").and_then(|v| v.as_str()).unwrap_or("unknown");
                            let msg = error.get("message").and_then(|v| v.as_str()).unwrap_or("");
                            if !msg.is_empty() {
                                bail!("Antigravity error ({}): {}", code, msg);
                            }
                        }
                    }
                    // End-of-stream bez error — OK
                    buffer.drain(..5 + msg_len);
                    continue;
                }

                // Normal data frame — wyodrębnij tekst z JSON response
                if let Ok(resp_json) = serde_json::from_slice::<serde_json::Value>(msg_data) {
                    if let Some(text) = extract_text_from_connect_response(&resp_json) {
                        if !text.is_empty() {
                            token_tx.send(text).await.ok();
                        }
                    }
                }

                // Usuń przetworzony frame z buffer
                buffer.drain(..5 + msg_len);
            }
        }

        Ok(())
    }
}

// ─── Connect JSON request/response helpers ──────────────────────────

/// Buduje HandleStreamingCommandRequest jako JSON (Connect streaming)
/// Schema z proto descriptor:
///   field 1: metadata (Metadata message)
///   field 2: document (Document message z absoluteUri)
///   field 4: requestedModelId (Model enum jako liczba)
///   field 8: commandText (string)
///   field 9: requestSource (CommandRequestSource enum, 16 = CASCADE_CHAT)
fn build_streaming_command_json(model_enum_value: u32, prompt: &str) -> serde_json::Value {
    let request_id = uuid::Uuid::new_v4().to_string();

    // Ścieżka pliku jako URI (Connect wymaga absoluteUri, nie absolutePath)
    let current_file = std::env::current_dir()
        .map(|d| d.join("antigravity_chat.txt"))
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| "C:/tmp/antigravity_chat.txt".to_string());
    let file_uri = format!("file:///{}", current_file);

    serde_json::json!({
        "metadata": {
            "requestId": request_id,
            "ideName": "antigravity",
            "ideVersion": "1.0.0",
            "extensionName": "antigravity",
            "extensionVersion": "1.0.0"
        },
        "document": {
            "absoluteUri": file_uri,
            "text": ""
        },
        "requestedModelId": model_enum_value,
        "commandText": prompt,
        "requestSource": 16  // COMMAND_REQUEST_SOURCE_CASCADE_CHAT
    })
}

/// Wyciąga tekst z Connect streaming JSON response (HandleStreamingCommandResponse)
/// Schema:
///   field 3: diff (UnifiedDiff z lines[{text, type}])
///   field 16: rawText (string — pełny tekst odpowiedzi)
///   field 15: trajectory (Trajectory — może zawierać wiadomości)
fn extract_text_from_connect_response(resp: &serde_json::Value) -> Option<String> {
    let mut text = String::new();

    // 1. Sprawdź rawText (field 16) — pełny tekst odpowiedzi
    if let Some(raw) = resp.get("rawText").and_then(|v| v.as_str()) {
        if !raw.is_empty() {
            return Some(raw.to_string());
        }
    }

    // 2. Sprawdź diff.lines (field 3) — wyodrębnij INSERT lines
    if let Some(diff) = resp.get("diff") {
        if let Some(lines) = diff.get("lines").and_then(|v| v.as_array()) {
            for line in lines {
                let line_text = line.get("text").and_then(|v| v.as_str()).unwrap_or("");
                let line_type = line.get("type").and_then(|v| v.as_str()).unwrap_or("");
                // INSERT lines = wygenerowany tekst
                if line_type.contains("INSERT") {
                    text.push_str(line_text);
                    text.push('\n');
                }
            }
        }
    }

    // 3. Sprawdź trajectory (field 15) — może zawierać wiadomości asystenta
    if text.is_empty() {
        if let Some(traj) = resp.get("trajectory") {
            if let Some(messages) = traj.get("messages").and_then(|v| v.as_array()) {
                for msg in messages {
                    let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("");
                    if role == "assistant" || role.to_lowercase().contains("model") {
                        if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
                            text.push_str(content);
                            text.push('\n');
                        }
                    }
                }
            }
        }
    }

    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Mapuje nazwę enum modelu (z GetAvailableModels `model` field) na wartość liczbową.
/// Wartości wyekstrahowane z proto descriptor codeium_common_pb:
///   MODEL_CHAT_20706 = 235, MODEL_CHAT_23310 = 269
///   MODEL_GOOGLE_GEMINI_2_5_FLASH = 312, MODEL_GOOGLE_GEMINI_2_5_PRO = 246
///   MODEL_CLAUDE_4_SONNET = 281, MODEL_CLAUDE_4_OPUS = 290
///   MODEL_PLACEHOLDER_M{N} = 1000 + N
fn model_enum_name_to_int(name: &str) -> u32 {
    // Hardcoded wartości dla znanych modeli (z proto descriptor)
    let hardcoded: &[(&str, u32)] = &[
        ("MODEL_CHAT_20706", 235),
        ("MODEL_CHAT_23310", 269),
        ("MODEL_GOOGLE_GEMINI_2_5_FLASH", 312),
        ("MODEL_GOOGLE_GEMINI_2_5_FLASH_THINKING", 313),
        ("MODEL_GOOGLE_GEMINI_2_5_FLASH_THINKING_TOOLS", 329),
        ("MODEL_GOOGLE_GEMINI_2_5_FLASH_LITE", 330),
        ("MODEL_GOOGLE_GEMINI_2_5_PRO", 246),
        ("MODEL_GOOGLE_GEMINI_2_5_PRO_EVAL", 331),
        ("MODEL_GOOGLE_GEMINI_FOR_GOOGLE_2_5_PRO", 327),
        ("MODEL_GOOGLE_GEMINI_2_5_FLASH_IMAGE_PREVIEW", 332),
        ("MODEL_GOOGLE_GEMINI_COMPUTER_USE_EXPERIMENTAL", 335),
        ("MODEL_CLAUDE_4_SONNET", 281),
        ("MODEL_CLAUDE_4_SONNET_THINKING", 282),
        ("MODEL_CLAUDE_4_OPUS", 290),
        ("MODEL_CLAUDE_4_OPUS_THINKING", 291),
        ("MODEL_CLAUDE_4_5_SONNET", 333),
        ("MODEL_CLAUDE_4_5_SONNET_THINKING", 334),
        ("MODEL_CLAUDE_4_5_HAIKU", 340),
        ("MODEL_CLAUDE_4_5_HAIKU_THINKING", 341),
        ("MODEL_OPENAI_GPT_OSS_120B_MEDIUM", 342),
    ];

    // Sprawdź hardcoded wartości
    for (n, v) in hardcoded {
        if name == *n {
            return *v;
        }
    }

    // MODEL_PLACEHOLDER_M{N} → 1000 + N
    if let Some(suffix) = name.strip_prefix("MODEL_PLACEHOLDER_M") {
        if let Ok(n) = suffix.parse::<u32>() {
            return 1000 + n;
        }
    }

    // Nieznany model — zwróć 0 (będzie odrzucony w stream_chat)
    0
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
    fn test_build_streaming_command_json() {
        let request = build_streaming_command_json(1026, "Say hello");
        // Powinien zawierać commandText "Say hello"
        assert_eq!(request["commandText"], "Say hello");
        // Powinien zawierać requestedModelId 1026
        assert_eq!(request["requestedModelId"], 1026);
        // Powinien zawierać requestSource 16 (CASCADE_CHAT)
        assert_eq!(request["requestSource"], 16);
        // Powinien zawierać document z absoluteUri
        assert!(request["document"]["absoluteUri"].as_str().unwrap().starts_with("file:///"));
    }

    #[test]
    fn test_model_enum_name_to_int() {
        // Znane modele z proto descriptor
        assert_eq!(model_enum_name_to_int("MODEL_CHAT_20706"), 235);
        assert_eq!(model_enum_name_to_int("MODEL_CHAT_23310"), 269);
        assert_eq!(model_enum_name_to_int("MODEL_GOOGLE_GEMINI_2_5_FLASH"), 312);
        assert_eq!(model_enum_name_to_int("MODEL_CLAUDE_4_SONNET"), 281);
        assert_eq!(model_enum_name_to_int("MODEL_CLAUDE_4_OPUS"), 290);
        // Placeholder modele: MODEL_PLACEHOLDER_M{N} → 1000 + N
        assert_eq!(model_enum_name_to_int("MODEL_PLACEHOLDER_M26"), 1026);
        assert_eq!(model_enum_name_to_int("MODEL_PLACEHOLDER_M0"), 1000);
        assert_eq!(model_enum_name_to_int("MODEL_PLACEHOLDER_M100"), 1100);
        // Nieznany model → 0
        assert_eq!(model_enum_name_to_int("unknown"), 0);
    }

    #[test]
    fn test_extract_text_from_connect_response_raw_text() {
        let resp = serde_json::json!({
            "rawText": "Hello from Antigravity!"
        });
        assert_eq!(
            extract_text_from_connect_response(&resp),
            Some("Hello from Antigravity!".to_string())
        );
    }

    #[test]
    fn test_extract_text_from_connect_response_diff() {
        let resp = serde_json::json!({
            "diff": {
                "lines": [
                    {"text": "hello", "type": "UNIFIED_DIFF_LINE_TYPE_UNCHANGED"},
                    {"text": "hi there", "type": "UNIFIED_DIFF_LINE_TYPE_INSERT"}
                ]
            }
        });
        assert_eq!(
            extract_text_from_connect_response(&resp),
            Some("hi there\n".to_string())
        );
    }

    #[test]
    fn test_extract_text_from_connect_response_empty() {
        let resp = serde_json::json!({});
        assert_eq!(extract_text_from_connect_response(&resp), None);
    }

    #[test]
    fn test_extract_param() {
        let text = "CommandLine : language_server.exe --csrf_token abc123 --port 13502";
        assert_eq!(extract_param(text, "--csrf_token"), Some("abc123".to_string()));
        assert_eq!(extract_param(text, "--port"), Some("13502".to_string()));
        assert_eq!(extract_param(text, "--nonexistent"), None);
    }
}
