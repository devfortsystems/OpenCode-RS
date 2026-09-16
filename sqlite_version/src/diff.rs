use similar::{ChangeTag, TextDiff};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub old: String,
    pub new: String,
}

impl FileDiff {
    pub fn lines(&self) -> Vec<Line<'static>> {
        let diff = TextDiff::from_lines(&self.old, &self.new);
        let mut out = Vec::new();
        out.push(Line::from(vec![Span::styled(format!("── {} ──", self.path), Style::default().fg(Color::Cyan))]));
        for change in diff.iter_all_changes() {
            let (sign, style) = match change.tag() {
                ChangeTag::Delete => ("- ", Style::default().fg(Color::LightRed)),
                ChangeTag::Insert => ("+ ", Style::default().fg(Color::LightGreen)),
                ChangeTag::Equal => ("  ", Style::default().fg(Color::DarkGray)),
            };
            let mut s = sign.to_string();
            s.push_str(&change.to_string());
            // Trim trailing \r
            let line = s.trim_end_matches("\r\n").trim_end_matches('\n').to_string();
            out.push(Line::from(Span::styled(line, style)));
        }
        out.push(Line::from(Span::styled("  [Y] Accept  [N] Reject  [Ctrl+Y] All", Style::default().fg(Color::Yellow))));
        out
    }

    pub fn apply(&self, work_dir: &std::path::Path) -> anyhow::Result<()> {
        let p = work_dir.join(&self.path);
        if let Some(parent) = p.parent() { std::fs::create_dir_all(parent).ok(); }
        std::fs::write(&p, &self.new)?;
        Ok(())
    }
}

pub struct DiffManager {
    pub diffs: Vec<FileDiff>,
    pub index: usize,
}

impl DiffManager {
    pub fn new() -> Self { Self { diffs: Vec::new(), index: 0 } }
    pub fn push(&mut self, d: FileDiff) { self.diffs.push(d); }
    pub fn current(&self) -> Option<&FileDiff> { self.diffs.get(self.index) }
    pub fn accept_current(&mut self, work_dir: &std::path::Path) -> anyhow::Result<()> {
        if let Some(d) = self.diffs.get(self.index) { d.apply(work_dir)?; }
        if self.index + 1 < self.diffs.len() { self.index += 1; } else { self.diffs.clear(); self.index = 0; }
        Ok(())
    }
    pub fn reject_current(&mut self) {
        if !self.diffs.is_empty() { self.diffs.remove(self.index); if self.index >= self.diffs.len() && self.index > 0 { self.index -= 1; } }
    }
    pub fn accept_all(&mut self, work_dir: &std::path::Path) -> anyhow::Result<()> {
        for d in &self.diffs { d.apply(work_dir).ok(); }
        self.diffs.clear(); self.index = 0;
        Ok(())
    }
}
