use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::session::ChatSession;

pub struct TemplateManager;

impl TemplateManager {
    /// Zwraca listę wbudowanych i niestandardowych szablonów
    pub fn get_templates() -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            ("/refactor", "Refaktoryzacja kodu", "Przeanalizuj podany fragment lub plik pod kątem czytelności, modularności, wydajności i wzorców projektowych. Zaproponuj zoptymalizowany kod z wyjaśnieniem zmian."),
            ("/tests", "Generowanie testów", "Napisz kompleksowy zestaw testów jednostkowych (edge-cases, błędy, happy-path) dla wskazanego komponentu."),
            ("/doc", "Generowanie dokumentacji", "Wygeneruj pełną dokumentację (docstringi, opisy parametrów, README) dla wskazanego kodu."),
            ("/review", "Audyt kodu", "Przeprowadź dokładny przegląd kodu (code review) szukając potencjalnych błędów logicznych, wycieków pamięci i podatności bezpieczeństwa."),
            ("/explain", "Wyjaśnienie działania", "Wyjaśnij krok po kroku działanie tego kodu w prosty i zrozumiały sposób."),
        ]
    }

    /// Pobiera treść szablonu na podstawie nazwy komendy
    pub fn resolve_template(cmd: &str) -> Option<&'static str> {
        Self::get_templates()
            .into_iter()
            .find(|(name, _, _)| *name == cmd)
            .map(|(_, _, prompt)| prompt)
    }

    /// Eksportuje całą sesję czatu do czytelnego pliku Markdown
    pub fn export_to_markdown(session: &ChatSession, output_path: &Path) -> Result<()> {
        let mut md = String::new();
        md.push_str(&format!("# ⚡ OpenCode-RS Raport Rozmowy: {}\n\n", session.title));
        md.push_str(&format!("- **ID Sesji**: `{}`\n", session.id));
        md.push_str(&format!("- **Projekt**: `{}`\n", session.project_path));
        md.push_str(&format!("- **Model**: `{}`\n", session.model));
        md.push_str(&format!("- **Utworzono**: {}\n", session.created_at));
        md.push_str(&format!("- **Zaktualizowano**: {}\n\n", session.updated_at));
        md.push_str("---\n\n");

        for msg in &session.messages {
            match msg.role.as_str() {
                "user" => {
                    md.push_str("### ❯ Użytkownik\n\n");
                    md.push_str(&msg.content);
                    md.push_str("\n\n");
                }
                "assistant" => {
                    md.push_str(&format!("### 🤖 OpenCode [{}]\n\n", session.model));
                    md.push_str(&msg.content);
                    md.push_str("\n\n");
                }
                "system" => {
                    md.push_str(&format!("> **System**: {}\n\n", msg.content));
                }
                _ => {}
            }
        }

        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        fs::write(output_path, md)?;
        Ok(())
    }
}
