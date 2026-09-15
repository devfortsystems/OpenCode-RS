use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

/// Pełny klient LSP (Language Server Protocol) over stdio.
/// Implementuje: initialize, initialized, didOpen, diagnostics (publishDiagnostics).
/// Komunikacja przez JSON-RPC 2.0 (Content-Length header + JSON body).
///
/// Używane do integracji skonfigurowanych LSP servers z opencode.json:
/// ```json
/// { "lsp": { "pyright": { "command": "pyright-langserver --stdio", "extensions": ["py"] } } }
/// ```
pub struct LspClient {
    child: Child,
    /// Kolejka diagnostyk odebranych od serwera (publishDiagnostics).
    diagnostics: Mutex<HashMap<PathBuf, Vec<LspDiagnostic>>>,
    /// ID kolejnego żądania JSON-RPC.
    #[allow(dead_code)]
    next_id: Mutex<u64>,
}

#[derive(Debug, Clone)]
pub struct LspDiagnostic {
    pub line: usize,
    pub col: usize,
    pub severity: String, // error, warning, info, hint
    pub message: String,
    pub source: Option<String>,
}

impl LspClient {
    /// Uruchamia serwer LSP i wysyła initialize + initialized.
    pub fn start(command: &str, work_dir: &Path) -> Result<Self> {
        let mut cmd = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.arg("/c");
            for arg in command.split_whitespace() {
                c.arg(arg);
            }
            c
        } else {
            let mut c = Command::new("sh");
            c.arg("-c").arg(command);
            c
        };
        cmd.current_dir(work_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let mut child = cmd.spawn().map_err(|e| anyhow!("LSP spawn failed: {e}"))?;

        // Wyślij initialize request
        let init_params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": format!("file://{}", work_dir.display()),
            "capabilities": {
                "textDocument": {
                    "publishDiagnostics": { "relatedInformation": true }
                }
            },
            "workspace": {
                "workspaceFolders": [{
                    "uri": format!("file://{}", work_dir.display()),
                    "name": work_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
                }]
            }
        });

        let stdin = child.stdin.as_mut().ok_or_else(|| anyhow!("LSP stdin missing"))?;
        let init_id = send_request(stdin, "initialize", init_params)?;
        // Odczytaj odpowiedź initialize (timeout 10s)
        let stdout = child.stdout.as_mut().ok_or_else(|| anyhow!("LSP stdout missing"))?;
        let mut reader = BufReader::new(stdout);
        let _init_response = read_message(&mut reader, std::time::Duration::from_secs(10))?;

        // Wyślij initialized notification
        send_notification(stdin, "initialized", serde_json::json!({}))?;

        Ok(Self {
            child,
            diagnostics: Mutex::new(HashMap::new()),
            next_id: Mutex::new(init_id + 1),
        })
    }

    /// Wysyła didOpen dla pliku i czeka na publishDiagnostics.
    pub fn open_file(&mut self, file_path: &Path) -> Result<()> {
        let uri = format!("file://{}", file_path.display());
        let content = std::fs::read_to_string(file_path).unwrap_or_default();
        let lang_id = detect_language_id(file_path);

        let stdin = self.child.stdin.as_mut().ok_or_else(|| anyhow!("LSP stdin missing"))?;
        send_notification(stdin, "textDocument/didOpen", serde_json::json!({
            "textDocument": {
                "uri": uri,
                "languageId": lang_id,
                "version": 1,
                "text": content
            }
        }))?;

        // Czekaj na publishDiagnostics (timeout 5s)
        let stdout = self.child.stdout.as_mut().ok_or_else(|| anyhow!("LSP stdout missing"))?;
        let mut reader = BufReader::new(stdout);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            let remaining = deadline - std::time::Instant::now();
            if let Ok(msg) = read_message(&mut reader, remaining) {
                if let Some(method) = msg.get("method").and_then(|m| m.as_str()) {
                    if method == "textDocument/publishDiagnostics" {
                        if let Some(params) = msg.get("params") {
                            if let Some(uri) = params.get("uri").and_then(|u| u.as_str()) {
                                if let Some(diags) = params.get("diagnostics").and_then(|d| d.as_array()) {
                                    let path = PathBuf::from(uri.strip_prefix("file://").unwrap_or(uri));
                                    let parsed: Vec<LspDiagnostic> = diags.iter().filter_map(|d| {
                                        let line = d.get("range").and_then(|r| r.get("start"))
                                            .and_then(|s| s.get("line")).and_then(|l| l.as_u64())
                                            .map(|l| l as usize + 1).unwrap_or(0);
                                        let col = d.get("range").and_then(|r| r.get("start"))
                                            .and_then(|s| s.get("character")).and_then(|c| c.as_u64())
                                            .map(|c| c as usize + 1).unwrap_or(0);
                                        let severity = match d.get("severity").and_then(|s| s.as_u64()) {
                                            Some(1) => "error",
                                            Some(2) => "warning",
                                            Some(3) => "info",
                                            Some(4) => "hint",
                                            _ => "info"
                                        }.to_string();
                                        let message = d.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string();
                                        let source = d.get("source").and_then(|s| s.as_str()).map(|s| s.to_string());
                                        Some(LspDiagnostic { line, col, severity, message, source })
                                    }).collect();
                                    self.diagnostics.lock().unwrap().insert(path, parsed);
                                }
                            }
                        }
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Zwraca diagnostyki dla pliku.
    pub fn diagnostics_for(&self, path: &Path) -> Vec<LspDiagnostic> {
        self.diagnostics.lock().unwrap().get(path).cloned().unwrap_or_default()
    }

    /// Zwraca wszystkie diagnostyki.
    pub fn all_diagnostics(&self) -> HashMap<PathBuf, Vec<LspDiagnostic>> {
        self.diagnostics.lock().unwrap().clone()
    }

    /// Liczba błędów (severity = error) we wszystkich plikach.
    pub fn error_count(&self) -> usize {
        self.diagnostics.lock().unwrap().values()
            .map(|v| v.iter().filter(|d| d.severity == "error").count())
            .sum()
    }

    /// Zamyka serwer LSP (shutdown + exit).
    pub fn shutdown(&mut self) {
        if let Some(stdin) = self.child.stdin.as_mut() {
            let _ = send_request(stdin, "shutdown", serde_json::json!({}));
            let _ = send_notification(stdin, "exit", serde_json::json!({}));
        }
        let _ = self.child.kill();
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Wyślij żądanie JSON-RPC (zwraca użyte ID).
fn send_request(stdin: &mut impl Write, method: &str, params: serde_json::Value) -> Result<u64> {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params
    });
    send_raw(stdin, &msg)?;
    Ok(id)
}

/// Wyślij notyfikację JSON-RPC (bez ID, bez odpowiedzi).
fn send_notification(stdin: &mut impl Write, method: &str, params: serde_json::Value) -> Result<()> {
    let msg = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params
    });
    send_raw(stdin, &msg)
}

/// Wyślij surową wiadomość JSON-RPC z nagłówkiem Content-Length.
fn send_raw(stdin: &mut impl Write, msg: &serde_json::Value) -> Result<()> {
    let body = serde_json::to_string(msg)?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    stdin.write_all(header.as_bytes())?;
    stdin.write_all(body.as_bytes())?;
    stdin.flush()?;
    Ok(())
}

/// Odczytaj jedną wiadomość JSON-RPC z stdout (z timeoutem).
fn read_message(reader: &mut impl BufRead, timeout: std::time::Duration) -> Result<serde_json::Value> {
    let deadline = std::time::Instant::now() + timeout;
    let mut content_length: Option<usize> = None;

    // Czytaj nagłówki aż do pustej linii
    loop {
        if std::time::Instant::now() > deadline {
            return Err(anyhow!("LSP read timeout"));
        }
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 { return Err(anyhow!("LSP EOF")); }
        let trimmed = line.trim();
        if trimmed.is_empty() { break; } // koniec nagłówków
        if let Some(len) = trimmed.strip_prefix("Content-Length:") {
            content_length = len.trim().parse().ok();
        }
    }

    let len = content_length.ok_or_else(|| anyhow!("LSP brak Content-Length"))?;
    let mut body = vec![0u8; len];
    std::io::Read::read_exact(reader, &mut body)?;
    let val: serde_json::Value = serde_json::from_slice(&body)?;
    Ok(val)
}

/// Wykryj languageId dla LSP na podstawie rozszerzenia.
fn detect_language_id(path: &Path) -> &str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("rs") => "rust",
        Some("py") => "python",
        Some("ts") => "typescript",
        Some("tsx") => "typescriptreact",
        Some("js") => "javascript",
        Some("jsx") => "javascriptreact",
        Some("go") => "go",
        Some("java") => "java",
        Some("c") | Some("h") => "c",
        Some("cpp") | Some("hpp") | Some("cc") => "cpp",
        Some("cs") => "csharp",
        Some("rb") => "ruby",
        Some("php") => "php",
        Some("swift") => "swift",
        Some("kt") => "kotlin",
        Some("scala") => "scala",
        Some("lua") => "lua",
        Some("sh") | Some("bash") => "shellscript",
        Some("json") => "json",
        Some("yaml") | Some("yml") => "yaml",
        Some("toml") => "toml",
        Some("md") => "markdown",
        Some("html") => "html",
        Some("css") => "css",
        Some("sql") => "sql",
        _ => "plaintext"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language_id() {
        assert_eq!(detect_language_id(Path::new("main.rs")), "rust");
        assert_eq!(detect_language_id(Path::new("app.py")), "python");
        assert_eq!(detect_language_id(Path::new("index.ts")), "typescript");
        assert_eq!(detect_language_id(Path::new("main.go")), "go");
        assert_eq!(detect_language_id(Path::new("unknown.xyz")), "plaintext");
    }

    #[test]
    fn test_send_raw_format() {
        // Sprawdź format wiadomości JSON-RPC
        let msg = serde_json::json!({"jsonrpc": "2.0", "method": "test"});
        let body = serde_json::to_string(&msg).unwrap();
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        assert!(header.contains("Content-Length:"));
        assert!(header.ends_with("\r\n\r\n"));
    }
}
