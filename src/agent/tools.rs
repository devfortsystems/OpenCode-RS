use anyhow::{anyhow, Result};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub name: String,
    pub success: bool,
    pub output: String,
}

pub struct ToolEngine {
    work_dir: PathBuf,
}

impl ToolEngine {
    pub fn new(work_dir: PathBuf) -> Self {
        Self { work_dir }
    }

    pub fn list_files(&self, subpath: Option<&str>, max_depth: Option<usize>) -> Result<String> {
        let root = if let Some(sp) = subpath {
            self.work_dir.join(sp)
        } else {
            self.work_dir.clone()
        };

        let mut builder = WalkBuilder::new(&root);
        builder.hidden(true);
        builder.git_ignore(true);
        if let Some(depth) = max_depth {
            builder.max_depth(Some(depth));
        }

        let mut output = String::new();
        for result in builder.build() {
            match result {
                Ok(entry) => {
                    let path = entry.path();
                    if let Ok(rel_path) = path.strip_prefix(&self.work_dir) {
                        let path_str = rel_path.to_string_lossy();
                        if !path_str.is_empty() {
                            if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                                output.push_str(&format!("📁 {}/\n", path_str));
                            } else {
                                output.push_str(&format!("📄 {}\n", path_str));
                            }
                        }
                    }
                }
                Err(err) => output.push_str(&format!("Błąd: {err}\n")),
            }
        }

        Ok(output)
    }

    pub fn read_file(&self, file_path: &str, start_line: Option<usize>, end_line: Option<usize>) -> Result<String> {
        let full_path = self.work_dir.join(file_path);
        if !full_path.exists() {
            return Err(anyhow!("Plik nie istnieje: {}", file_path));
        }

        let content = fs::read_to_string(&full_path)?;
        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();
        let file_bytes = content.len();

        let start = start_line.unwrap_or(1).saturating_sub(1);
        let end = end_line.unwrap_or(total_lines).min(total_lines);

        if start >= total_lines {
            return Ok(String::new());
        }

        // Metadane pliku — pomocne dla agenta i dla estymacji tokenów w UI.
        // Format: header z rozmiarem, liczbą linii, estymowanymi tokenami.
        let est_tokens_full = crate::cost::TokenEstimator::estimate_from_file_size(file_path, file_bytes);
        let is_partial = start > 0 || end < total_lines;
        let header = if is_partial {
            let slice_tokens = crate::cost::TokenEstimator::estimate(
                &lines[start..end].join("\n")
            );
            format!(
                "📄 {} ({} B, {} linii) — czytam linie {}-{} z {} | ~{} tokenów (cały plik ~{})\n",
                file_path, file_bytes, total_lines, start + 1, end, total_lines, slice_tokens, est_tokens_full
            )
        } else {
            format!(
                "📄 {} ({} B, {} linii) | ~{} tokenów\n",
                file_path, file_bytes, total_lines, est_tokens_full
            )
        };

        let mut result = String::new();
        result.push_str(&header);
        result.push_str(&"─".repeat(60));
        result.push('\n');
        for (i, line) in lines[start..end].iter().enumerate() {
            result.push_str(&format!("{:4} | {}\n", start + i + 1, line));
        }

        Ok(result)
    }

    pub fn edit_file(&self, file_path: &str, target_content: &str, replacement_content: &str) -> Result<String> {
        let full_path = self.work_dir.join(file_path);
        if !full_path.exists() {
            return Err(anyhow!("Plik nie istnieje: {}", file_path));
        }

        let original = fs::read_to_string(&full_path)?;
        if !original.contains(target_content) {
            return Err(anyhow!("Nie znaleziono zadanego fragmentu (target_content) w pliku"));
        }

        let new_content = original.replacen(target_content, replacement_content, 1);
        fs::write(&full_path, &new_content)?;

        // Metadane diff — rozmiar przed/po, delta tokenów
        let tokens_before = crate::cost::TokenEstimator::estimate(&original);
        let tokens_after = crate::cost::TokenEstimator::estimate(&new_content);
        let delta = tokens_after as i64 - tokens_before as i64;
        let delta_str = if delta >= 0 { format!("+{}", delta) } else { delta.to_string() };

        // Wygeneruj ładny podgląd diff
        let diff = TextDiff::from_lines(&original, &new_content);
        let mut diff_str = String::new();
        for change in diff.iter_all_changes() {
            let sign = match change.tag() {
                ChangeTag::Delete => "-",
                ChangeTag::Insert => "+",
                ChangeTag::Equal => " ",
            };
            diff_str.push_str(&format!("{}{}", sign, change));
        }

        Ok(format!(
            "✏️  Zaktualizowano {} ({}→{} B, {}→{} tok, Δ {})\n\nDiff:\n{}",
            file_path, original.len(), new_content.len(), tokens_before, tokens_after, delta_str, diff_str
        ))
    }

    pub fn write_file(&self, file_path: &str, content: &str) -> Result<String> {
        let full_path = self.work_dir.join(file_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&full_path, content)?;

        let tokens = crate::cost::TokenEstimator::estimate_from_file_size(file_path, content.len());
        Ok(format!(
            "📝 Zapisano {} ({} B, {} linii, ~{} tokenów)",
            file_path, content.len(), content.lines().count(), tokens
        ))
    }

    pub fn bash_exec(&self, command: &str) -> Result<String> {
        #[cfg(target_os = "windows")]
        let mut cmd = {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-Command", command]);
            c
        };

        #[cfg(not(target_os = "windows"))]
        let mut cmd = {
            let mut c = Command::new("sh");
            c.args(["-c", command]);
            c
        };

        cmd.current_dir(&self.work_dir);
        let output = cmd.output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let mut result = String::new();
        if !stdout.is_empty() {
            result.push_str(&stdout);
        }
        if !stderr.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&format!("[STDERR]\n{}", stderr));
        }

        if result.is_empty() {
            result = format!("Polecenie wykonane pomyślnie (kod wyjścia: {:?})", output.status.code());
        }

        Ok(result)
    }

    pub fn grep_search(&self, query: &str) -> Result<String> {
        let mut builder = WalkBuilder::new(&self.work_dir);
        builder.hidden(true);
        builder.git_ignore(true);

        let mut matches = Vec::new();

        for result in builder.build() {
            if let Ok(entry) = result {
                let path = entry.path();
                if entry.file_type().is_some_and(|ft| ft.is_file()) {
                    if let Ok(content) = fs::read_to_string(path) {
                        for (line_num, line) in content.lines().enumerate() {
                            if line.contains(query) {
                                if let Ok(rel) = path.strip_prefix(&self.work_dir) {
                                    matches.push(format!("{}:{}: {}", rel.display(), line_num + 1, line.trim()));
                                    if matches.len() >= 50 {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if matches.len() >= 50 {
                break;
            }
        }

        if matches.is_empty() {
            Ok(format!("Brak wyników dla zapytania: '{}'", query))
        } else {
            Ok(matches.join("\n"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_workdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opencode_tools_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_read_file_metadata_header() {
        let dir = tmp_workdir();
        let tools = ToolEngine::new(dir.clone());
        // Plik .rs — kod
        let code = "fn main() {\n    println!(\"hello\");\n}\n";
        fs::write(dir.join("main.rs"), code).unwrap();

        let result = tools.read_file("main.rs", None, None).unwrap();
        // Header powinien zawierać metadane: rozmiar, linie, tokeny
        assert!(result.contains("📄 main.rs"), "brak headera: {}", result);
        assert!(result.contains("B,"), "brak rozmiaru: {}", result);
        assert!(result.contains("linii"), "brak liczby linii: {}", result);
        assert!(result.contains("token"), "brak estymacji tokenów: {}", result);
        // Powinien pokazać 3 linie
        assert!(result.contains("3 linii"), "zła liczba linii: {}", result);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_read_file_partial_slice() {
        let dir = tmp_workdir();
        let tools = ToolEngine::new(dir.clone());
        let code = "line1\nline2\nline3\nline4\nline5\n";
        fs::write(dir.join("test.txt"), code).unwrap();

        // Czytaj tylko linie 2-4
        let result = tools.read_file("test.txt", Some(2), Some(4)).unwrap();
        assert!(result.contains("linie 2-4 z 5"), "brak info o slice: {}", result);
        assert!(result.contains("cały plik"), "brak info o całym pliku: {}", result);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_write_file_metadata() {
        let dir = tmp_workdir();
        let tools = ToolEngine::new(dir.clone());
        let content = "# README\n\nHello world\n";
        let result = tools.write_file("README.md", content).unwrap();
        assert!(result.contains("📝"), "brak ikony: {}", result);
        assert!(result.contains("B,"), "brak rozmiaru: {}", result);
        assert!(result.contains("linii"), "brak linii: {}", result);
        assert!(result.contains("token"), "brak tokenów: {}", result);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_edit_file_token_delta() {
        let dir = tmp_workdir();
        let tools = ToolEngine::new(dir.clone());
        fs::write(dir.join("app.rs"), "fn old() {\n    todo()\n}\n").unwrap();
        let result = tools.edit_file("app.rs", "fn old() {", "fn new_function() {").unwrap();
        assert!(result.contains("✏️"), "brak ikony: {}", result);
        assert!(result.contains("tok"), "brak tokenów: {}", result);
        assert!(result.contains("Δ"), "brak delty: {}", result);
        fs::remove_dir_all(&dir).ok();
    }
}
