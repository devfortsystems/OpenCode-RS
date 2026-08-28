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

        let start = start_line.unwrap_or(1).saturating_sub(1);
        let end = end_line.unwrap_or(lines.len()).min(lines.len());

        if start >= lines.len() {
            return Ok(String::new());
        }

        let mut result = String::new();
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

        Ok(format!("Pomyślnie zaktualizowano {}\n\nDiff zmian:\n{}", file_path, diff_str))
    }

    pub fn write_file(&self, file_path: &str, content: &str) -> Result<String> {
        let full_path = self.work_dir.join(file_path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&full_path, content)?;
        Ok(format!("Zapisano plik: {} ({} bajtów)", file_path, content.len()))
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
