use std::fs;
use std::path::{Path, PathBuf};

/// Uniwersalny loader skilli: Roo Code (.roo/skills), Cline (.clinerules, .cline/skills), CommandCode (.commandcode/skills), OpenCode (.opencode/skills)
pub struct SkillsManager {
    work_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SkillInfo {
    pub name: String,
    pub source: String, // roo/cline/opencode/commandcode
    pub path: PathBuf,
    pub preview: String,
}

impl SkillsManager {
    pub fn new(work_dir: PathBuf) -> Self {
        Self { work_dir }
    }

    fn read_file_safe(p: &Path) -> Option<String> {
        fs::read_to_string(p).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }

    pub fn list_skills(&self) -> Vec<SkillInfo> {
        let mut v = Vec::new();
        let roots = vec![
            (self.work_dir.join(".roo").join("skills"), "roo"),
            (self.work_dir.join(".roo").join("modes"), "roo-mode"),
            (self.work_dir.join(".cline").join("skills"), "cline"),
            (self.work_dir.join(".commandcode").join("skills"), "commandcode"),
            (self.work_dir.join(".opencode").join("skills"), "opencode"),
            (self.work_dir.join("skills"), "generic"),
        ];
        for (root, src) in roots {
            if let Ok(entries) = fs::read_dir(&root) {
                for e in entries.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        for cand in ["SKILL.md", "skill.md", "README.md", "prompt.md"] {
                            let f = p.join(cand);
                            if let Some(c) = Self::read_file_safe(&f) {
                                v.push(SkillInfo {
                                    name: p.file_name().unwrap_or_default().to_string_lossy().to_string(),
                                    source: src.to_string(),
                                    path: f,
                                    preview: c.lines().take(3).collect::<Vec<_>>().join(" "),
                                });
                                break;
                            }
                        }
                    } else if p.extension().is_some_and(|e| e == "md") {
                        if let Some(c) = Self::read_file_safe(&p) {
                            v.push(SkillInfo {
                                name: p.file_stem().unwrap_or_default().to_string_lossy().to_string(),
                                source: src.to_string(),
                                path: p.clone(),
                                preview: c.lines().take(3).collect::<Vec<_>>().join(" "),
                            });
                        }
                    }
                }
            }
        }
        // Dodatkowo: cline custom instructions
        for p in [
            self.work_dir.join(".clinerules"),
            self.work_dir.join(".roorules"),
            self.work_dir.join(".cursorrules"),
        ] {
            if let Some(c) = Self::read_file_safe(&p) {
                v.push(SkillInfo {
                    name: p.file_name().unwrap_or_default().to_string_lossy().to_string(),
                    source: "rules".to_string(),
                    path: p,
                    preview: c.lines().take(2).collect::<Vec<_>>().join(" "),
                });
            }
        }
        v
    }

    pub fn collect_skills_context(&self) -> String {
        let skills = self.list_skills();
        if skills.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        out.push_str(&format!("\n[Skills — {} skilli Roo/Cline/CommandCode/OpenCode]:\n", skills.len()));
        for s in skills.iter().take(12) {
            if let Some(c) = Self::read_file_safe(&s.path) {
                let trunc = if c.len() > 2000 { format!("{}... [przycięto]", &c[..2000]) } else { c };
                out.push_str(&format!("\n--- Skill {} [{}] {} ---\n{}\n", s.name, s.source, s.path.display(), trunc));
            }
        }
        if skills.len() > 12 {
            out.push_str(&format!("\n... i {} więcej skilli (użyj /skills aby zobaczyć wszystkie)\n", skills.len() - 12));
        }
        out
    }

    pub fn preview_list(&self) -> String {
        let skills = self.list_skills();
        if skills.is_empty() {
            return "📦 Brak skilli — dodaj .roo/skills/<name>/SKILL.md lub .opencode/skills/*.md\nWzór: https://docs.roocode.com/skills".to_string();
        }
        let mut lines = vec![format!("📦 Skilli: {}", skills.len())];
        for s in skills {
            lines.push(format!(" • {} [{}] — {} — {}", s.name, s.source, s.path.display(), s.preview.chars().take(60).collect::<String>()));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn test_skills_empty_when_no_dir() {
        let dir = std::env::temp_dir().join(format!("opencode_skills_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let sm = SkillsManager::new(dir.clone());
        assert!(sm.list_skills().is_empty());
        assert!(sm.collect_skills_context().is_empty());
        assert!(sm.preview_list().contains("Brak skilli"));
        fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn test_skills_roo_detection() {
        let dir = std::env::temp_dir().join(format!("opencode_skills_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join(".roo").join("skills").join("my-skill")).unwrap();
        fs::write(dir.join(".roo").join("skills").join("my-skill").join("SKILL.md"), "# My Skill\nDo X").unwrap();
        let sm = SkillsManager::new(dir.clone());
        let list = sm.list_skills();
        assert!(list.iter().any(|s| s.name == "my-skill" && s.source == "roo"));
        assert!(sm.collect_skills_context().contains("My Skill"));
        fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn test_skills_clinerules_detection() {
        let dir = std::env::temp_dir().join(format!("opencode_skills_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".clinerules"), "follow clean code").unwrap();
        let sm = SkillsManager::new(dir.clone());
        assert!(sm.list_skills().iter().any(|s| s.name == ".clinerules"));
        fs::remove_dir_all(&dir).ok();
    }
}
