use anyhow::{anyhow, Result};
use chrono::Utc;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use crate::config::AppConfig;
use crate::session::{ChatSession, SessionManager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleData {
    pub version: String,
    pub exported_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<AppConfig>,
    pub sessions: Vec<ChatSession>,
}

pub struct SyncEngine;

impl SyncEngine {
    /// Eksportuje sesje (i opcjonalnie konfigurację) do pliku .json/.bundle
    pub fn export_bundle(
        output_path: &Path,
        sessions: &[ChatSession],
        include_config: bool,
        config: Option<&AppConfig>,
    ) -> Result<()> {
        let bundle = BundleData {
            version: "1.0".to_string(),
            exported_at: Utc::now().to_rfc3339(),
            config: if include_config { config.cloned() } else { None },
            sessions: sessions.to_vec(),
        };

        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let json = serde_json::to_string_pretty(&bundle)?;
        fs::write(output_path, json)?;
        Ok(())
    }

    /// Importuje sesje z pliku paczki do podanego SessionManager
    pub fn import_bundle(
        input_path: &Path,
        session_manager: &SessionManager,
        config_updater: Option<&mut AppConfig>,
    ) -> Result<usize> {
        if !input_path.exists() {
            return Err(anyhow!("Plik paczki nie istnieje: {:?}", input_path));
        }

        let content = fs::read_to_string(input_path)?;
        let bundle = serde_json::from_str::<BundleData>(&content)?;

        let mut imported_count = 0;
        for session in &bundle.sessions {
            session_manager.save_session(session)?;
            imported_count += 1;
        }

        if let (Some(imported_cfg), Some(current_cfg)) = (bundle.config, config_updater) {
            *current_cfg = imported_cfg;
            current_cfg.save().ok();
        }

        Ok(imported_count)
    }

    /// Wysyła sesje na własny serwer synchronizacji przez REST API
    pub async fn push_to_server(
        server_url: &str,
        token: Option<&str>,
        project_name: &str,
        sessions: &[ChatSession],
    ) -> Result<String> {
        let client = Client::builder().build()?;
        let endpoint = format!("{}/api/v1/sync/push", server_url.trim_end_matches('/'));

        let bundle = BundleData {
            version: "1.0".to_string(),
            exported_at: Utc::now().to_rfc3339(),
            config: None,
            sessions: sessions.to_vec(),
        };

        let mut req = client.post(&endpoint).json(&serde_json::json!({
            "project": project_name,
            "bundle": bundle
        }));

        if let Some(tok) = token {
            req = req.header("Authorization", format!("Bearer {tok}"));
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Błąd serwera synchronizacji ({status}): {body}"));
        }

        Ok(format!("Pomyślnie zsynchronizowano {} sesji z serwerem.", sessions.len()))
    }

    /// Pobiera sesje ze zdalnego serwera synchronizacji i zapisuje je lokalnie
    pub async fn pull_from_server(
        server_url: &str,
        token: Option<&str>,
        project_name: &str,
        session_manager: &SessionManager,
    ) -> Result<usize> {
        let client = Client::builder().build()?;
        let endpoint = format!(
            "{}/api/v1/sync/pull?project={}",
            server_url.trim_end_matches('/'),
            urlencoding::encode(project_name)
        );

        let mut req = client.get(&endpoint);
        if let Some(tok) = token {
            req = req.header("Authorization", format!("Bearer {tok}"));
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("Błąd serwera synchronizacji ({status}): {body}"));
        }

        let bundle = resp.json::<BundleData>().await?;
        let mut count = 0;
        for session in &bundle.sessions {
            session_manager.save_session(session)?;
            count += 1;
        }

        Ok(count)
    }
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut encoded = String::new();
        for b in s.bytes() {
            match b {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    encoded.push(b as char);
                }
                _ => {
                    encoded.push_str(&format!("%{:02X}", b));
                }
            }
        }
        encoded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ChatMessage;
    use uuid::Uuid;

    #[test]
    fn test_export_and_import_bundle() {
        let temp_dir_a = std::env::temp_dir().join(format!("opencode_sync_a_{}", Uuid::new_v4()));
        let temp_dir_b = std::env::temp_dir().join(format!("opencode_sync_b_{}", Uuid::new_v4()));
        let bundle_file = temp_dir_a.join("test_export.bundle.json");

        let sm_a = SessionManager::new(temp_dir_a.clone(), "in_project");
        let sm_b = SessionManager::new(temp_dir_b.clone(), "in_project");

        // 1. Stwórz sesję w projekcie A
        let mut session = sm_a.create_session("cursor-claude-3-7-sonnet");
        session.title = "Sesja do eksportu".to_string();
        session.messages.push(ChatMessage {
            role: "user".to_string(),
            content: "Wygeneruj API".to_string(),
        });
        sm_a.save_session(&session).unwrap();

        // 2. Eksportuj sesję do bundle
        let config = AppConfig::default();
        let sessions = vec![session.clone()];
        assert!(SyncEngine::export_bundle(&bundle_file, &sessions, true, Some(&config)).is_ok());
        assert!(bundle_file.exists());

        // 3. Zaimportuj bundle do projektu B
        let mut new_config = AppConfig::default();
        let imported = SyncEngine::import_bundle(&bundle_file, &sm_b, Some(&mut new_config)).unwrap();
        assert_eq!(imported, 1);

        // 4. Sprawdź czy projekt B ma tę sesję
        let loaded = sm_b.load_session(&session.id).unwrap();
        assert_eq!(loaded.title, "Sesja do eksportu");
        assert_eq!(loaded.messages.len(), 2);

        // Posprzątaj
        fs::remove_dir_all(&temp_dir_a).ok();
        fs::remove_dir_all(&temp_dir_b).ok();
    }
}
