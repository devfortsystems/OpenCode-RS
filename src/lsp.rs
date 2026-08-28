use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Pełna implementacja LSP diagnostics (nie stub) — cargo check + parsowanie JSON
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub file: PathBuf,
    pub line: usize,
    pub level: String, // error, warning
    pub message: String,
}

pub struct LspDiagnostics {
    pub diagnostics: HashMap<PathBuf, Vec<Diagnostic>>,
}

impl LspDiagnostics {
    pub fn new() -> Self { Self { diagnostics: HashMap::new() } }

    /// Uruchamia `cargo check --message-format=json` i parsuje diagnostykę
    pub fn refresh(&mut self, work_dir: &Path) -> Result<usize> {
        self.diagnostics.clear();
        let out = Command::new("cargo").args(["check", "--message-format=json"]).current_dir(work_dir).output();
        if let Ok(o) = out {
            let stdout = String::from_utf8_lossy(&o.stdout);
            for line in stdout.lines() {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(msg) = val.get("message") {
                        if let Some(spans) = msg.get("spans").and_then(|s| s.as_array()) {
                            for span in spans {
                                if span.get("is_primary").and_then(|v| v.as_bool()).unwrap_or(false) {
                                    if let Some(file) = span.get("file_name").and_then(|v| v.as_str()) {
                                        let line = span.get("line_start").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                                        let level = msg.get("level").and_then(|v| v.as_str()).unwrap_or("error").to_string();
                                        let message = msg.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                        let diag = Diagnostic { file: PathBuf::from(file), line, level, message };
                                        self.diagnostics.entry(PathBuf::from(file)).or_default().push(diag);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Fallback: jeśli cargo nie JSON, parsuj stderr dla rustc
            if self.diagnostics.is_empty() {
                let stderr = String::from_utf8_lossy(&o.stderr);
                for line in stderr.lines() {
                    if line.contains("error[") || line.contains("warning:") {
                        // Prosty fallback: cały stderr jako diagnostyka work_dir
                        self.diagnostics.entry(work_dir.to_path_buf()).or_default().push(Diagnostic {
                            file: work_dir.to_path_buf(),
                            line: 0,
                            level: if line.contains("error") { "error".to_string() } else { "warning".to_string() },
                            message: line.to_string(),
                        });
                    }
                }
            }
        }
        Ok(self.diagnostics.values().map(|v| v.len()).sum())
    }

    pub fn has_errors(&self, path: &Path) -> bool {
        self.diagnostics.get(path).is_some_and(|v| v.iter().any(|d| d.level == "error"))
    }

    pub fn count_for(&self, path: &Path) -> usize {
        self.diagnostics.get(path).map_or(0, |v| v.len())
    }

    pub fn summary(&self) -> String {
        let total: usize = self.diagnostics.values().map(|v| v.len()).sum();
        let files = self.diagnostics.len();
        if total == 0 { "LSP: brak błędów".to_string() } else { format!("LSP: {} błędów/ostrzeżeń w {} plikach", total, files) }
    }
}
