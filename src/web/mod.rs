use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc::Sender, Mutex};

use crate::agent::Agent;
use crate::app::AppEvent;
use crate::auth::AuthManager;
use crate::config::AppConfig;
use crate::providers::ProviderRouter;
use crate::quota::QuotaManager;
use crate::workspace::WorkspaceManager;

type SessionStore = Arc<Mutex<HashMap<String, (tokio::task::AbortHandle, Instant, String)>>>;
type ModelCache = Arc<Mutex<Option<(Instant, Vec<crate::cost::ModelCatalogItem>)>>>;

pub struct WebCompanionServer {
    port: u16,
    work_dir: PathBuf,
    config: AppConfig,
    workspace: Arc<Mutex<WorkspaceManager>>,
    router: Arc<ProviderRouter>,
    sessions: SessionStore,
    models_cache: ModelCache,
}

impl WebCompanionServer {
    pub fn new(port: u16, work_dir: PathBuf, config: AppConfig) -> Self {
        let workspace = Arc::new(Mutex::new(
            WorkspaceManager::load_or_create(&work_dir, &config.default_model),
        ));
        let router = Arc::new(ProviderRouter::new(config.clone(), work_dir.clone()));
        Self {
            port,
            work_dir,
            config,
            workspace,
            router,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            models_cache: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_workspace(
        port: u16,
        work_dir: PathBuf,
        config: AppConfig,
        workspace: Arc<Mutex<WorkspaceManager>>,
    ) -> Self {
        let router = Arc::new(ProviderRouter::new(config.clone(), work_dir.clone()));
        Self {
            port,
            work_dir,
            config,
            workspace,
            router,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            models_cache: Arc::new(Mutex::new(None)),
        }
    }

    async fn read_http_request(
        stream: &mut tokio::net::TcpStream,
    ) -> Result<(String, Vec<u8>), std::io::Error> {
        use std::io::{Error, ErrorKind};

        let mut total: Vec<u8> = Vec::with_capacity(65536);
        let mut header_end: Option<usize> = None;
        let mut small_buf = [0u8; 16384];

        while header_end.is_none() && total.len() < 1_048_576 {
            let n = stream.read(&mut small_buf).await?;
            if n == 0 {
                break;
            }
            total.extend_from_slice(&small_buf[..n]);
            if let Some(pos) = Self::find_subslice(&total, b"\r\n\r\n") {
                header_end = Some(pos + 4);
                break;
            }
        }

        let header_end_idx = match header_end {
            Some(i) => i,
            None => return Err(Error::new(ErrorKind::InvalidData, "Malformed HTTP headers")),
        };

        let headers_str = String::from_utf8_lossy(&total[..header_end_idx]).to_string();

        let mut content_length: Option<usize> = None;
        let mut expect_100 = false;
        for line in headers_str.lines() {
            let lower = line.to_ascii_lowercase();
            if let Some(val) = lower.strip_prefix("content-length:") {
                if let Ok(v) = val.trim().parse::<usize>() {
                    content_length = Some(v);
                }
            }
            if lower.starts_with("expect:") && lower.contains("100-continue") {
                expect_100 = true;
            }
        }

        if expect_100 {
            let _ = stream.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await;
            let _ = stream.flush().await;
        }

        let needed = content_length.unwrap_or(0);
        let already = total.len() - header_end_idx;
        let mut remaining = needed.saturating_sub(already);

        if remaining > 0 {
            total.reserve(remaining);
            let deadline = Instant::now() + Duration::from_secs(15);
            while remaining > 0 {
                if Instant::now() > deadline {
                    return Err(Error::new(ErrorKind::TimedOut, "HTTP body read timeout"));
                }
                let to_read = remaining.min(small_buf.len());
                let n = stream.read(&mut small_buf[..to_read]).await?;
                if n == 0 {
                    break;
                }
                total.extend_from_slice(&small_buf[..n]);
                remaining -= n;
            }
        }

        Ok((headers_str, total))
    }

    /// Uruchamia asynchroniczny serwer Web Companion w tle
    pub fn start_background(self: Arc<Self>, _event_tx: Sender<AppEvent>) {
        let port = self.port;

        tokio::spawn(async move {
            let mut listener: Option<TcpListener> = None;
            let mut actual_port = port;

            for offset in 0..5u16 {
                actual_port = port + offset;
                let addr_all = format!("0.0.0.0:{}", actual_port);
                let addr_local = format!("127.0.0.1:{}", actual_port);

                if let Ok(l) = TcpListener::bind(&addr_all).await {
                    listener = Some(l);
                    if offset > 0 {
                        eprintln!("ℹ️  [Web Companion] Port {port} zajęty — używam portu {actual_port}.");
                    }
                    break;
                }

                if let Ok(l) = TcpListener::bind(&addr_local).await {
                    listener = Some(l);
                    if offset > 0 {
                        eprintln!("ℹ️  [Web Companion] Port {port} zajęty — używam portu {actual_port} (tylko localhost).");
                    }
                    break;
                }
            }

            let listener = match listener {
                Some(l) => l,
                None => {
                    eprintln!("❌ [Web Companion] Nie można uruchomić serwera na żadnym z portów {}-{} — wszystkie zajęte.", port, port + 4);
                    return;
                }
            };

            println!("✅ [OpenCode-RS] Web Companion wystartował na porcie http://127.0.0.1:{}", actual_port);
            eprintln!("🌐 [Web Companion] Dashboard IDE: http://127.0.0.1:{actual_port}");
            eprintln!("🌐 [Web Companion] Status API : http://127.0.0.1:{actual_port}/api/status");
            eprintln!("🌐 [Web Companion] Models API : http://127.0.0.1:{actual_port}/api/models");

            while let Ok((mut stream, _)) = listener.accept().await {
                let this = self.clone();
                tokio::spawn(async move {
                    let (request, full_buf) = match Self::read_http_request(&mut stream).await {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    if request.is_empty() {
                        return;
                    }
                    let full_n = full_buf.len();

                    // ─────────────────────────────────────────────────────────
                    // Specjalny endpoint: POST /api/chat/prompt (SSE streaming)
                    // Nie można zwrócić tupli, bo wysyłamy dane w pętli live.
                    // ─────────────────────────────────────────────────────────
                    if request.starts_with("POST /api/chat/prompt") {
                        let body_bytes = Self::extract_body(&full_buf, full_n, &request);
                            let payload: serde_json::Value = match serde_json::from_slice(&body_bytes) {
                                Ok(v) => v,
                                Err(_) => {
                                    let _ = Self::write_json_response(&mut stream, 400, r#"{"status":"invalid_json"}"#).await;
                                    return;
                                }
                            };
                            let session_id = payload
                                .get("session_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or(&uuid::Uuid::new_v4().to_string())
                                .to_string();
                            let model = payload
                                .get("model")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| this.config.default_model.clone());
                            let prompt = payload
                                .get("prompt")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string();
                            if prompt.trim().is_empty() {
                                let _ = Self::write_json_response(&mut stream, 400, r#"{"status":"empty_prompt"}"#).await;
                                return;
                            }

                            let sessions = this.sessions.clone();
                            let router = this.router.clone();
                            let work_dir = this.work_dir.clone();

                            // Wysyłamy nagłówek SSE (bez Content-Length, bo streaming)
                            let header = "\
HTTP/1.1 200 OK\r\n\
Content-Type: text/event-stream\r\n\
Cache-Control: no-cache\r\n\
Connection: keep-alive\r\n\
Access-Control-Allow-Origin: *\r\n\
Access-Control-Allow-Headers: *\r\n\
\r\n";
                            if stream.write_all(header.as_bytes()).await.is_err() {
                                return;
                            }
                            let _ = stream.flush().await;

                            let started = Instant::now();
                            let (token_tx, mut token_rx) = tokio::sync::mpsc::channel::<String>(512);
                            let (ctx_tx, mut ctx_rx) = tokio::sync::mpsc::channel::<(usize, usize)>(16);

                            let agent_work_dir = work_dir.clone();
                            let agent_model = model.clone();
                            let agent_prompt = prompt.clone();
                            let task = tokio::spawn(async move {
                                let agent = Agent::new(router, agent_work_dir);
                                let history = Vec::new();
                                let res = agent
                                    .process_user_prompt(&agent_model, "default", &history, &agent_prompt, token_tx, ctx_tx)
                                    .await;
                                match res {
                                    Ok(text) => (Some(text), None),
                                    Err(e) => (None, Some(e.to_string())),
                                }
                            });

                            // Zapisz AbortHandle do sesji (do cancel)
                            sessions
                                .lock()
                                .await
                                .insert(session_id.clone(), (task.abort_handle(), started, "running".to_string()));

                            // Pętla forwardowania SSE do klienta
                            let mut final_text: Option<String> = None;
                            let mut final_err: Option<String> = None;
                            let mut ctx_chars: usize = 0;
                            let mut ctx_tokens: usize = 0;

                            loop {
                                tokio::select! {
                                    biased;
                                    Some((chars, tokens)) = ctx_rx.recv() => {
                                        ctx_chars = chars;
                                        ctx_tokens = tokens;
                                        let payload = serde_json::json!({"chars": chars, "tokens": tokens}).to_string();
                                        let chunk = format!("event: context\r\ndata: {}\r\n\r\n", payload);
                                        if stream.write_all(chunk.as_bytes()).await.is_err() { break; }
                                        let _ = stream.flush().await;
                                    }
                                    Some(tok) = token_rx.recv() => {
                                        let payload = serde_json::json!({"token": tok}).to_string();
                                        let chunk = format!("event: token\r\ndata: {}\r\n\r\n", payload);
                                        if stream.write_all(chunk.as_bytes()).await.is_err() { break; }
                                        let _ = stream.flush().await;
                                    }
                                    else => break,
                                }
                            }

                            // Oczekaj na wynik taska (albo aborted, albo finished)
                            match task.await {
                                Ok((Some(text), None)) => {
                                    final_text = Some(text);
                                }
                                Ok((None, Some(err))) => {
                                    final_err = Some(err);
                                }
                                Ok((_, _)) => {}
                                Err(join_err) if join_err.is_cancelled() => {
                                    final_err = Some("Session cancelled by user (session/cancel).".to_string());
                                }
                                Err(join_err) => {
                                    final_err = Some(format!("Task error: {}", join_err));
                                }
                            }

                            let elapsed_ms = started.elapsed().as_millis() as u64;
                            let done_payload = if let Some(err) = final_err {
                                serde_json::json!({
                                    "status": "error",
                                    "message": err,
                                    "elapsed_ms": elapsed_ms,
                                    "context": {"chars": ctx_chars, "tokens": ctx_tokens}
                                })
                            } else {
                                serde_json::json!({
                                    "status": "ok",
                                    "final_text": final_text.unwrap_or_default(),
                                    "model": model,
                                    "elapsed_ms": elapsed_ms,
                                    "context": {"chars": ctx_chars, "tokens": ctx_tokens}
                                })
                            };
                            let chunk = format!("event: done\r\ndata: {}\r\n\r\n", done_payload.to_string());
                            let _ = stream.write_all(chunk.as_bytes()).await;
                            let _ = stream.flush().await;

                            // Usuń sesję po zakończeniu
                            sessions.lock().await.remove(&session_id);
                            return;
                        }

                        let (status_line, content_type, body) = if request.starts_with("GET /api/status") {
                            let json = format!(
                                r#"{{"status":"online","port":{},"project":"{}","version":"1.18.25"}}"#,
                                this.port,
                                this.work_dir.file_name().unwrap_or_default().to_string_lossy()
                            );
                            ("HTTP/1.1 200 OK", "application/json", json)
                        } else if request.starts_with("GET /api/config") {
                            let json = serde_json::to_string(&this.config).unwrap_or_else(|_| "{}".to_string());
                            ("HTTP/1.1 200 OK", "application/json", json)
                        } else if request.starts_with("GET /api/workspace") {
                            let mgr = this.workspace.lock().await;
                            let json = serde_json::to_string(&mgr.state).unwrap_or_else(|_| "{}".to_string());
                            ("HTTP/1.1 200 OK", "application/json", json)
                        } else if request.starts_with("POST /api/workspace/tab/new") {
                            let body_bytes = Self::extract_body(&full_buf, full_n, &request);
                            let mut tab_id = String::new();
                            if let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                                let title = payload.get("title").and_then(|v| v.as_str());
                                let path_str = payload.get("project_path").and_then(|v| v.as_str()).unwrap_or("");
                                let proj_path = if path_str.is_empty() {
                                    this.work_dir.clone()
                                } else {
                                    PathBuf::from(path_str)
                                };
                                let model = payload.get("model").and_then(|v| v.as_str()).unwrap_or(&this.config.default_model);

                                let mut mgr = this.workspace.lock().await;
                                tab_id = mgr.new_tab(title, proj_path, model);
                            }
                            let json = format!(r#"{{"status":"ok","tab_id":"{}"}}"#, tab_id);
                            ("HTTP/1.1 200 OK", "application/json", json)
                        } else if request.starts_with("POST /api/workspace/tab/switch") {
                            let body_bytes = Self::extract_body(&full_buf, full_n, &request);
                            if let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                                if let Some(tab_id) = payload.get("tab_id").and_then(|v| v.as_str()) {
                                    let mut mgr = this.workspace.lock().await;
                                    mgr.switch_tab(tab_id);
                                }
                            }
                            ("HTTP/1.1 200 OK", "application/json", r#"{"status":"ok"}"#.to_string())
                        } else if request.starts_with("POST /api/workspace/tab/close") {
                            let body_bytes = Self::extract_body(&full_buf, full_n, &request);
                            if let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                                if let Some(tab_id) = payload.get("tab_id").and_then(|v| v.as_str()) {
                                    let mut mgr = this.workspace.lock().await;
                                    mgr.close_tab(tab_id);
                                }
                            }
                            ("HTTP/1.1 200 OK", "application/json", r#"{"status":"ok"}"#.to_string())
                        } else if request.starts_with("POST /api/workspace/tab/draft") {
                            let body_bytes = Self::extract_body(&full_buf, full_n, &request);
                            if let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                                if let (Some(tab_id), Some(draft)) = (
                                    payload.get("tab_id").and_then(|v| v.as_str()),
                                    payload.get("draft_prompt").and_then(|v| v.as_str()),
                                ) {
                                    let mut mgr = this.workspace.lock().await;
                                    mgr.update_draft(tab_id, draft);
                                }
                            }
                            ("HTTP/1.1 200 OK", "application/json", r#"{"status":"ok"}"#.to_string())
                        } else if request.starts_with("GET /api/quotas") {
                            AuthManager::auto_load_credentials(&this.work_dir);
                            let auth_keys = AuthManager::get_active_keys();
                            let quotas = QuotaManager::check_all_quotas(&auth_keys, 0).await;
                            let json = serde_json::to_string(&quotas).unwrap_or_else(|_| "[]".to_string());
                            ("HTTP/1.1 200 OK", "application/json", json)
                        } else if request.starts_with("GET /api/models") {
                            let force = Self::extract_query_param(&request, "refresh")
                                .map(|s| s == "1" || s.eq_ignore_ascii_case("true"))
                                .unwrap_or(false);
                            let models = this
                                .get_active_models_catalog_cached(force)
                                .await;
                            let json = serde_json::to_string(&models).unwrap_or_else(|_| "[]".to_string());
                            ("HTTP/1.1 200 OK", "application/json", json)
                        } else if request.starts_with("GET /api/fs/tree") {
                            // Wyciągnij parametr ?path=...
                            let target_path = Self::extract_query_param(&request, "path")
                                .map(PathBuf::from)
                                .unwrap_or_else(|| this.work_dir.clone());
                            let tree = Self::build_fs_tree(&target_path, 2);
                            let json = serde_json::to_string(&tree).unwrap_or_else(|_| "[]".to_string());
                            ("HTTP/1.1 200 OK", "application/json", json)
                        } else if request.starts_with("GET /api/fs/file") {
                            if let Some(file_path_str) = Self::extract_query_param(&request, "path") {
                                let path = PathBuf::from(&file_path_str);
                                if path.exists() && path.is_file() {
                                    match std::fs::read_to_string(&path) {
                                        Ok(content) => {
                                            let resp_json = serde_json::json!({
                                                "status": "ok",
                                                "path": file_path_str,
                                                "content": content
                                            });
                                            ("HTTP/1.1 200 OK", "application/json", resp_json.to_string())
                                        }
                                        Err(e) => {
                                            let err = serde_json::json!({"status": "error", "message": e.to_string()});
                                            ("HTTP/1.1 500 Internal Error", "application/json", err.to_string())
                                        }
                                    }
                                } else {
                                    ("HTTP/1.1 404 Not Found", "application/json", r#"{"status":"not_found"}"#.to_string())
                                }
                            } else {
                                ("HTTP/1.1 400 Bad Request", "application/json", r#"{"status":"missing_path"}"#.to_string())
                            }
                        } else if request.starts_with("POST /api/fs/file") || request.starts_with("POST /api/fs/save") {
                            let body_bytes = Self::extract_body(&full_buf, full_n, &request);
                            if let Ok(body_str) = std::str::from_utf8(&body_bytes) {
                                if let Ok(payload) = serde_json::from_str::<serde_json::Value>(body_str) {
                                    if let (Some(path_str), Some(content)) = (
                                        payload.get("path").and_then(|v| v.as_str()),
                                        payload.get("content").and_then(|v| v.as_str()),
                                    ) {
                                        let path = PathBuf::from(path_str);
                                        match std::fs::write(&path, content) {
                                            Ok(_) => {
                                                let json = serde_json::json!({
                                                    "status": "ok",
                                                    "path": path_str,
                                                    "bytes_written": content.len()
                                                });
                                                ("HTTP/1.1 200 OK", "application/json", json.to_string())
                                            }
                                            Err(e) => {
                                                let json = serde_json::json!({"status": "error", "message": e.to_string()});
                                                ("HTTP/1.1 500 Internal Error", "application/json", json.to_string())
                                            }
                                        }
                                    } else {
                                        ("HTTP/1.1 400 Bad Request", "application/json", r#"{"status":"missing_fields"}"#.to_string())
                                    }
                                } else {
                                    ("HTTP/1.1 400 Bad Request", "application/json", r#"{"status":"invalid_json"}"#.to_string())
                                }
                            } else {
                                ("HTTP/1.1 400 Bad Request", "application/json", r#"{"status":"invalid_utf8"}"#.to_string())
                            }
                        } else if request.starts_with("POST /api/upload") {
                            let body_bytes = Self::extract_body(&full_buf, full_n, &request);
                            match Self::handle_upload(&body_bytes, &request) {
                                Ok(msg) => {
                                    let json = format!(r#"{{"status":"ok","message":"{}","files":{}}}"#, msg.replace('"', "\\\""), "[]");
                                    ("HTTP/1.1 200 OK", "application/json", json)
                                }
                                Err(e) => {
                                    let json = format!(r#"{{"status":"error","message":"{}"}}"#, e.to_string().replace('"', "\\\""));
                                    ("HTTP/1.1 400 Bad Request", "application/json", json)
                                }
                            }
                        } else if request.starts_with("POST /api/cancel") {
                            let body_bytes = Self::extract_body(&full_buf, full_n, &request);
                            let (status_line, json) = match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                                Ok(payload) => {
                                    if let Some(session_id) = payload.get("session_id").and_then(|v| v.as_str()) {
                                        let mut guard = this.sessions.lock().await;
                                        if let Some((abort_handle, started, _status)) = guard.remove(session_id) {
                                            abort_handle.abort();
                                            let elapsed_ms = started.elapsed().as_millis() as u64;
                                            ("HTTP/1.1 200 OK", format!(r#"{{"status":"ok","cancelled":true,"session_id":"{}","elapsed_ms":{}}}"#, session_id, elapsed_ms))
                                        } else {
                                            ("HTTP/1.1 200 OK", format!(r#"{{"status":"ok","cancelled":false,"reason":"not_found","session_id":"{}"}}"#, session_id))
                                        }
                                    } else {
                                        ("HTTP/1.1 400 Bad Request", r#"{"status":"error","message":"missing session_id"}"#.to_string())
                                    }
                                }
                                Err(_) => ("HTTP/1.1 400 Bad Request", r#"{"status":"invalid_json"}"#.to_string()),
                            };
                            (status_line, "application/json", json)
                        } else if request.starts_with("POST /api/tools/bash_exec") || request.starts_with("POST /api/terminal/bash") {
                            let body_bytes = Self::extract_body(&full_buf, full_n, &request);
                            let (status_line, json_resp) = match serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                                Ok(payload) => {
                                    let command = payload.get("command").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                                    if command.trim().is_empty() {
                                        ("HTTP/1.1 400 Bad Request", r#"{"status":"error","message":"empty command"}"#.to_string())
                                    } else {
                                        let cwd = payload.get("cwd").and_then(|v| v.as_str()).map(PathBuf::from);
                                        let timeout_ms = payload
                                            .get("timeout_ms")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(30_000);
                                        let result = Self::run_bash(command, cwd.as_deref(), timeout_ms, &this.work_dir).await;
                                        ("HTTP/1.1 200 OK", result.to_string())
                                    }
                                }
                                Err(_) => ("HTTP/1.1 400 Bad Request", r#"{"status":"invalid_json"}"#.to_string()),
                            };
                            (status_line, "application/json", json_resp)
                        } else {
                            // Domyślnie serwuj dashboard Web Companion Single Page App
                            let html = include_str!("dashboard.html");
                            ("HTTP/1.1 200 OK", "text/html; charset=utf-8", html.to_string())
                        };

                        let response = format!(
                            "{}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\nConnection: close\r\n\r\n{}",
                            status_line,
                            content_type,
                            body.len(),
                            body
                        );

                        let _ = stream.write_all(response.as_bytes()).await;
                        let _ = stream.flush().await;
                });
            }
        });
    }

    /// Pomocnicza: wysyła statyczną odpowiedź JSON bezpośrednio do streamu (dla błędów SSE endpointu)
    async fn write_json_response(stream: &mut tokio::net::TcpStream, code: u16, body: &str) -> std::io::Result<()> {
        let status = match code {
            200 => "HTTP/1.1 200 OK",
            400 => "HTTP/1.1 400 Bad Request",
            404 => "HTTP/1.1 404 Not Found",
            500 => "HTTP/1.1 500 Internal Server Error",
            _ => "HTTP/1.1 200 OK",
        };
        let header = format!(
            "{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
            status,
            body.len()
        );
        stream.write_all(header.as_bytes()).await?;
        stream.write_all(body.as_bytes()).await?;
        stream.flush().await
    }

    /// Wykonuje komendę bash/system w subprocessie z timeoutem.
    /// Windows: cmd /C "<command>". Unix: sh -c "<command>".
    async fn run_bash(command: String, cwd: Option<&Path>, timeout_ms: u64, work_dir: &Path) -> serde_json::Value {
        use std::process::{Command, Stdio};

        let cmd_clone = command.clone();
        let work_dir = work_dir.to_path_buf();
        let cwd_owned = cwd.map(|p| p.to_path_buf());

        let spawned = tokio::task::spawn_blocking(move || {
            let t0 = Instant::now();
            let effective_cwd = cwd_owned.unwrap_or(work_dir);
            let mut cmd = if cfg!(windows) {
                let mut c = Command::new("cmd");
                c.args(["/C", &cmd_clone]);
                c
            } else {
                let mut c = Command::new("sh");
                c.args(["-c", &cmd_clone]);
                c
            };
            cmd.current_dir(&effective_cwd)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            match cmd.output() {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let exit_code = output.status.code().unwrap_or(-1);
                    serde_json::json!({
                        "status": "ok",
                        "command": cmd_clone,
                        "cwd": effective_cwd,
                        "stdout": stdout,
                        "stderr": stderr,
                        "exit_code": exit_code,
                        "elapsed_ms": t0.elapsed().as_millis() as u64,
                    })
                }
                Err(e) => serde_json::json!({
                    "status": "error",
                    "command": cmd_clone,
                    "cwd": effective_cwd,
                    "message": e.to_string(),
                    "elapsed_ms": t0.elapsed().as_millis() as u64,
                }),
            }
        });

        match tokio::time::timeout(Duration::from_millis(timeout_ms), spawned).await {
            Ok(Ok(v)) => v,
            Ok(Err(join_err)) => serde_json::json!({
                "status": "error",
                "command": command,
                "message": format!("spawn_blocking join error: {}", join_err),
                "exit_code": -2,
                "elapsed_ms": timeout_ms,
            }),
            Err(_elapsed) => serde_json::json!({
                "status": "timeout",
                "command": command,
                "message": format!("timeout after {} ms", timeout_ms),
                "exit_code": -124,
                "elapsed_ms": timeout_ms,
            }),
        }
    }

    /// Web Companion — szybki katalog modeli z cache i TTL (nowa domyślna metoda).
    ///
    /// Zastępuje poprzednią `get_active_models_catalog(work_dir, config)` która na KAŻDYM
    /// requeście tworzyła NOWY ProviderRouter i czekała na `discover_models()` (5–30 s,
    /// subprocessy kilo/opencode CLI). Przez to onMounted refreshAll() wisiał, modelCatalog
    /// był pusty, a w dropdownie menu "nie było aktualnych modeli".
    ///
    /// Strategia FAST-FIRST + BACKGROUND-FULL:
    /// 1. Cache `models_cache: (Instant, Vec)` z TTL 60 s — jeśli valid → zwróć natychmiast.
    /// 2. Jeśli cache pusty LUB `force=true`:
    ///    a. Zwróć natychmiast `build_catalog(with_dynamic=false)` — tylko statyczne
    ///       get_available_models + health checki (<300 ms).
    ///    b. Jednocześnie spawn w tle build_catalog(with_dynamic=true) + po wykonaniu
    ///       zapisz do cache. Kolejne requesty / refresh dostaną pełny katalog.
    pub async fn get_active_models_catalog_cached(
        self: &Arc<Self>,
        force: bool,
    ) -> Vec<crate::cost::ModelCatalogItem> {
        const TTL_SECS: u64 = 60;

        if !force {
            let guard = self.models_cache.lock().await;
            if let Some((stamp, items)) = guard.as_ref() {
                if stamp.elapsed().as_secs() < TTL_SECS {
                    return items.clone();
                }
            }
        }

        let fast = self.build_catalog(false).await;
        let this = Arc::clone(self);
        let do_full = force || self.models_cache.lock().await.is_none();
        if do_full {
            tokio::spawn(async move {
                let full = this.build_catalog(true).await;
                let mut guard = this.models_cache.lock().await;
                *guard = Some((Instant::now(), full));
            });
        }

        let mut guard = self.models_cache.lock().await;
        if guard.is_none() {
            *guard = Some((Instant::now(), fast.clone()));
        }
        fast
    }

    /// Rdzeń budowania katalogu modeli — wspólny dla fast i full trybu.
    /// Używa już istniejącego `self.router` (nie tworzy nowego!).
    async fn build_catalog(
        &self,
        with_dynamic: bool,
    ) -> Vec<crate::cost::ModelCatalogItem> {
        let cli_map = ProviderRouter::detect_cli_providers();
        let auth_keys = AuthManager::get_active_keys();
        let antigravity_ok = crate::providers::antigravity::AntigravityProvider::is_available().await;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(500))
            .build()
            .unwrap_or_default();
        let bridge_ok = client
            .get(format!("{}/health", self.config.bridge_url.trim_end_matches('/')))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        let ollama_ok = client
            .get("http://127.0.0.1:11434/api/tags")
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        let lmstudio_ok = client
            .get("http://127.0.0.1:1234/v1/models")
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false);

        let mut all_models: Vec<(String, String, String)> = self
            .router
            .get_available_models()
            .into_iter()
            .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
            .collect();

        if with_dynamic {
            let dynamic = tokio::time::timeout(
                Duration::from_secs(12),
                self.router.discover_models(),
            )
            .await
            .unwrap_or_default();
            let seen: std::collections::HashSet<String> =
                all_models.iter().map(|(id, _, _)| id.clone()).collect();
            for (id, name, prov) in dynamic {
                if !seen.contains(&id) {
                    all_models.push((id, name, prov));
                }
            }
        }

        let config = &self.config;
        let active_models: Vec<(String, String, String)> = all_models
            .into_iter()
            .filter(|(id, _, prov)| {
                let p = prov.to_lowercase();
                let m = id.to_lowercase();
                if p == "antigravity" || m.contains("antigravity") {
                    return antigravity_ok;
                }
                if let Some(binary) = ProviderRouter::cli_binary_for_provider(prov) {
                    return *cli_map.get(binary).unwrap_or(&false);
                }
                if p.contains("kilo") {
                    return *cli_map.get("kilo").unwrap_or(&false);
                }
                if p.contains("opencode") {
                    return *cli_map.get("opencode").unwrap_or(&false);
                }
                if p == "openai" {
                    return auth_keys.contains_key("openai") || config.direct_openai_api_key.is_some();
                }
                if p == "anthropic" {
                    return auth_keys.contains_key("anthropic")
                        || config.direct_anthropic_api_key.is_some();
                }
                if p == "gemini" {
                    return auth_keys.contains_key("gemini") || config.direct_gemini_api_key.is_some();
                }
                if p == "openrouter" {
                    return auth_keys.contains_key("openrouter")
                        || config.direct_openrouter_api_key.is_some();
                }
                if p == "deepseek" {
                    return auth_keys.contains_key("deepseek")
                        || config.direct_deepseek_api_key.is_some();
                }
                if p == "groq" {
                    return auth_keys.contains_key("groq") || config.direct_groq_api_key.is_some();
                }
                if p == "mistral" {
                    return auth_keys.contains_key("mistral")
                        || config.direct_mistral_api_key.is_some();
                }
                if p == "devin-cloud" {
                    return auth_keys.contains_key("devin") || config.devin_api_key.is_some();
                }
                if p == "ollama" || m.contains("ollama") {
                    return ollama_ok;
                }
                if p == "lmstudio" || m.contains("lmstudio") {
                    return lmstudio_ok;
                }
                if p == "cursor"
                    || p == "windsurf"
                    || p == "trae"
                    || p == "copilot"
                    || p == "amazon-q"
                    || p == "augment"
                {
                    return bridge_ok;
                }
                if p == "commandcode" {
                    return bridge_ok
                        || *cli_map.get("commandcode").unwrap_or(&false)
                        || auth_keys.contains_key("commandcode");
                }
                false
            })
            .collect();

        let mut catalog_items: Vec<crate::cost::ModelCatalogItem> = active_models
            .iter()
            .map(|(id, name, prov)| crate::cost::CostEstimator::get_model_catalog_item(id, name, prov))
            .collect();

        catalog_items.sort_by(|a, b| {
            a.cost_per_1m_input
                .partial_cmp(&b.cost_per_1m_input)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        catalog_items
    }

    /// (Pomocnicza / wsteczna kompatybilność) — nie używaj w Web Companion API.
    #[allow(dead_code)]
    pub async fn get_active_models_catalog(work_dir: &Path, config: &AppConfig) -> Vec<crate::cost::ModelCatalogItem> {
        let router = Arc::new(ProviderRouter::new(config.clone(), work_dir.to_path_buf()));
        let cli_map = ProviderRouter::detect_cli_providers();
        let auth_keys = AuthManager::get_active_keys();
        let antigravity_ok = crate::providers::antigravity::AntigravityProvider::is_available().await;

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(500))
            .build()
            .unwrap_or_default();
        let bridge_ok = client.get(format!("{}/health", config.bridge_url.trim_end_matches('/'))).send().await.map(|r| r.status().is_success()).unwrap_or(false);
        let ollama_ok = client.get("http://127.0.0.1:11434/api/tags").send().await.map(|r| r.status().is_success()).unwrap_or(false);
        let lmstudio_ok = client.get("http://127.0.0.1:1234/v1/models").send().await.map(|r| r.status().is_success()).unwrap_or(false);

        let mut all_models: Vec<(String, String, String)> = router
            .get_available_models()
            .into_iter()
            .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
            .collect();

        let dynamic = router.discover_models().await;
        let seen: std::collections::HashSet<String> = all_models.iter().map(|(id, _, _)| id.clone()).collect();
        for (id, name, prov) in dynamic {
            if !seen.contains(&id) {
                all_models.push((id, name, prov));
            }
        }

        let active_models: Vec<(String, String, String)> = all_models
            .into_iter()
            .filter(|(id, _, prov)| {
                let p = prov.to_lowercase();
                let m = id.to_lowercase();
                if p == "antigravity" || m.contains("antigravity") {
                    return antigravity_ok;
                }
                if let Some(binary) = ProviderRouter::cli_binary_for_provider(prov) {
                    return *cli_map.get(binary).unwrap_or(&false);
                }
                if p.contains("kilo") {
                    return *cli_map.get("kilo").unwrap_or(&false);
                }
                if p.contains("opencode") {
                    return *cli_map.get("opencode").unwrap_or(&false);
                }
                if p == "openai" {
                    return auth_keys.contains_key("openai") || config.direct_openai_api_key.is_some();
                }
                if p == "anthropic" {
                    return auth_keys.contains_key("anthropic") || config.direct_anthropic_api_key.is_some();
                }
                if p == "gemini" {
                    return auth_keys.contains_key("gemini") || config.direct_gemini_api_key.is_some();
                }
                if p == "openrouter" {
                    return auth_keys.contains_key("openrouter") || config.direct_openrouter_api_key.is_some();
                }
                if p == "deepseek" {
                    return auth_keys.contains_key("deepseek") || config.direct_deepseek_api_key.is_some();
                }
                if p == "groq" {
                    return auth_keys.contains_key("groq") || config.direct_groq_api_key.is_some();
                }
                if p == "mistral" {
                    return auth_keys.contains_key("mistral") || config.direct_mistral_api_key.is_some();
                }
                if p == "devin-cloud" {
                    return auth_keys.contains_key("devin") || config.devin_api_key.is_some();
                }
                if p == "ollama" || m.contains("ollama") {
                    return ollama_ok;
                }
                if p == "lmstudio" || m.contains("lmstudio") {
                    return lmstudio_ok;
                }
                if p == "cursor" || p == "windsurf" || p == "trae" || p == "copilot" || p == "amazon-q" || p == "augment" {
                    return bridge_ok;
                }
                if p == "commandcode" {
                    return bridge_ok || *cli_map.get("commandcode").unwrap_or(&false) || auth_keys.contains_key("commandcode");
                }
                false
            })
            .collect();

        let mut catalog_items: Vec<crate::cost::ModelCatalogItem> = active_models
            .iter()
            .map(|(id, name, prov)| crate::cost::CostEstimator::get_model_catalog_item(id, name, prov))
            .collect();

        catalog_items.sort_by(|a, b| {
            a.cost_per_1m_input.partial_cmp(&b.cost_per_1m_input).unwrap_or(std::cmp::Ordering::Equal)
        });

        catalog_items
    }

    /// Wyciąga wartość parametru query (np. path z /api/fs/tree?path=C%3A%5C...)
    fn extract_query_param(request: &str, param: &str) -> Option<String> {
        let first_line = request.lines().next()?;
        let query_start = first_line.find('?')?;
        let query_end = first_line[query_start..].find(' ').unwrap_or(first_line.len() - query_start) + query_start;
        let query = &first_line[query_start + 1..query_end];

        for pair in query.split('&') {
            let mut parts = pair.split('=');
            if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
                if k == param {
                    // Prosty url decode
                    let decoded = v.replace("%20", " ").replace("%2F", "/").replace("%5C", "\\").replace("%3A", ":");
                    return Some(decoded);
                }
            }
        }
        None
    }

    /// Buduje drzewo katalogów dla zadanego folderu
    fn build_fs_tree(dir: &Path, max_depth: usize) -> Vec<serde_json::Value> {
        let mut items = Vec::new();
        if max_depth == 0 || !dir.exists() || !dir.is_dir() {
            return items;
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            entries.sort_by_key(|e| (e.path().is_file(), e.file_name()));

            for entry in entries {
                let name = entry.file_name().to_string_lossy().to_string();
                // Ignoruj foldery systemowe i VCS
                if name.starts_with('.') && name != ".opencode-rs" {
                    continue;
                }
                if name == "target" || name == "node_modules" {
                    continue;
                }

                let path = entry.path();
                let is_dir = path.is_dir();
                let children = if is_dir && max_depth > 1 {
                    Self::build_fs_tree(&path, max_depth - 1)
                } else {
                    Vec::new()
                };

                items.push(serde_json::json!({
                    "label": name,
                    "path": path.to_string_lossy(),
                    "is_dir": is_dir,
                    "children": children
                }));
            }
        }

        items
    }

    /// Wyciąga body z requestu HTTP (po \r\n\r\n).
    fn extract_body(buffer: &[u8], n: usize, request: &str) -> Vec<u8> {
        if let Some(idx) = request.find("\r\n\r\n") {
            buffer[idx + 4..n].to_vec()
        } else {
            Vec::new()
        }
    }

    /// Obsługa multipart/form-data upload — zapisuje pliki do dropzone.
    fn handle_upload(body: &[u8], request: &str) -> Result<String, std::io::Error> {
        let boundary = request
            .lines()
            .find(|l| l.to_lowercase().starts_with("content-type:"))
            .and_then(|l| l.split("boundary=").nth(1))
            .map(|b| b.trim().trim_matches('"').to_string())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Brak boundary"))?;

        let dropzone = crate::transfer::dropzone_dir();
        std::fs::create_dir_all(&dropzone)?;

        let boundary_bytes = format!("--{boundary}").into_bytes();
        let mut files_saved = 0;
        let mut pos = 0;

        while pos < body.len() {
            let next = Self::find_subslice(&body[pos..], &boundary_bytes);
            if next.is_none() {
                break;
            }
            let start = pos + next.unwrap() + boundary_bytes.len();
            let part_start = if start + 2 <= body.len() && body[start..start + 2] == *b"\r\n" { start + 2 } else { start };

            let next_boundary = Self::find_subslice(&body[part_start..], &boundary_bytes);
            let part_end = if let Some(ne) = next_boundary {
                let abs_end = part_start + ne;
                if abs_end >= 2 && body[abs_end - 2..abs_end] == *b"\r\n" { abs_end - 2 } else { abs_end }
            } else {
                body.len()
            };

            let part = &body[part_start..part_end];
            let part_str = String::from_utf8_lossy(part);
            if let Some(hdr_end) = part_str.find("\r\n\r\n") {
                let headers = &part_str[..hdr_end];
                let content = &part[hdr_end + 4..];

                let filename = headers
                    .split(';')
                    .find_map(|seg| {
                        let seg = seg.trim();
                        if seg.starts_with("filename=") {
                            Some(seg[9..].trim_matches('"').to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| format!("upload_{}", std::time::SystemTime::now().elapsed().unwrap_or_default().as_millis()));

                if !filename.is_empty() && !content.is_empty() {
                    let safe_name = std::path::Path::new(&filename)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or(filename);
                    let dest = dropzone.join(&safe_name);
                    std::fs::write(&dest, content)?;
                    files_saved += 1;
                }
            }

            pos = part_end;
        }

        Ok(format!("Zapisano {files_saved} plik(ów) do dropzone"))
    }

    fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() || haystack.len() < needle.len() {
            return None;
        }
        haystack.windows(needle.len()).position(|w| w == needle)
    }
}
