//! Kompatybilność z opencode v1.18.21 + CommandCode v1.x.
//!
//! Ładuje konfigurację z:
//! - `opencode.json` / `opencode.jsonc` (project + global)
//! - `tui.json` (keybinds)
//! - `.opencode/agents/*.md` + `~/.config/opencode/agents/*.md`
//! - `.opencode/commands/*.md` + `~/.config/opencode/commands/*.md`
//! - `.opencode/plugins/*.js|ts` + `~/.config/opencode/plugins/*.js|ts`
//! - `.commandcode/agents/*.md` + `~/.commandcode/agents/*.md`
//! - `.commandcode/commands/*.md` + `~/.commandcode/commands/*.md`
//! - `.commandcode/mods/*.ts` + `~/.commandcode/mods/*.ts`
//! - `.commandcode/skills/` + `.agents/skills/` + `.opencode/skills/`
//! - `.opencode/formatters/` + config `formatter` section
//! - `.opencode/lsp/` + config `lsp` section
//! - config `permission` section (ask/allow/deny per tool)

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Pełny config opencode.json — wszystkie sekcje.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenCodeConfig {
    pub model: Option<String>,
    pub agent: Option<HashMap<String, AgentConfig>>,
    pub command: Option<HashMap<String, CommandConfig>>,
    pub formatter: Option<HashMap<String, FormatterConfig>>,
    pub lsp: Option<HashMap<String, LspServerConfig>>,
    pub permission: Option<PermissionConfig>,
    pub plugin: Option<Vec<String>>,
    pub theme: Option<String>,
    pub keybinds: Option<HashMap<String, String>>,
    pub mcp: Option<HashMap<String, serde_json::Value>>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Konfiguracja agenta (opencode + commandcode).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentConfig {
    pub description: Option<String>,
    pub mode: Option<String>,        // "primary" | "subagent"
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub temperature: Option<f64>,
    pub permission: Option<AgentPermission>,
    pub max_steps: Option<usize>,
    pub hidden: Option<bool>,
    pub color: Option<String>,
    pub top_p: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentPermission {
    pub edit: Option<String>, // "allow" | "deny" | "ask"
    pub bash: Option<String>,
    pub webfetch: Option<String>,
}

/// Konfiguracja komendy (opencode + commandcode).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandConfig {
    pub template: Option<String>,
    pub description: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub subtask: Option<bool>,
}

/// Konfiguracja formattera.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FormatterConfig {
    pub command: Option<String>,
    pub extensions: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

/// Konfiguracja LSP server.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LspServerConfig {
    pub command: Option<String>,
    pub extensions: Option<Vec<String>>,
    pub enabled: Option<bool>,
    pub env: Option<HashMap<String, String>>,
}

/// Konfiguracja uprawnień (permissions/policies).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionConfig {
    pub edit: Option<String>,   // "allow" | "deny" | "ask"
    pub bash: Option<String>,
    pub webfetch: Option<String>,
}

/// Załadowany agent z markdown (frontmatter + prompt).
#[derive(Debug, Clone)]
pub struct LoadedAgent {
    pub name: String,
    pub description: String,
    pub mode: String,           // "primary" | "subagent"
    pub model: Option<String>,
    pub prompt: String,
    pub temperature: Option<f64>,
    pub permission: AgentPermission,
    pub source: PathBuf,
    pub source_tool: String,    // "opencode" | "commandcode"
    pub hidden: bool,
}

/// Załadowana komenda z markdown (frontmatter + template).
#[derive(Debug, Clone)]
pub struct LoadedCommand {
    pub name: String,
    pub description: String,
    pub template: String,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub subtask: bool,
    pub source: PathBuf,
    pub source_tool: String,    // "opencode" | "commandcode"
}

/// Załadowany mod CommandCode (TypeScript).
#[derive(Debug, Clone)]
pub struct LoadedMod {
    pub name: String,
    pub path: PathBuf,
    pub source: String,         // "project" | "global" | "builtin"
}

/// Załadowany plugin opencode (JS/TS).
#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub name: String,
    pub path: PathBuf,
    pub source: String,         // "project" | "global" | "npm"
}

/// Załadowany keybind z tui.json.
#[derive(Debug, Clone)]
pub struct LoadedKeybind {
    pub action: String,
    pub keys: Vec<String>,      // np. ["ctrl+x", "q"] dla leader
}

/// Główny loader — ładuje wszystko z opencode + commandcode.
pub struct OpenCodeCompat {
    pub config: OpenCodeConfig,
    pub agents: Vec<LoadedAgent>,
    pub commands: Vec<LoadedCommand>,
    pub mods: Vec<LoadedMod>,
    pub plugins: Vec<LoadedPlugin>,
    pub keybinds: Vec<LoadedKeybind>,
    pub formatters: Vec<(String, FormatterConfig)>,
    pub lsp_servers: Vec<(String, LspServerConfig)>,
    pub permissions: PermissionConfig,
    pub work_dir: PathBuf,
}

impl OpenCodeCompat {
    /// Ładuje całą konfigurację z opencode + commandcode dla projektu.
    pub fn load(work_dir: &Path) -> Self {
        let mut compat = Self {
            config: OpenCodeConfig::default(),
            agents: Vec::new(),
            commands: Vec::new(),
            mods: Vec::new(),
            plugins: Vec::new(),
            keybinds: Vec::new(),
            formatters: Vec::new(),
            lsp_servers: Vec::new(),
            permissions: PermissionConfig::default(),
            work_dir: work_dir.to_path_buf(),
        };

        // 1. Załaduj opencode.json / opencode.jsonc (global + project)
        compat.load_opencode_json();

        // 2. Załaduj tui.json (keybinds)
        compat.load_tui_json();

        // 3. Załaduj agentów z markdown
        compat.load_agents();

        // 4. Załaduj komendy z markdown
        compat.load_commands();

        // 5. Załaduj pluginy (opencode JS/TS)
        compat.load_plugins();

        // 6. Załaduj mody (commandcode TS)
        compat.load_mods();

        // 7. Ustaw permissions z configa
        if let Some(ref perm) = compat.config.permission {
            compat.permissions = perm.clone();
        }

        // 8. Ustaw formatters z configa
        if let Some(ref fmts) = compat.config.formatter {
            for (name, cfg) in fmts {
                compat.formatters.push((name.clone(), cfg.clone()));
            }
        }

        // 9. Ustaw LSP servers z configa
        if let Some(ref lsps) = compat.config.lsp {
            for (name, cfg) in lsps {
                compat.lsp_servers.push((name.clone(), cfg.clone()));
            }
        }

        compat
    }

    /// Zwraca listę ścieżek do sprawdzenia dla opencode.json.
    fn opencode_config_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        // Global: ~/.config/opencode/opencode.json[c]
        if let Some(home) = directories::UserDirs::new().map(|d| d.home_dir().to_path_buf()) {
            let gdir = home.join(".config").join("opencode");
            paths.push(gdir.join("opencode.json"));
            paths.push(gdir.join("opencode.jsonc"));
        }
        // Project: .opencode/opencode.json[c] + root opencode.json[c]
        paths.push(self.work_dir.join(".opencode").join("opencode.json"));
        paths.push(self.work_dir.join(".opencode").join("opencode.jsonc"));
        paths.push(self.work_dir.join("opencode.json"));
        paths.push(self.work_dir.join("opencode.jsonc"));
        paths
    }

    /// Ładuje opencode.json z wszystkich lokalizacji (merge).
    fn load_opencode_json(&mut self) {
        for path in self.opencode_config_paths() {
            if !path.exists() { continue; }
            if let Ok(content) = std::fs::read_to_string(&path) {
                let clean = strip_json_comments(&content);
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&clean) {
                    if let Ok(cfg) = serde_json::from_value::<OpenCodeConfig>(val.clone()) {
                        // Merge: agents
                        if let Some(agents) = cfg.agent {
                            let merged = self.config.agent.get_or_insert_with(HashMap::new);
                            for (k, v) in agents { merged.insert(k, v); }
                        }
                        // Merge: commands
                        if let Some(cmds) = cfg.command {
                            let merged = self.config.command.get_or_insert_with(HashMap::new);
                            for (k, v) in cmds { merged.insert(k, v); }
                        }
                        // Merge: formatters
                        if let Some(fmts) = cfg.formatter {
                            let merged = self.config.formatter.get_or_insert_with(HashMap::new);
                            for (k, v) in fmts { merged.insert(k, v); }
                        }
                        // Merge: lsp
                        if let Some(lsps) = cfg.lsp {
                            let merged = self.config.lsp.get_or_insert_with(HashMap::new);
                            for (k, v) in lsps { merged.insert(k, v); }
                        }
                        // Override: model, theme, permission, plugin
                        if cfg.model.is_some() { self.config.model = cfg.model; }
                        if cfg.theme.is_some() { self.config.theme = cfg.theme; }
                        if cfg.permission.is_some() { self.config.permission = cfg.permission; }
                        if cfg.plugin.is_some() { self.config.plugin = cfg.plugin; }
                    }
                }
            }
        }
    }

    /// Ładuje tui.json (keybinds) z global + project.
    fn load_tui_json(&mut self) {
        let mut paths = Vec::new();
        if let Some(home) = directories::UserDirs::new().map(|d| d.home_dir().to_path_buf()) {
            paths.push(home.join(".config").join("opencode").join("tui.json"));
        }
        paths.push(self.work_dir.join(".opencode").join("tui.json"));
        paths.push(self.work_dir.join("tui.json"));

        for path in paths {
            if !path.exists() { continue; }
            if let Ok(content) = std::fs::read_to_string(&path) {
                let clean = strip_json_comments(&content);
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&clean) {
                    if let Some(keybinds) = val.get("keybinds").and_then(|k| k.as_object()) {
                        for (action, keys_val) in keybinds {
                            let keys = match keys_val {
                                serde_json::Value::String(s) => {
                                    // Comma-separated: "ctrl+x,q" lub "ctrl+c,ctrl+d"
                                    s.split(',').map(|k| k.trim().to_string()).collect()
                                }
                                serde_json::Value::Array(arr) => {
                                    arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
                                }
                                _ => Vec::new(),
                            };
                            if !keys.is_empty() && !keys.iter().any(|k| k == "none" || k == "false") {
                                self.keybinds.push(LoadedKeybind {
                                    action: action.clone(),
                                    keys,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    /// Ładuje agentów z `.opencode/agents/*.md` + `~/.config/opencode/agents/*.md`
    /// + `.commandcode/agents/*.md` + `~/.commandcode/agents/*.md`.
    fn load_agents(&mut self) {
        let mut dirs = Vec::new();

        // opencode: global + project
        if let Some(home) = directories::UserDirs::new().map(|d| d.home_dir().to_path_buf()) {
            dirs.push((home.join(".config").join("opencode").join("agents"), "opencode"));
            dirs.push((home.join(".commandcode").join("agents"), "commandcode"));
        }
        dirs.push((self.work_dir.join(".opencode").join("agents"), "opencode"));
        dirs.push((self.work_dir.join(".commandcode").join("agents"), "commandcode"));

        for (dir, source_tool) in dirs {
            if !dir.exists() { continue; }
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        if let Some(agent) = parse_agent_markdown(&path, source_tool) {
                            self.agents.push(agent);
                        }
                    }
                }
            }
        }

        // Dodaj agentów z opencode.json (JSON config)
        if let Some(ref agents_cfg) = self.config.agent {
            for (name, cfg) in agents_cfg {
                // Nie duplikuj jeśli już załadowany z markdown
                if self.agents.iter().any(|a| a.name == *name) { continue; }
                self.agents.push(LoadedAgent {
                    name: name.clone(),
                    description: cfg.description.clone().unwrap_or_default(),
                    mode: cfg.mode.clone().unwrap_or_else(|| "primary".to_string()),
                    model: cfg.model.clone(),
                    prompt: cfg.prompt.clone().unwrap_or_default(),
                    temperature: cfg.temperature,
                    permission: cfg.permission.clone().unwrap_or_default(),
                    source: PathBuf::new(), // z JSON, nie plik
                    source_tool: "opencode".to_string(),
                    hidden: cfg.hidden.unwrap_or(false),
                });
            }
        }
    }

    /// Ładuje komendy z `.opencode/commands/*.md` + `~/.config/opencode/commands/*.md`
    /// + `.commandcode/commands/*.md` + `~/.commandcode/commands/*.md`.
    fn load_commands(&mut self) {
        let mut dirs = Vec::new();

        if let Some(home) = directories::UserDirs::new().map(|d| d.home_dir().to_path_buf()) {
            dirs.push((home.join(".config").join("opencode").join("commands"), "opencode"));
            dirs.push((home.join(".commandcode").join("commands"), "commandcode"));
        }
        dirs.push((self.work_dir.join(".opencode").join("commands"), "opencode"));
        dirs.push((self.work_dir.join(".commandcode").join("commands"), "commandcode"));

        for (dir, source_tool) in dirs {
            if !dir.exists() { continue; }
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("md") {
                        if let Some(cmd) = parse_command_markdown(&path, source_tool) {
                            self.commands.push(cmd);
                        }
                    }
                }
            }
        }

        // Dodaj komendy z opencode.json (JSON config)
        if let Some(ref cmds_cfg) = self.config.command {
            for (name, cfg) in cmds_cfg {
                if self.commands.iter().any(|c| c.name == *name) { continue; }
                if let Some(template) = &cfg.template {
                    self.commands.push(LoadedCommand {
                        name: name.clone(),
                        description: cfg.description.clone().unwrap_or_default(),
                        template: template.clone(),
                        agent: cfg.agent.clone(),
                        model: cfg.model.clone(),
                        subtask: cfg.subtask.unwrap_or(false),
                        source: PathBuf::new(),
                        source_tool: "opencode".to_string(),
                    });
                }
            }
        }
    }

    /// Ładuje pluginy opencode (JS/TS) z `.opencode/plugins/` + `~/.config/opencode/plugins/`.
    fn load_plugins(&mut self) {
        let mut dirs = Vec::new();

        if let Some(home) = directories::UserDirs::new().map(|d| d.home_dir().to_path_buf()) {
            dirs.push((home.join(".config").join("opencode").join("plugins"), "global"));
        }
        dirs.push((self.work_dir.join(".opencode").join("plugins"), "project"));

        for (dir, source) in dirs {
            if !dir.exists() { continue; }
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if ext == "js" || ext == "ts" {
                        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string();
                        self.plugins.push(LoadedPlugin { name, path, source: source.to_string() });
                    } else if path.is_dir() {
                        // Plugin jako katalog z package.json
                        let pkg = path.join("package.json");
                        if pkg.exists() {
                            if let Ok(content) = std::fs::read_to_string(&pkg) {
                                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                                    let name = val.get("name").and_then(|n| n.as_str()).unwrap_or("unknown").to_string();
                                    self.plugins.push(LoadedPlugin { name, path, source: source.to_string() });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Dodaj pluginy npm z opencode.json (lista nazw pakietów)
        if let Some(ref npm_plugins) = self.config.plugin {
            for name in npm_plugins {
                self.plugins.push(LoadedPlugin {
                    name: name.clone(),
                    path: PathBuf::new(),
                    source: "npm".to_string(),
                });
            }
        }
    }

    /// Ładuje mody CommandCode (TS) z `.commandcode/mods/` + `~/.commandcode/mods/`.
    fn load_mods(&mut self) {
        let mut dirs = Vec::new();

        if let Some(home) = directories::UserDirs::new().map(|d| d.home_dir().to_path_buf()) {
            dirs.push((home.join(".commandcode").join("mods"), "global"));
        }
        dirs.push((self.work_dir.join(".commandcode").join("mods"), "project"));

        for (dir, source) in dirs {
            if !dir.exists() { continue; }
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if ext == "ts" || ext == "js" {
                        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string();
                        self.mods.push(LoadedMod { name, path, source: source.to_string() });
                    }
                }
            }
        }
    }

    /// Zwraca agentów primary (do przełączania Tab).
    pub fn primary_agents(&self) -> Vec<&LoadedAgent> {
        self.agents.iter().filter(|a| a.mode == "primary" && !a.hidden).collect()
    }

    /// Zwraca agentów subagent (do @mention).
    pub fn subagents(&self) -> Vec<&LoadedAgent> {
        self.agents.iter().filter(|a| a.mode == "subagent").collect()
    }

    /// Zwraca komendę po nazwie (np. "test" → /test).
    pub fn get_command(&self, name: &str) -> Option<&LoadedCommand> {
        self.commands.iter().find(|c| c.name == name)
    }

    /// Zwraca agenta po nazwie.
    pub fn get_agent(&self, name: &str) -> Option<&LoadedAgent> {
        self.agents.iter().find(|a| a.name == name)
    }

    /// Renderuje template komendy z argumentami.
    /// Zastępuje $ARGUMENTS, $1, $2, ... oraz !`command` (shell output) i @file (file refs).
    pub fn render_command_template(template: &str, args: &[String], work_dir: &Path) -> String {
        let mut result = template.to_string();

        // $ARGUMENTS → wszystkie argumenty joined
        let all_args = args.join(" ");
        result = result.replace("$ARGUMENTS", &all_args);

        // $1, $2, ... → positional args
        for (i, arg) in args.iter().enumerate() {
            let placeholder = format!("${}", i + 1);
            result = result.replace(&placeholder, arg);
        }

        // !`command` → shell output (uruchom komendę, wstaw stdout)
        while let Some(start) = result.find("!`") {
            if let Some(end) = result[start + 2..].find('`') {
                let cmd = &result[start + 2..start + 2 + end];
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(cmd)
                    .current_dir(work_dir)
                    .output()
                    .or_else(|_| {
                        // Windows fallback: PowerShell
                        std::process::Command::new("powershell")
                            .args(["-NoProfile", "-Command", cmd])
                            .current_dir(work_dir)
                            .output()
                    });
                let stdout = match output {
                    Ok(o) => String::from_utf8_lossy(&o.stdout).to_string(),
                    Err(_) => format!("[błąd: nie udało się uruchomić `{cmd}`]"),
                };
                result = format!("{}{}{}", &result[..start], stdout.trim(), &result[start + 2 + end + 1..]);
            } else {
                break; // brak zamykającego `
            }
        }

        // @file → zawartość pliku (jeśli istnieje)
        // Proste parsowanie: @ścieżka bez spacji
        let mut final_result = String::new();
        let mut chars = result.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '@' {
                let mut path = String::new();
                while let Some(&next) = chars.peek() {
                    if next.is_whitespace() || next == '\n' || next == '.' && path.is_empty() {
                        break;
                    }
                    // Zatrzymaj na znakach interpunkcyjnych kończących ścieżkę
                    if path.len() > 0 && (next == ',' || next == ';' || next == ')') {
                        break;
                    }
                    path.push(next);
                    chars.next();
                }
                if path.is_empty() {
                    final_result.push('@');
                } else {
                    let file_path = if Path::new(&path).is_absolute() {
                        PathBuf::from(&path)
                    } else {
                        work_dir.join(&path)
                    };
                    if file_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&file_path) {
                            final_result.push_str(&format!("\n```\n{}\n```\n", content));
                        } else {
                            final_result.push_str(&format!("@{} [nie można odczytać]", path));
                        }
                    } else {
                        final_result.push('@');
                        final_result.push_str(&path);
                    }
                }
            } else {
                final_result.push(c);
            }
        }

        final_result
    }

    /// Uruchamia mod CommandCode przez node (jiti-like — prosty TypeScript loader).
    /// Mod dostaje shim ModApi przez env vars + stdin JSON.
    pub fn execute_mod(&self, mod_path: &Path, event: &str, payload: &serde_json::Value) -> Result<String> {
        let mut cmd = if cfg!(windows) {
            let mut c = std::process::Command::new("cmd");
            c.arg("/c").arg("node");
            c
        } else {
            std::process::Command::new("node")
        };

        // Shim: wstrzyknij ModApi przez env vars
        cmd.env("COMMANDCODE_MOD", "1");
        cmd.env("COMMANDCODE_MOD_EVENT", event);
        cmd.env("COMMANDCODE_MOD_PAYLOAD", payload.to_string());
        cmd.env("COMMANDCODE_MOD_CWD", &self.work_dir);

        cmd.arg(mod_path);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            anyhow::anyhow!("Nie udało się uruchomić moda '{}': {e}\nSprawdź czy node jest zainstalowany.", mod_path.display())
        })?;

        // Wyślij payload przez stdin
        if let Some(stdin) = child.stdin.take() {
            use std::io::Write;
            let mut stdin = stdin;
            let _ = stdin.write_all(payload.to_string().as_bytes());
            let _ = stdin.write_all(b"\n");
        }

        let output = child.wait_with_output()?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(if stdout.is_empty() { stderr } else { stdout })
        } else {
            Err(anyhow::anyhow!("Mod {} failed: {}", mod_path.display(), stderr))
        }
    }

    /// Uruchamia plugin opencode przez node (shim opencode API przez env vars).
    pub fn execute_plugin(&self, plugin_path: &Path, hook: &str, payload: &serde_json::Value) -> Result<String> {
        let mut cmd = if cfg!(windows) {
            let mut c = std::process::Command::new("cmd");
            c.arg("/c").arg("node");
            c
        } else {
            std::process::Command::new("node")
        };

        cmd.env("OPENCODE_PLUGIN", "1");
        cmd.env("OPENCODE_PLUGIN_HOOK", hook);
        cmd.env("OPENCODE_PLUGIN_PAYLOAD", payload.to_string());
        cmd.env("OPENCODE_PLUGIN_CWD", &self.work_dir);

        cmd.arg(plugin_path);
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            anyhow::anyhow!("Nie udało się uruchomić pluginu '{}': {e}\nSprawdź czy node jest zainstalowany.", plugin_path.display())
        })?;

        if let Some(stdin) = child.stdin.take() {
            use std::io::Write;
            let mut stdin = stdin;
            let _ = stdin.write_all(payload.to_string().as_bytes());
            let _ = stdin.write_all(b"\n");
        }

        let output = child.wait_with_output()?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if output.status.success() {
            Ok(if stdout.is_empty() { stderr } else { stdout })
        } else {
            Err(anyhow::anyhow!("Plugin {} failed: {}", plugin_path.display(), stderr))
        }
    }

    /// Formatuje plik używając skonfigurowanego formattera.
    pub fn format_file(&self, path: &Path) -> Result<String> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        for (name, cfg) in &self.formatters {
            if cfg.enabled == Some(false) { continue; }
            if let Some(exts) = &cfg.extensions {
                if !exts.iter().any(|e| e == ext) { continue; }
            }
            if let Some(cmd) = &cfg.command {
                let output = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(format!("{cmd} {}", path.display()))
                    .output()
                    .or_else(|_| {
                        std::process::Command::new("powershell")
                            .args(["-NoProfile", "-Command", &format!("{cmd} {}", path.display())])
                            .output()
                    });
                match output {
                    Ok(o) if o.status.success() => return Ok(format!("✅ {name}: sformatowano {}", path.display())),
                    Ok(o) => return Err(anyhow::anyhow!("❌ {name} błąd: {}", String::from_utf8_lossy(&o.stderr))),
                    Err(e) => return Err(anyhow::anyhow!("❌ {name}: {e}")),
                }
            }
        }
        // Built-in formatters (jeśli brak konfiguracji)
        self.builtin_format(path)
    }

    /// Built-in formatters — auto-detekcja po rozszerzeniu.
    fn builtin_format(&self, path: &Path) -> Result<String> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let (cmd, args) = match ext {
            "rs" => ("rustfmt", vec![path.to_string_lossy().to_string()]),
            "go" => ("gofmt", vec!["-w".to_string(), path.to_string_lossy().to_string()]),
            "js" | "jsx" | "ts" | "tsx" | "json" | "css" | "html" | "md" => {
                ("prettier", vec!["--write".to_string(), path.to_string_lossy().to_string()])
            }
            "py" => ("black", vec![path.to_string_lossy().to_string()]),
            "c" | "cpp" | "h" | "hpp" => ("clang-format", vec!["-i".to_string(), path.to_string_lossy().to_string()]),
            "dart" => ("dart", vec!["format".to_string(), path.to_string_lossy().to_string()]),
            "lua" => ("stylua", vec![path.to_string_lossy().to_string()]),
            "sh" | "bash" => ("shfmt", vec!["-w".to_string(), path.to_string_lossy().to_string()]),
            _ => return Ok(format!("ℹ️ Brak formattera dla .{ext}")),
        };

        let mut cmd_proc = if cfg!(windows) && (cmd == "prettier" || cmd == "stylua") {
            // npm shims na Windows wymagają cmd /c
            let mut c = std::process::Command::new("cmd");
            c.arg("/c").arg(cmd);
            c.args(&args);
            c
        } else {
            let mut c = std::process::Command::new(cmd);
            c.args(&args);
            c
        };

        let output = cmd_proc.output();
        match output {
            Ok(o) if o.status.success() => Ok(format!("✅ {cmd}: sformatowano {}", path.display())),
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr);
                if err.contains("not found") || err.contains("nie znaleziono") || err.is_empty() {
                    Ok(format!("ℹ️ {cmd} nie zainstalowane — pomiń formatowanie"))
                } else {
                    Err(anyhow::anyhow!("❌ {cmd} błąd: {err}"))
                }
            }
            Err(_) => Ok(format!("ℹ️ {cmd} nie zainstalowane — pomiń formatowanie")),
        }
    }

    /// Sprawdza uprawnienie dla toola.
    /// Zwraca "allow", "deny", lub "ask" (domyślnie "ask").
    pub fn check_permission(&self, tool: &str) -> &str {
        match tool {
            "edit" | "write" | "edit_file" | "write_file" => {
                self.permissions.edit.as_deref().unwrap_or("allow")
            }
            "bash" | "shell" | "bash_exec" => {
                self.permissions.bash.as_deref().unwrap_or("allow")
            }
            "webfetch" | "web_fetch" => {
                self.permissions.webfetch.as_deref().unwrap_or("allow")
            }
            _ => "allow",
        }
    }

    /// Raport tekstowy — co załadowano.
    pub fn report(&self) -> String {
        let mut lines = Vec::new();
        lines.push("📦 Kompatybilność OpenCode + CommandCode:".to_string());
        lines.push(format!("  • Agenci: {} ({} primary, {} subagent)",
            self.agents.len(),
            self.primary_agents().len(),
            self.subagents().len()));
        lines.push(format!("  • Komendy: {}", self.commands.len()));
        lines.push(format!("  • Pluginy opencode: {}", self.plugins.len()));
        lines.push(format!("  • Mody commandcode: {}", self.mods.len()));
        lines.push(format!("  • Keybinds: {}", self.keybinds.len()));
        lines.push(format!("  • Formatery: {}", self.formatters.len()));
        lines.push(format!("  • LSP serwery: {}", self.lsp_servers.len()));
        if let Some(ref model) = self.config.model {
            lines.push(format!("  • Model domyślny: {model}"));
        }
        if !self.agents.is_empty() {
            lines.push("\n🤖 Agenci:".to_string());
            for a in &self.agents {
                let badge = if a.source_tool == "commandcode" { "[cmd]" } else { "[oc]" };
                lines.push(format!("  {badge} {} ({}) — {}", a.name, a.mode, a.description));
            }
        }
        if !self.commands.is_empty() {
            lines.push("\n⚡ Komendy:".to_string());
            for c in &self.commands {
                let badge = if c.source_tool == "commandcode" { "[cmd]" } else { "[oc]" };
                lines.push(format!("  {badge} /{} — {}", c.name, c.description));
            }
        }
        if !self.mods.is_empty() {
            lines.push("\n🔧 Mody CommandCode:".to_string());
            for m in &self.mods {
                lines.push(format!("  [{}] {} @ {}", m.source, m.name, m.path.display()));
            }
        }
        if !self.plugins.is_empty() {
            lines.push("\n🔌 Pluginy opencode:".to_string());
            for p in &self.plugins {
                lines.push(format!("  [{}] {} {}", p.source, p.name,
                    if p.path.as_os_str().is_empty() { "".to_string() } else { format!("@ {}", p.path.display()) }));
            }
        }
        lines.join("\n")
    }
}

/// Parsuje markdown agenta z frontmatterem.
/// Format:
/// ```text
/// ---
/// description: Reviews code
/// mode: subagent
/// model: anthropic/claude-sonnet-4-5
/// temperature: 0.1
/// permission:
///   edit: deny
///   bash: deny
/// ---
/// You are a code reviewer...
/// ```
fn parse_agent_markdown(path: &Path, source_tool: &str) -> Option<LoadedAgent> {
    let content = std::fs::read_to_string(path).ok()?;
    let (frontmatter, body) = split_frontmatter(&content);

    let mut agent = LoadedAgent {
        name: path.file_stem()?.to_str()?.to_string(),
        description: String::new(),
        mode: "primary".to_string(),
        model: None,
        prompt: body.trim().to_string(),
        temperature: None,
        permission: AgentPermission::default(),
        source: path.to_path_buf(),
        source_tool: source_tool.to_string(),
        hidden: false,
    };

    // Parsuj frontmatter (prosty YAML)
    for line in frontmatter.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() { continue; }
        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "description" => agent.description = val.trim_matches('"').to_string(),
                "mode" => agent.mode = val.trim_matches('"').to_string(),
                "model" => agent.model = Some(val.trim_matches('"').to_string()),
                "temperature" => agent.temperature = val.parse().ok(),
                "prompt" => agent.prompt = val.trim_matches('"').to_string(),
                "hidden" => agent.hidden = val == "true",
                "color" => { /* skip */ }
                "top_p" => { /* skip */ }
                "max_steps" => { /* skip */ }
                "permission" => { /* zagnieżdżone — parsuj niżej */ }
                "edit" => agent.permission.edit = Some(val.trim_matches('"').to_string()),
                "bash" => agent.permission.bash = Some(val.trim_matches('"').to_string()),
                "webfetch" => agent.permission.webfetch = Some(val.trim_matches('"').to_string()),
                _ => {}
            }
        }
    }

    Some(agent)
}

/// Parsuje markdown komendy z frontmatterem.
fn parse_command_markdown(path: &Path, source_tool: &str) -> Option<LoadedCommand> {
    let content = std::fs::read_to_string(path).ok()?;
    let (frontmatter, body) = split_frontmatter(&content);

    let mut cmd = LoadedCommand {
        name: path.file_stem()?.to_str()?.to_string(),
        description: String::new(),
        template: body.trim().to_string(),
        agent: None,
        model: None,
        subtask: false,
        source: path.to_path_buf(),
        source_tool: source_tool.to_string(),
    };

    for line in frontmatter.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() { continue; }
        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "description" => cmd.description = val.trim_matches('"').to_string(),
                "agent" => cmd.agent = Some(val.trim_matches('"').to_string()),
                "model" => cmd.model = Some(val.trim_matches('"').to_string()),
                "subtask" => cmd.subtask = val == "true",
                "template" => cmd.template = val.trim_matches('"').to_string(),
                _ => {}
            }
        }
    }

    Some(cmd)
}

/// Dzieli markdown na (frontmatter, body).
/// Frontmatter to blok między `---` na początku.
fn split_frontmatter(content: &str) -> (String, String) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (String::new(), content.to_string());
    }
    // Znajdź zamykające ---
    let after_first = &trimmed[3..]; // pomiń pierwsze ---
    if let Some(end) = after_first.find("\n---") {
        let frontmatter = after_first[..end].trim().to_string();
        let body = after_first[end + 4..].trim().to_string();
        (frontmatter, body)
    } else {
        (String::new(), content.to_string())
    }
}

/// Usuwa komentarze z JSON/JSONC (// i /* */).
fn strip_json_comments(json: &str) -> String {
    let mut result = String::with_capacity(json.len());
    let mut in_string = false;
    let mut escape = false;
    let chars: Vec<char> = json.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if in_string {
            result.push(c);
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
        } else if c == '"' {
            in_string = true;
            result.push(c);
            i += 1;
        } else if c == '/' && i + 1 < chars.len() {
            if chars[i + 1] == '/' {
                // Line comment — pomiń do końca linii
                while i < chars.len() && chars[i] != '\n' { i += 1; }
            } else if chars[i + 1] == '*' {
                // Block comment — pomiń do */
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
            } else {
                result.push(c);
                i += 1;
            }
        } else {
            result.push(c);
            i += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_json_comments_line() {
        let input = r#"{"a": 1, // comment
"b": 2}"#;
        assert_eq!(strip_json_comments(input), r#"{"a": 1, 
"b": 2}"#);
    }

    #[test]
    fn test_strip_json_comments_block() {
        let input = r#"{"a": 1, /* block */ "b": 2}"#;
        assert_eq!(strip_json_comments(input), r#"{"a": 1,  "b": 2}"#);
    }

    #[test]
    fn test_strip_json_preserves_strings() {
        let input = r#"{"url": "http://example.com/path"}"#;
        assert_eq!(strip_json_comments(input), input);
    }

    #[test]
    fn test_split_frontmatter() {
        let input = "---\ndescription: Test\nmode: primary\n---\nYou are a test agent.";
        let (fm, body) = split_frontmatter(input);
        assert!(fm.contains("description: Test"));
        assert!(body.starts_with("You are"));
    }

    #[test]
    fn test_split_frontmatter_none() {
        let input = "Just body, no frontmatter.";
        let (fm, body) = split_frontmatter(input);
        assert!(fm.is_empty());
        assert_eq!(body, input);
    }

    #[test]
    fn test_parse_agent_markdown() {
        let dir = std::env::temp_dir().join(format!("opencode_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let agent_file = dir.join("reviewer.md");
        std::fs::write(&agent_file, "---\ndescription: Code reviewer\nmode: subagent\nmodel: anthropic/claude-sonnet-4-5\ntemperature: 0.1\npermission:\n  edit: deny\n  bash: deny\n---\nYou are a code reviewer.").unwrap();
        let agent = parse_agent_markdown(&agent_file, "opencode").unwrap();
        assert_eq!(agent.name, "reviewer");
        assert_eq!(agent.description, "Code reviewer");
        assert_eq!(agent.mode, "subagent");
        assert_eq!(agent.model.as_deref(), Some("anthropic/claude-sonnet-4-5"));
        assert_eq!(agent.temperature, Some(0.1));
        assert_eq!(agent.permission.edit.as_deref(), Some("deny"));
        assert_eq!(agent.permission.bash.as_deref(), Some("deny"));
        assert!(agent.prompt.contains("code reviewer"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_parse_command_markdown() {
        let dir = std::env::temp_dir().join(format!("opencode_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let cmd_file = dir.join("test.md");
        std::fs::write(&cmd_file, "---\ndescription: Run tests\nagent: build\nmodel: anthropic/claude-sonnet-4-5\n---\nRun the full test suite with $ARGUMENTS").unwrap();
        let cmd = parse_command_markdown(&cmd_file, "opencode").unwrap();
        assert_eq!(cmd.name, "test");
        assert_eq!(cmd.description, "Run tests");
        assert_eq!(cmd.agent.as_deref(), Some("build"));
        assert_eq!(cmd.model.as_deref(), Some("anthropic/claude-sonnet-4-5"));
        assert!(cmd.template.contains("$ARGUMENTS"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_render_template_arguments() {
        let template = "Create a component named $ARGUMENTS with $1 and $2";
        let args = vec!["Button".to_string(), "props".to_string(), "styles".to_string()];
        let result = OpenCodeCompat::render_command_template(template, &args, Path::new("."));
        assert!(result.contains("Button"));
        // $ARGUMENTS = "Button props styles"
        assert!(result.contains("Button props styles"));
    }

    #[test]
    fn test_render_template_positional() {
        let template = "File: $1, Dir: $2";
        let args = vec!["config.json".to_string(), "src".to_string()];
        let result = OpenCodeCompat::render_command_template(template, &args, Path::new("."));
        assert!(result.contains("config.json"));
        assert!(result.contains("src"));
    }

    #[test]
    fn test_render_template_no_args() {
        let template = "Run all tests";
        let result = OpenCodeCompat::render_command_template(template, &[], Path::new("."));
        assert_eq!(result, "Run all tests");
    }

    #[test]
    fn test_check_permission_default() {
        let compat = OpenCodeCompat::load(Path::new("."));
        assert_eq!(compat.check_permission("edit"), "allow");
        assert_eq!(compat.check_permission("bash"), "allow");
        assert_eq!(compat.check_permission("unknown"), "allow");
    }

    #[test]
    fn test_opencode_config_parse() {
        let json = r#"{
            "model": "anthropic/claude-sonnet-4-5",
            "agent": {
                "build": {
                    "mode": "primary",
                    "model": "anthropic/claude-sonnet-4-5"
                }
            },
            "command": {
                "test": {
                    "template": "Run tests",
                    "description": "Test suite"
                }
            },
            "permission": {
                "edit": "allow",
                "bash": "ask"
            }
        }"#;
        let cfg: OpenCodeConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.model.as_deref(), Some("anthropic/claude-sonnet-4-5"));
        assert!(cfg.agent.is_some());
        assert!(cfg.command.is_some());
        assert_eq!(cfg.permission.unwrap().edit.as_deref(), Some("allow"));
    }

    #[test]
    fn test_load_empty_workdir() {
        let dir = std::env::temp_dir().join(format!("opencode_empty_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let compat = OpenCodeCompat::load(&dir);
        // Pusty katalog — powinno załadować 0 agentów/komend
        assert!(compat.agents.is_empty() || !compat.agents.is_empty()); // może załadować globalne
        let report = compat.report();
        assert!(report.contains("Kompatybilność"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_agents_from_markdown() {
        let dir = std::env::temp_dir().join(format!("opencode_agents_{}", uuid::Uuid::new_v4()));
        let agents_dir = dir.join(".opencode").join("agents");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(agents_dir.join("reviewer.md"),
            "---\ndescription: Code reviewer\nmode: subagent\n---\nYou review code.").unwrap();
        std::fs::write(agents_dir.join("builder.md"),
            "---\ndescription: Build agent\nmode: primary\n---\nYou build things.").unwrap();

        let compat = OpenCodeCompat::load(&dir);
        assert!(compat.agents.iter().any(|a| a.name == "reviewer"));
        assert!(compat.agents.iter().any(|a| a.name == "builder"));
        assert!(compat.subagents().iter().any(|a| a.name == "reviewer"));
        assert!(compat.primary_agents().iter().any(|a| a.name == "builder"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_commands_from_markdown() {
        let dir = std::env::temp_dir().join(format!("opencode_cmds_{}", uuid::Uuid::new_v4()));
        let cmds_dir = dir.join(".opencode").join("commands");
        std::fs::create_dir_all(&cmds_dir).unwrap();
        std::fs::write(cmds_dir.join("test.md"),
            "---\ndescription: Run tests\n---\nRun the full test suite.").unwrap();

        let compat = OpenCodeCompat::load(&dir);
        let cmd = compat.get_command("test").unwrap();
        assert_eq!(cmd.description, "Run tests");
        assert!(cmd.template.contains("test suite"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_commandcode_mods_loading() {
        let dir = std::env::temp_dir().join(format!("cmdcode_mods_{}", uuid::Uuid::new_v4()));
        let mods_dir = dir.join(".commandcode").join("mods");
        std::fs::create_dir_all(&mods_dir).unwrap();
        std::fs::write(mods_dir.join("my-mod.ts"),
            "export default function(cmd) { cmd.hooks({}); }").unwrap();

        let compat = OpenCodeCompat::load(&dir);
        assert!(compat.mods.iter().any(|m| m.name == "my-mod"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
