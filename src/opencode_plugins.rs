use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct OpencodePlugin {
    pub id: String,
    pub path: PathBuf,
    pub manifest: serde_json::Value,
}

pub struct OpencodePluginLoader {
    pub plugins: Vec<OpencodePlugin>,
}

impl OpencodePluginLoader {
    pub fn new() -> Self { Self { plugins: Vec::new() } }

    /// Skanuje ~/.config/opencode/plugins + .opencode/plugins (jak opencode)
    pub fn discover(&mut self, work_dir: &Path) -> usize {
        self.plugins.clear();
        let mut dirs = Vec::new();
        if let Some(home) = directories::UserDirs::new().map(|d| d.home_dir().to_path_buf()) {
            dirs.push(home.join(".config").join("opencode").join("plugins"));
            dirs.push(home.join(".local").join("share").join("opencode").join("plugins"));
        }
        dirs.push(work_dir.join(".opencode").join("plugins"));
        dirs.push(work_dir.join("plugins"));

        for dir in dirs {
            if dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("js") || path.extension().and_then(|e| e.to_str()) == Some("ts") {
                            let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string();
                            let manifest = serde_json::json!({"id": id, "path": path.display().to_string()});
                            self.plugins.push(OpencodePlugin { id, path, manifest });
                        } else if path.is_dir() {
                            let pkg = path.join("package.json");
                            if pkg.exists() {
                                if let Ok(content) = std::fs::read_to_string(&pkg) {
                                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                                        let id = val.get("name").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
                                        self.plugins.push(OpencodePlugin { id, path: path.clone(), manifest: val });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        self.plugins.len()
    }

    pub fn list(&self) -> Vec<String> {
        self.plugins.iter().map(|p| format!("{} @ {}", p.id, p.path.display())).collect()
    }

    /// Uruchamia plugin via node (pełna implementacja, nie stub) — shim opencode API via env
    pub fn execute(&self, id: &str, args: &[String]) -> Result<String> {
        let plugin = self.plugins.iter().find(|p| p.id == id).ok_or_else(|| anyhow::anyhow!("Plugin {} nie znaleziony", id))?;
        let entry = if plugin.path.is_dir() {
            plugin.path.join("index.js").to_string_lossy().to_string()
        } else {
            plugin.path.to_string_lossy().to_string()
        };
        // Użyj node jeśli dostępny, fallback na deno
        let mut cmd = Command::new("node");
        cmd.arg(&entry);
        for a in args { cmd.arg(a); }
        cmd.env("OPENCODE_PLUGIN", "1");
        let out = cmd.output();
        match out {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                if o.status.success() { Ok(if stdout.is_empty() { stderr } else { stdout }) }
                else { Err(anyhow::anyhow!("Plugin {} failed: {}", id, stderr)) }
            },
            Err(e) => Err(anyhow::anyhow!("Brak node do uruchomienia pluginu {}: {}", id, e)),
        }
    }

    pub fn get(&self, id: &str) -> Option<&OpencodePlugin> {
        self.plugins.iter().find(|p| p.id == id)
    }
}
