use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    #[serde(rename = "mcpServers", default)]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

pub struct McpManager {
    pub config: McpConfig,
    pub config_path: Option<PathBuf>,
    pub work_dir: PathBuf,
}

impl McpManager {
    pub fn load_from_project_or_global(work_dir: &Path) -> Self {
        // 1. Sprawdź .opencode-rs/mcp.json (zapis), potem legacy .opencode/mcp.json
        let project_mcp_rs = work_dir.join(".opencode-rs").join("mcp.json");
        if project_mcp_rs.exists() {
            if let Ok(cfg) = Self::load_file(&project_mcp_rs) {
                return Self {
                    config: cfg,
                    config_path: Some(project_mcp_rs),
                    work_dir: work_dir.to_path_buf(),
                };
            }
        }
        let project_mcp = work_dir.join(".opencode").join("mcp.json");
        if project_mcp.exists() {
            if let Ok(cfg) = Self::load_file(&project_mcp) {
                return Self {
                    config: cfg,
                    config_path: Some(project_mcp),
                    work_dir: work_dir.to_path_buf(),
                };
            }
        }

        // 2. Sprawdź mcp_servers.json w projekcie
        let project_mcp_servers = work_dir.join("mcp_servers.json");
        if project_mcp_servers.exists() {
            if let Ok(cfg) = Self::load_file(&project_mcp_servers) {
                return Self {
                    config: cfg,
                    config_path: Some(project_mcp_servers),
                    work_dir: work_dir.to_path_buf(),
                };
            }
        }

        // 3. Sprawdź globalny ~/.config/opencode/mcp.json
        if let Some(base_dirs) = directories::BaseDirs::new() {
            let global_mcp = base_dirs.config_dir().join("opencode").join("mcp.json");
            if global_mcp.exists() {
                if let Ok(cfg) = Self::load_file(&global_mcp) {
                    return Self {
                        config: cfg,
                        config_path: Some(global_mcp),
                        work_dir: work_dir.to_path_buf(),
                    };
                }
            }
        }

        Self {
            config: McpConfig::default(),
            config_path: None,
            work_dir: work_dir.to_path_buf(),
        }
    }

    fn load_file(path: &Path) -> Result<McpConfig> {
        let content = fs::read_to_string(path)?;
        let parsed: McpConfig = serde_json::from_str(&content)?;
        Ok(parsed)
    }

    pub fn get_status_report(&self) -> String {
        if self.config.mcp_servers.is_empty() {
            let mut report = "🔌 Model Context Protocol (MCP):\nBrak skonfigurowanych serwerów MCP.\n\nAby dodać serwery (np. PostgreSQL, SQLite, GitHub, Playwright):\nUtwórz plik `.opencode-rs/mcp.json` z konfiguracją:\n```json\n{\n  \"mcpServers\": {\n    \"sqlite\": {\n      \"command\": \"npx\",\n      \"args\": [\"-y\", \"@modelcontextprotocol/server-sqlite\", \"--db-path\", \"test.db\"]\n    }\n  }\n}\n```".to_string();
            if let Some(ref p) = self.config_path {
                report.push_str(&format!("\nSzukano w: {}", p.display()));
            }
            return report;
        }

        let mut out = format!("🔌 Zarejestrowane serwery MCP (znaleziono {}):\n\n", self.config.mcp_servers.len());
        for (name, server) in &self.config.mcp_servers {
            out.push_str(&format!("• **{}**: `{} {}`\n", name, server.command, server.args.join(" ")));
            if !server.env.is_empty() {
                out.push_str(&format!("  Zmienne środowiskowe: {}\n", server.env.keys().cloned().collect::<Vec<_>>().join(", ")));
            }
        }

        if let Some(ref p) = self.config_path {
            out.push_str(&format!("\nKonfiguracja załadowana z: {}", p.display()));
        }

        out
    }

    /// Wykonuje wywołanie narzędzia na zdefiniowanym serwerze MCP przez stdio (JSON-RPC)
    pub fn execute_mcp_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<String> {
        let server_cfg = self
            .config
            .mcp_servers
            .get(server_name)
            .ok_or_else(|| anyhow!("Serwer MCP '{}' nie istnieje w konfiguracji", server_name))?;

        let mut cmd = Command::new(&server_cfg.command);
        cmd.args(&server_cfg.args);
        cmd.current_dir(&self.work_dir);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        for (k, v) in &server_cfg.env {
            cmd.env(k, v);
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow!("Nie można uruchomić serwera MCP '{}' ({}): {e}", server_name, server_cfg.command))?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Brak dostępu do stdin procesu MCP"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Brak dostępu do stdout procesu MCP"))?;

        // 1. Wyślij 'initialize' JSON-RPC
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "opencode-rs",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });
        writeln!(stdin, "{}", serde_json::to_string(&init_req)?)?;
        stdin.flush()?;

        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let _ = reader.read_line(&mut line);

        // 2. Wyślij powiadomienie 'notifications/initialized'
        let notify = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        writeln!(stdin, "{}", serde_json::to_string(&notify)?)?;
        stdin.flush()?;

        // 3. Wyślij właściwe wywołanie 'tools/call'
        let call_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });
        writeln!(stdin, "{}", serde_json::to_string(&call_req)?)?;
        stdin.flush()?;

        // Odczytaj odpowiedź JSON-RPC
        line.clear();
        let mut result_line = String::new();
        while reader.read_line(&mut line)? > 0 {
            let trimmed = line.trim();
            if trimmed.starts_with('{') && trimmed.contains("\"id\":2") {
                result_line = trimmed.to_string();
                break;
            }
            line.clear();
        }

        let _ = child.kill();

        if result_line.is_empty() {
            return Err(anyhow!("Brak odpowiedzi JSON-RPC od serwera MCP '{}'", server_name));
        }

        let resp: serde_json::Value = serde_json::from_str(&result_line)?;
        if let Some(err) = resp.get("error") {
            return Err(anyhow!("MCP Tool Error: {}", err));
        }

        if let Some(res) = resp.get("result") {
            if let Some(content) = res.get("content").and_then(|c| c.as_array()) {
                let texts: Vec<String> = content
                    .iter()
                    .filter_map(|item| item.get("text").and_then(|t| t.as_str()).map(|s| s.to_string()))
                    .collect();
                if !texts.is_empty() {
                    return Ok(texts.join("\n"));
                }
            }
            return Ok(res.to_string());
        }

        Ok(result_line)
    }
}

// ─── WBUDOWANY SERWER MCP (OPENCODE-RS MCP SERVER HUB) ──────────────────────

pub struct McpServerHub;

impl McpServerHub {
    /// Uruchamia wbudowany serwer MCP w trybie stdio JSON-RPC
    pub fn run_stdio(work_dir: PathBuf) -> Result<()> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let reader = stdin.lock();

        let tool_engine = crate::agent::tools::ToolEngine::new(work_dir.clone());
        let mcp_manager = McpManager::load_from_project_or_global(&work_dir);

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Ok(req) = serde_json::from_str::<serde_json::Value>(trimmed) {
                let id = req.get("id").cloned();
                let method = req.get("method").and_then(|m| m.as_str()).unwrap_or_default();

                match method {
                    "initialize" => {
                        let resp = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "protocolVersion": "2024-11-05",
                                "capabilities": {
                                    "tools": { "listChanged": true },
                                    "resources": { "subscribe": false },
                                    "prompts": { "listChanged": true }
                                },
                                "serverInfo": {
                                    "name": "opencode-rs-mcp-server",
                                    "version": env!("CARGO_PKG_VERSION")
                                }
                            }
                        });
                        writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
                        stdout.flush()?;
                    }
                    "notifications/initialized" => {
                        // Klient potwierdził start
                    }
                    "ping" => {
                        let resp = serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": {} });
                        writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
                        stdout.flush()?;
                    }
                    "tools/list" => {
                        let mut tools = vec![
                            serde_json::json!({
                                "name": "read_file",
                                "description": "Odczytuje zawartość pliku w projekcie z opcjonalnym zakresem linii",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "file_path": { "type": "string", "description": "Względna ścieżka do pliku" },
                                        "start_line": { "type": "integer", "description": "Początkowa linia (1-indexed)" },
                                        "end_line": { "type": "integer", "description": "Końcowa linia (inclusive)" }
                                    },
                                    "required": ["file_path"]
                                }
                            }),
                            serde_json::json!({
                                "name": "edit_file",
                                "description": "Precyzyjnie podmienia zadany fragment kodu w pliku (generuje diff)",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "file_path": { "type": "string", "description": "Ścieżka do pliku" },
                                        "target_content": { "type": "string", "description": "Dokładny fragment do zastąpienia" },
                                        "replacement_content": { "type": "string", "description": "Nowy fragment kodu" }
                                    },
                                    "required": ["file_path", "target_content", "replacement_content"]
                                }
                            }),
                            serde_json::json!({
                                "name": "write_file",
                                "description": "Tworzy nowy plik lub nadpisuje istniejący",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "file_path": { "type": "string", "description": "Ścieżka do pliku" },
                                        "content": { "type": "string", "description": "Zawartość pliku" }
                                    },
                                    "required": ["file_path", "content"]
                                }
                            }),
                            serde_json::json!({
                                "name": "bash_exec",
                                "description": "Wykonuje polecenie powłoki w katalogu projektu",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "command": { "type": "string", "description": "Polecenie do wykonania" }
                                    },
                                    "required": ["command"]
                                }
                            }),
                            serde_json::json!({
                                "name": "grep_search",
                                "description": "Przeszukuje pliki projektu pod kątem wskazanego wzorca tekstowego",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "query": { "type": "string", "description": "Wyszukiwana fraza" }
                                    },
                                    "required": ["query"]
                                }
                            }),
                            serde_json::json!({
                                "name": "list_files",
                                "description": "Wyświetla drzewo katalogów i plików projektu z respektowaniem .gitignore",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "subpath": { "type": "string", "description": "Podkatalog (opcjonalny)" },
                                        "depth": { "type": "integer", "description": "Maksymalna głębokość" }
                                    }
                                }
                            }),
                        ];

                        // Dołącz narzędzia z zarejestrowanych podrzędnych serwerów MCP
                        for srv_name in mcp_manager.config.mcp_servers.keys() {
                            tools.push(serde_json::json!({
                                "name": format!("mcp__{}__forward", srv_name),
                                "description": format!("Przekazuje wywołanie do podpiętego serwera MCP '{}'", srv_name),
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "tool": { "type": "string", "description": "Nazwa narzędzia na serwerze podrzędnym" },
                                        "arguments": { "type": "object", "description": "Parametry narzędzia" }
                                    },
                                    "required": ["tool"]
                                }
                            }));
                        }

                        let resp = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": { "tools": tools }
                        });
                        writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
                        stdout.flush()?;
                    }
                    "tools/call" => {
                        let params = req.get("params").cloned().unwrap_or(serde_json::json!({}));
                        let name = params.get("name").and_then(|n| n.as_str()).unwrap_or_default();
                        let args = params.get("arguments").cloned().unwrap_or(serde_json::json!({}));

                        let output = match name {
                            "read_file" => {
                                let path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or_default();
                                let start = args.get("start_line").and_then(|v| v.as_u64()).map(|d| d as usize);
                                let end = args.get("end_line").and_then(|v| v.as_u64()).map(|d| d as usize);
                                tool_engine.read_file(path, start, end)
                            }
                            "edit_file" => {
                                let path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or_default();
                                let target = args.get("target_content").and_then(|v| v.as_str()).unwrap_or_default();
                                let replacement = args.get("replacement_content").and_then(|v| v.as_str()).unwrap_or_default();
                                tool_engine.edit_file(path, target, replacement)
                            }
                            "write_file" => {
                                let path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or_default();
                                let content = args.get("content").and_then(|v| v.as_str()).unwrap_or_default();
                                tool_engine.write_file(path, content)
                            }
                            "bash_exec" => {
                                let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or_default();
                                tool_engine.bash_exec(cmd)
                            }
                            "grep_search" => {
                                let query = args.get("query").and_then(|v| v.as_str()).unwrap_or_default();
                                tool_engine.grep_search(query)
                            }
                            "list_files" => {
                                let subpath = args.get("subpath").and_then(|v| v.as_str());
                                let depth = args.get("depth").and_then(|v| v.as_u64()).map(|d| d as usize);
                                tool_engine.list_files(subpath, depth)
                            }
                            _ => {
                                if name.starts_with("mcp__") {
                                    let parts: Vec<&str> = name.split("__").collect();
                                    if parts.len() >= 2 {
                                        let srv = parts[1];
                                        let sub_tool = args.get("tool").and_then(|t| t.as_str()).unwrap_or("default");
                                        let sub_args = args.get("arguments").cloned().unwrap_or(serde_json::json!({}));
                                        mcp_manager.execute_mcp_tool(srv, sub_tool, &sub_args)
                                    } else {
                                        Err(anyhow!("Nieprawidłowa nazwa serwera MCP"))
                                    }
                                } else {
                                    Err(anyhow!("Nieznane narzędzie MCP: {}", name))
                                }
                            }
                        };

                        let (is_err, content_text) = match output {
                            Ok(text) => (false, text),
                            Err(err) => (true, err.to_string()),
                        };

                        let resp = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [{
                                    "type": "text",
                                    "text": content_text
                                }],
                                "isError": is_err
                            }
                        });
                        writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
                        stdout.flush()?;
                    }
                    _ => {
                        // Domyślna odpowiedź na nieznane metody
                        if let Some(req_id) = id {
                            let resp = serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": req_id,
                                "error": { "code": -32601, "message": "Method not found" }
                            });
                            writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
                            stdout.flush()?;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
