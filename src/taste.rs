use std::fs;
use std::path::{Path, PathBuf};

/// Manager dla CommandCode Taste (taste-1 meta neuro-symbolic)
/// Zgodny z https://commandcode.ai/docs/taste
/// Priority: 1. .commandcode/settings.local.json 2. .commandcode/settings.json 3. ~/.commandcode/config.json 4. default true
pub struct TasteManager {
    work_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct TastePackageInfo {
    pub name: String,
    pub path: PathBuf,
    pub learnings: usize,
}

impl TasteManager {
    pub fn new(work_dir: PathBuf) -> Self {
        Self { work_dir }
    }

    fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(PathBuf::from)
            .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
            .or_else(|| directories::UserDirs::new().map(|d| d.home_dir().to_path_buf()))
    }

    fn read_taste_learning_flag(path: &Path) -> Option<bool> {
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(b) = v.get("tasteLearning").and_then(|x| x.as_bool()) {
                    return Some(b);
                }
            }
        }
        None
    }

    /// Sprawdź czy Taste learning jest włączony (wg priority docs)
    pub fn is_enabled(&self) -> bool {
        // 1. local
        let local = self.work_dir.join(".commandcode").join("settings.local.json");
        if let Some(v) = Self::read_taste_learning_flag(&local) {
            return v;
        }
        // 2. project
        let proj = self.work_dir.join(".commandcode").join("settings.json");
        if let Some(v) = Self::read_taste_learning_flag(&proj) {
            return v;
        }
        // 3. user global
        if let Some(home) = Self::home_dir() {
            let user_cfg = home.join(".commandcode").join("config.json");
            if let Some(v) = Self::read_taste_learning_flag(&user_cfg) {
                return v;
            }
        }
        // 4. default on
        true
    }

    pub fn set_enabled(&self, enabled: bool, user_level: bool) -> anyhow::Result<String> {
        let target = if user_level {
            Self::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Brak katalogu domowego"))?
                .join(".commandcode")
                .join("config.json")
        } else {
            self.work_dir.join(".commandcode").join("settings.local.json")
        };
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).ok();
        }
        let mut json: serde_json::Value = if target.exists() {
            fs::read_to_string(&target)
                .ok()
                .and_then(|c| serde_json::from_str(&c).ok())
                .unwrap_or(serde_json::json!({}))
        } else {
            serde_json::json!({})
        };
        json["tasteLearning"] = serde_json::Value::Bool(enabled);
        fs::write(&target, serde_json::to_string_pretty(&json)?)?;
        Ok(format!(
            "{} Taste learning {} ({})",
            if enabled { "✅" } else { "🚫" },
            if enabled { "ON" } else { "OFF" },
            target.display()
        ))
    }

    /// Zbierz całą zawartość taste.md z projektu i global
    pub fn collect_taste_content(&self) -> String {
        let mut out = String::new();
        let mut count = 0;

        // Project: .commandcode/taste/**/taste.md
        let proj_root = self.work_dir.join(".commandcode").join("taste");
        if proj_root.exists() {
            if let Ok(collected) = Self::collect_from_dir(&proj_root, "Project") {
                if !collected.is_empty() {
                    out.push_str(&collected);
                    count += 1;
                }
            }
        }
        // Global: ~/.commandcode/taste/**/taste.md
        if let Some(home) = Self::home_dir() {
            let global_root = home.join(".commandcode").join("taste");
            if global_root.exists() {
                if let Ok(collected) = Self::collect_from_dir(&global_root, "Global") {
                    if !collected.is_empty() {
                        out.push_str(&collected);
                        count += 1;
                    }
                }
            }
        }
        // Main file fallback .commandcode/taste.md (legacy)
        let legacy = self.work_dir.join(".commandcode").join("taste.md");
        if let Ok(c) = fs::read_to_string(&legacy) {
            out.push_str(&format!("\n[TASTE Legacy {}]:\n{}\n", legacy.display(), c.trim()));
        }

        if out.is_empty() && count == 0 {
            String::new()
        } else {
            out
        }
    }

    fn collect_from_dir(root: &Path, scope: &str) -> anyhow::Result<String> {
        let mut buf = String::new();
        // main file root/taste.md
        let main = root.join("taste.md");
        if let Ok(c) = fs::read_to_string(&main) {
            buf.push_str(&format!("\n[TASTE {} {}]:\n{}\n", scope, main.display(), c.trim()));
        }
        // subpackages */taste.md
        if let Ok(entries) = fs::read_dir(root) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    let sub = p.join("taste.md");
                    if let Ok(c) = fs::read_to_string(&sub) {
                        let pkg = p.file_name().unwrap_or_default().to_string_lossy();
                        buf.push_str(&format!("\n[TASTE {}:{} {}]:\n{}\n", scope, pkg, sub.display(), c.trim()));
                    }
                }
            }
        }
        Ok(buf)
    }

    pub fn list_packages(&self) -> Vec<TastePackageInfo> {
        let mut v = Vec::new();
        for root in [
            self.work_dir.join(".commandcode").join("taste"),
            Self::home_dir()
                .map(|h| h.join(".commandcode").join("taste"))
                .unwrap_or_default(),
        ] {
            if let Ok(entries) = fs::read_dir(&root) {
                for e in entries.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        let md = p.join("taste.md");
                        if md.exists() {
                            let cnt = fs::read_to_string(&md).map(|c| c.lines().count()).unwrap_or(0);
                            v.push(TastePackageInfo {
                                name: p.file_name().unwrap_or_default().to_string_lossy().to_string(),
                                path: md,
                                learnings: cnt,
                            });
                        }
                    }
                }
            }
            // main
            let main = root.join("taste.md");
            if main.exists() {
                let cnt = fs::read_to_string(&main).map(|c| c.lines().count()).unwrap_or(0);
                v.push(TastePackageInfo {
                    name: "main".to_string(),
                    path: main,
                    learnings: cnt,
                });
            }
        }
        v
    }

    /// Wrapper na `npx taste ...` / `cmd taste ...` – proxy do orginalnego CLI
    pub fn npx_taste_args(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    pub fn exec_npx_taste(&self, args: &[&str]) -> anyhow::Result<String> {
        // Prefer npx taste, fallback cmd taste
        let output = std::process::Command::new("npx")
            .arg("taste")
            .args(args)
            .current_dir(&self.work_dir)
            .output();

        match output {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                let combined = if stderr.is_empty() {
                    stdout
                } else {
                    format!("{stdout}\n{stderr}")
                };
                if o.status.success() {
                    Ok(combined)
                } else {
                    Err(anyhow::anyhow!("npx taste {} failed:\n{}", args.join(" "), combined))
                }
            }
            Err(e) => Err(anyhow::anyhow!("Nie znaleziono `npx taste` (npm i -g command-code): {e}")),
        }
    }

    pub fn status_report(&self) -> String {
        let enabled = self.is_enabled();
        let content = self.collect_taste_content();
        let pkgs = self.list_packages();
        let enabled_str = if enabled { "✅ ON (uczy się w tle)" } else { "🚫 OFF" };
        let pkgs_str = if pkgs.is_empty() {
            "  (brak pakietów .commandcode/taste/**/taste.md)".to_string()
        } else {
            pkgs.iter()
                .map(|p| format!("  • {} ({} linii) -> {}", p.name, p.learnings, p.path.display()))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let preview = if content.is_empty() {
            "  (puste)".to_string()
        } else {
            let trunc = if content.len() > 1200 {
                format!("{}... [przycięto]", &content[..1200])
            } else {
                content.clone()
            };
            trunc
        };
        format!(
            "🧠 Taste-1 Status: {enabled_str}\nScope priority: .commandcode/settings.local.json > .commandcode/settings.json > ~/.commandcode/config.json > default ON\nPakiety:\n{pkgs_str}\nPodgląd taste.md:\n{preview}\n\nKomendy: /taste enable|disable [--user] | /taste push --all | /taste pull <ns/pkg> | /taste list [--global|--remote] | /taste lint | /taste open <pkg>"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn test_taste_default_enabled_when_no_files() {
        let dir = std::env::temp_dir().join(format!("opencode_taste_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let tm = TasteManager::new(dir.clone());
        assert!(tm.is_enabled(), "default should be ON when no settings exist");
        assert!(tm.collect_taste_content().is_empty());
        assert!(tm.list_packages().is_empty());
        fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn test_taste_disable_via_local_settings() {
        let dir = std::env::temp_dir().join(format!("opencode_taste_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join(".commandcode")).unwrap();
        fs::write(dir.join(".commandcode").join("settings.local.json"), r#"{"tasteLearning": false}"#).unwrap();
        let tm = TasteManager::new(dir.clone());
        assert!(!tm.is_enabled());
        fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn test_taste_collect_from_file() {
        let dir = std::env::temp_dir().join(format!("opencode_taste_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join(".commandcode").join("taste").join("cli")).unwrap();
        fs::write(dir.join(".commandcode").join("taste").join("cli").join("taste.md"), "# cli taste\nprefer tabs").unwrap();
        let tm = TasteManager::new(dir.clone());
        let c = tm.collect_taste_content();
        assert!(c.contains("prefer tabs"));
        assert!(c.contains("cli"));
        let pkgs = tm.list_packages();
        assert!(!pkgs.is_empty());
        fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn test_taste_set_enabled_roundtrip() {
        let dir = std::env::temp_dir().join(format!("opencode_taste_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let tm = TasteManager::new(dir.clone());
        tm.set_enabled(false, false).unwrap();
        assert!(!tm.is_enabled());
        tm.set_enabled(true, false).unwrap();
        assert!(tm.is_enabled());
        fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn test_taste_status_report_contains_sections() {
        let dir = std::env::temp_dir().join(format!("opencode_taste_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let tm = TasteManager::new(dir.clone());
        let r = tm.status_report();
        assert!(r.contains("Taste-1 Status"));
        assert!(r.contains("/taste"));
        fs::remove_dir_all(&dir).ok();
    }
}
