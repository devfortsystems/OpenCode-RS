use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc::Sender;

use crate::app::AppEvent;
use crate::auth::AuthManager;
use crate::config::AppConfig;
use crate::providers::ProviderRouter;
use crate::quota::QuotaManager;
use crate::workspace::WorkspaceManager;

pub struct WebCompanionServer {
    port: u16,
    work_dir: PathBuf,
    config: AppConfig,
    workspace: Arc<tokio::sync::Mutex<WorkspaceManager>>,
}

impl WebCompanionServer {
    pub fn new(port: u16, work_dir: PathBuf, config: AppConfig) -> Self {
        let workspace = Arc::new(tokio::sync::Mutex::new(
            WorkspaceManager::load_or_create(&work_dir, &config.default_model),
        ));
        Self {
            port,
            work_dir,
            config,
            workspace,
        }
    }

    pub fn with_workspace(
        port: u16,
        work_dir: PathBuf,
        config: AppConfig,
        workspace: Arc<tokio::sync::Mutex<WorkspaceManager>>,
    ) -> Self {
        Self {
            port,
            work_dir,
            config,
            workspace,
        }
    }

    /// Uruchamia asynchroniczny serwer Web Companion w tle
    pub fn start_background(self: Arc<Self>, _event_tx: Sender<AppEvent>) {
        let port = self.port;

        tokio::spawn(async move {
            let addr = format!("0.0.0.0:{}", port);
            let listener = match TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(_) => {
                    // Fallback na 127.0.0.1
                    match TcpListener::bind(format!("127.0.0.1:{}", port)).await {
                        Ok(l) => l,
                        Err(e) => {
                            eprintln!("⚠️ [Web Companion] Nie można uruchomić serwera na porcie {port}: {e}");
                            return;
                        }
                    }
                }
            };

            while let Ok((mut stream, _)) = listener.accept().await {
                let this = self.clone();
                tokio::spawn(async move {
                    let mut buffer = vec![0u8; 65536]; // większy bufor dla uploadów
                    if let Ok(n) = stream.read(&mut buffer).await {
                        if n == 0 {
                            return;
                        }
                        let request = String::from_utf8_lossy(&buffer[..n]);

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
                            let body_bytes = Self::extract_body(&buffer, n, &request);
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
                            let body_bytes = Self::extract_body(&buffer, n, &request);
                            if let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                                if let Some(tab_id) = payload.get("tab_id").and_then(|v| v.as_str()) {
                                    let mut mgr = this.workspace.lock().await;
                                    mgr.switch_tab(tab_id);
                                }
                            }
                            ("HTTP/1.1 200 OK", "application/json", r#"{"status":"ok"}"#.to_string())
                        } else if request.starts_with("POST /api/workspace/tab/close") {
                            let body_bytes = Self::extract_body(&buffer, n, &request);
                            if let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                                if let Some(tab_id) = payload.get("tab_id").and_then(|v| v.as_str()) {
                                    let mut mgr = this.workspace.lock().await;
                                    mgr.close_tab(tab_id);
                                }
                            }
                            ("HTTP/1.1 200 OK", "application/json", r#"{"status":"ok"}"#.to_string())
                        } else if request.starts_with("POST /api/workspace/tab/draft") {
                            let body_bytes = Self::extract_body(&buffer, n, &request);
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
                            let models = Self::get_active_models_catalog(&this.work_dir, &this.config).await;
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
                            let body_bytes = Self::extract_body(&buffer, n, &request);
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
                            let body_bytes = Self::extract_body(&buffer, n, &request);
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
                    }
                });
            }
        });
    }

    /// Pobiera katalog TYLKO działających modeli z cenami i metadanymi
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
