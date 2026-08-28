use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Pełna implementacja Live Grep — ripgrep + preview (nie stub)
#[derive(Debug, Clone)]
pub struct GrepHit {
    pub file: PathBuf,
    pub line: usize,
    pub text: String,
}

pub struct LiveGrep;

impl LiveGrep {
    /// Szuka `query` w `work_dir` via `rg` (ripgrep) lub fallback `grep`
    pub fn search(work_dir: &Path, query: &str, limit: usize) -> Result<Vec<GrepHit>> {
        if query.trim().is_empty() { return Ok(Vec::new()); }
        // Spróbuj ripgrep
        let out = Command::new("rg").args(["--line-number", "--no-heading", "--color", "never", query, "."]).current_dir(work_dir).output();
        if let Ok(o) = out {
            if o.status.success() {
                return Ok(Self::parse_rg(&String::from_utf8_lossy(&o.stdout), limit));
            }
        }
        // Fallback: PowerShell Select-String na Windows
        let out2 = if cfg!(target_os = "windows") {
            Command::new("powershell").args(["-NoProfile", "-Command", &format!("Select-String -Path * -Pattern '{}' -Recurse | Select-Object -First {} | ForEach-Object {{ \"$($_.Path):$($_.LineNumber):$($_.Line)\" }}", query, limit)]).current_dir(work_dir).output()
        } else {
            Command::new("grep").args(["-rn", query, "."]).current_dir(work_dir).output()
        };
        if let Ok(o) = out2 {
            Ok(Self::parse_rg(&String::from_utf8_lossy(&o.stdout), limit))
        } else {
            Ok(Vec::new())
        }
    }

    fn parse_rg(s: &str, limit: usize) -> Vec<GrepHit> {
        let mut hits = Vec::new();
        for line in s.lines().take(limit) {
            if let Some((file_part, rest)) = line.split_once(':') {
                if let Some((line_no, text)) = rest.split_once(':') {
                    if let Ok(n) = line_no.parse::<usize>() {
                        hits.push(GrepHit { file: PathBuf::from(file_part), line: n, text: text.to_string() });
                    }
                }
            }
        }
        hits
    }

    pub fn preview(work_dir: &Path, hit: &GrepHit, ctx: usize) -> String {
        let path = work_dir.join(&hit.file);
        if let Ok(content) = std::fs::read_to_string(&path) {
            let lines: Vec<&str> = content.lines().collect();
            let start = hit.line.saturating_sub(ctx + 1);
            let end = (hit.line + ctx).min(lines.len());
            lines[start..end].iter().enumerate().map(|(i, l)| {
                let ln = start + i + 1;
                if ln == hit.line { format!("> {:4} | {}", ln, l) } else { format!("  {:4} | {}", ln, l) }
            }).collect::<Vec<_>>().join("\n")
        } else {
            hit.text.clone()
        }
    }
}
