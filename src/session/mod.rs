use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::providers::ChatMessage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub project_path: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub model: String,
    pub message_count: usize,
}

pub struct SessionManager {
    work_dir: PathBuf,
    storage_dir: PathBuf,
    db: Option<crate::database::Database>,
}

impl SessionManager {
    /// Full-text search po wszystkich sesjach (Ctrl+S) — pełna implementacja, nie stub
    pub fn search_sessions(&self, query: &str) -> Vec<(SessionMetadata, Vec<String>)> {
        let q = query.to_lowercase();
        let mut hits = Vec::new();
        if let Ok(metas) = self.list_sessions() {
            for meta in metas {
                if let Ok(sess) = self.load_session(&meta.id) {
                    let mut matched_lines = Vec::new();
                    for m in &sess.messages {
                        if m.content.to_lowercase().contains(&q) {
                            matched_lines.push(format!("[{}] {}", m.role, m.content.lines().next().unwrap_or("").chars().take(80).collect::<String>()));
                            if matched_lines.len() >= 3 { break; }
                        }
                    }
                    if !matched_lines.is_empty() || meta.title.to_lowercase().contains(&q) {
                        hits.push((meta, matched_lines));
                    }
                }
            }
        }
        hits
    }
}

impl SessionManager {
    pub fn new(work_dir: PathBuf, storage_mode: &str) -> Self {
        let storage_dir = Self::resolve_storage_dir(&work_dir, storage_mode);
        fs::create_dir_all(&storage_dir).ok();

        // Otwórz DevFortDB — jeśli się nie uda, fallback do JSON
        let db = crate::database::Database::open(&work_dir).ok();

        Self {
            work_dir,
            storage_dir,
            db,
        }
    }

    pub fn storage_path(&self) -> &Path {
        &self.storage_dir
    }

    fn resolve_storage_dir(work_dir: &Path, storage_mode: &str) -> PathBuf {
        if storage_mode == "in_project" {
            return work_dir.join(".opencode").join("sessions");
        }

        // Domyślny tryb "central" - izolowany per katalog projektu w systemowym katalogu danych
        if let Some(proj_dirs) = directories::ProjectDirs::from("com", "opencode", "opencode-rs") {
            let data_dir = proj_dirs.data_dir();
            let path_str = work_dir.to_string_lossy().to_string();
            let sanitized = path_str
                .replace([':', '\\', '/'], "_");
            return data_dir.join("projects").join(sanitized).join("sessions");
        }

        work_dir.join(".opencode").join("sessions")
    }

    pub fn create_session(&self, model: &str) -> ChatSession {
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();
        ChatSession {
            id,
            project_path: self.work_dir.to_string_lossy().to_string(),
            title: "Nowy czat".to_string(),
            created_at: now.clone(),
            updated_at: now,
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: "system".to_string(),
                content: "Witaj w OpenCode-RS! Wpisz prompt lub użyj [Ctrl+M], aby wybrać operatora, albo [Ctrl+H], aby przejrzeć historię sesji.".to_string(),
            }],
        }
    }

    pub fn save_session(&self, session: &ChatSession) -> Result<()> {
        // 1. DB primary — zapis do DevFortDB
        if let Some(db) = &self.db {
            if db.put_session(&session.id, session).is_ok() {
                // Migracja: usuń stary plik JSON jeśli istnieje (już w DB)
                let old_json = self.storage_dir.join(format!("{}.json", session.id));
                if old_json.exists() {
                    let _ = fs::remove_file(&old_json);
                }
                return Ok(());
            }
            // DB failed — fallback do JSON
        }

        // 2. JSON fallback
        let file_path = self.storage_dir.join(format!("{}.json", session.id));
        let json = serde_json::to_string_pretty(session)?;
        fs::write(file_path, json)?;
        Ok(())
    }

    pub fn load_session(&self, id: &str) -> Result<ChatSession> {
        // 1. DB primary
        if let Some(db) = &self.db {
            if let Ok(Some(session)) = db.get_session::<ChatSession>(id) {
                return Ok(session);
            }
            // DB miss — spróbuj JSON (może być stara sesja niezmigrowana)
        }

        // 2. JSON fallback + auto-migracja do DB
        let file_path = self.storage_dir.join(format!("{}.json", id));
        if !file_path.exists() {
            return Err(anyhow!("Sesja o ID {} nie istnieje", id));
        }

        let content = fs::read_to_string(&file_path)?;
        let session = serde_json::from_str::<ChatSession>(&content)?;

        // Auto-migracja: zapisz do DB, usuń JSON
        if let Some(db) = &self.db {
            if db.put_session(&session.id, &session).is_ok() {
                let _ = fs::remove_file(&file_path);
            }
        }

        Ok(session)
    }

    pub fn delete_session(&self, id: &str) -> Result<()> {
        // 1. DB
        if let Some(db) = &self.db {
            let _ = db.delete_session(id);
        }
        // 2. JSON (może istnieć jeśli DB nie był używany)
        let file_path = self.storage_dir.join(format!("{}.json", id));
        if file_path.exists() {
            fs::remove_file(file_path)?;
        }
        Ok(())
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionMetadata>> {
        let mut sessions = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        // 1. DB primary
        if let Some(db) = &self.db {
            if let Ok(ids) = db.list_sessions() {
                for id in ids {
                    if let Ok(Some(session)) = db.get_session::<ChatSession>(&id) {
                        seen_ids.insert(session.id.clone());
                        sessions.push(SessionMetadata {
                            id: session.id,
                            title: session.title,
                            created_at: session.created_at,
                            updated_at: session.updated_at,
                            model: session.model,
                            message_count: session.messages.len(),
                        });
                    }
                }
            }
        }

        // 2. JSON fallback — sesje niezmigrowane
        if self.storage_dir.exists() {
            for entry in fs::read_dir(&self.storage_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        if let Ok(session) = serde_json::from_str::<ChatSession>(&content) {
                            if seen_ids.contains(&session.id) {
                                continue; // już w DB
                            }
                            sessions.push(SessionMetadata {
                                id: session.id,
                                title: session.title,
                                created_at: session.created_at,
                                updated_at: session.updated_at,
                                model: session.model,
                                message_count: session.messages.len(),
                            });
                        }
                    }
                }
            }
        }

        // Sortuj od najnowszych do najstarszych
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(sessions)
    }

    pub fn get_latest_session(&self) -> Option<ChatSession> {
        let sessions = self.list_sessions().ok()?;
        let latest_meta = sessions.first()?;
        self.load_session(&latest_meta.id).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_creation_and_persistence() {
        let temp_dir = std::env::temp_dir().join(format!("opencode_test_{}", Uuid::new_v4()));
        let sm = SessionManager::new(temp_dir.clone(), "in_project");

        // 1. Utwórz sesję
        let mut session = sm.create_session("cursor-claude-3-7-sonnet");
        session.title = "Testowa sesja".to_string();
        session.messages.push(ChatMessage {
            role: "user".to_string(),
            content: "Napisz kalkulator w Rust".to_string(),
        });
        session.messages.push(ChatMessage {
            role: "assistant".to_string(),
            content: "Oto kod kalkulatora...".to_string(),
        });

        // 2. Zapisz sesję
        assert!(sm.save_session(&session).is_ok());

        // 3. Wylistuj sesje
        let list = sm.list_sessions().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Testowa sesja");
        assert_eq!(list[0].message_count, 3); // system + user + assistant

        // 4. Załaduj sesję
        let loaded = sm.load_session(&session.id).unwrap();
        assert_eq!(loaded.messages.len(), 3);
        assert_eq!(loaded.messages[1].content, "Napisz kalkulator w Rust");

        // 5. Usuń sesję
        assert!(sm.delete_session(&session.id).is_ok());
        let list_after = sm.list_sessions().unwrap();
        assert_eq!(list_after.len(), 0);

        // Posprzątaj
        fs::remove_dir_all(&temp_dir).ok();
    }
}
