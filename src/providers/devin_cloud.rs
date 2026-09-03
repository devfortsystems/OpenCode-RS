//! Devin Cloud — integracja z api.devin.ai v3 (sesje w chmurze Devina).
//!
//! W przeciwieństwie do `CliSubprocessProvider` (który uruchamia `devin -p` lokalnie),
//! ten provider tworzy **sesję w chmurze Devina** — pełny VM z shell, browser, repo access.
//! To jest "Devin as a Service": delegujemy zadanie do chmurowego agenta i pollujemy wynik.
//!
//! Wymaga:
//! - `DEVIN_API_KEY` — service user key (prefix `cog_`), generowany na app.devin.ai → Settings → Service Users
//! - `DEVIN_ORG_ID` — organization ID (prefix `org-`), widoczne na tej samej stronie
//!
//! API v3: https://docs.devin.ai/api-reference/v3
//! - POST /v3/organizations/{org_id}/sessions — create session
//! - GET  /v3/organizations/{org_id}/sessions/{devin_id} — get status
//! - GET  /v3/organizations/{org_id}/sessions/{devin_id}/messages — list messages
//!
//! `devin_mode` (normal/fast/lite/ultra/fusion) mapuje modele opencode-rs na tryby agenta.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc::Sender;

use super::{ChatMessage, Provider};

const DEVIN_API_BASE: &str = "https://api.devin.ai/v3";
const POLL_INTERVAL_SECS: u64 = 4; // co ile sekund pollować status + messages

pub struct DevinCloudProvider {
    api_key: String,
    org_id: String,
    work_dir: std::path::PathBuf,
    client: reqwest::Client,
}

impl DevinCloudProvider {
    pub fn new(api_key: String, org_id: String, work_dir: std::path::PathBuf) -> Self {
        Self {
            api_key,
            org_id,
            work_dir,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.api_key)
    }

    /// Mapuje nazwę modelu opencode-rs na `devin_mode` dla API v3.
    /// `devin-cloud` → normal, `devin-cloud-fast` → fast, etc.
    fn devin_mode_for_model(model: &str) -> &'static str {
        match model {
            "devin-cloud-fast" | "devin-cloud-fast-mode" => "fast",
            "devin-cloud-lite" => "lite",
            "devin-cloud-ultra" => "ultra",
            "devin-cloud-fusion" => "fusion",
            _ => "normal", // devin-cloud i wszystko innego → domyślny normal
        }
    }

    /// Tworzy sesję w chmurze Devina. Zwraca session_id.
    async fn create_session(&self, prompt: &str, devin_mode: &str) -> Result<String> {
        #[derive(Serialize)]
        struct CreateReq<'a> {
            prompt: &'a str,
            devin_mode: &'a str,
            bypass_approval: bool,
        }

        let url = format!("{DEVIN_API_BASE}/organizations/{}/sessions", self.org_id);
        let body = CreateReq {
            prompt,
            devin_mode,
            bypass_approval: true, // auto-approve — jak --permission-mode dangerous
        };

        let resp = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Devin API create session HTTP {status}: {text}");
        }

        #[derive(Deserialize)]
        struct CreateResp {
            session_id: Option<String>,
            #[serde(default)]
            url: Option<String>,
            #[serde(default)]
            status: Option<String>,
        }
        let parsed: CreateResp = serde_json::from_str(&text)
            .map_err(|e| anyhow!("Devin API: nie udało się sparsować odpowiedzi create: {e}\nSurowy: {text}"))?;

        let session_id = parsed.session_id.ok_or_else(|| {
            anyhow!("Devin API: brak session_id w odpowiedzi. Surowy: {text}")
        })?;

        // Jeśli jest URL, wyślij go jako pierwszą wiadomość (user widzi link do sesji w webapp).
        Ok(session_id)
    }

    /// Pobiera status sesji. Zwraca jeden z: "running", "exit", "error", "suspended", etc.
    async fn get_session_status(&self, session_id: &str) -> Result<String> {
        let url = format!(
            "{DEVIN_API_BASE}/organizations/{}/sessions/{}",
            self.org_id, session_id
        );
        let resp = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Devin API get session HTTP {status}: {text}");
        }

        #[derive(Deserialize)]
        struct SessionResp {
            #[serde(default)]
            status: Option<String>,
        }
        let parsed: SessionResp = serde_json::from_str(&text)
            .map_err(|e| anyhow!("Devin API: nie udało się sparsować statusu: {e}\nSurowy: {text}"))?;
        Ok(parsed.status.unwrap_or_else(|| "unknown".to_string()))
    }

    /// Pobiera messages sesji (chronologicznie). Zwraca listę (message_type, content).
    async fn list_messages(&self, session_id: &str) -> Result<Vec<DevinMessage>> {
        let url = format!(
            "{DEVIN_API_BASE}/organizations/{}/sessions/{}/messages?first=200",
            self.org_id, session_id
        );
        let resp = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("Devin API list messages HTTP {status}: {text}");
        }

        #[derive(Deserialize)]
        struct PaginatedMessages {
            #[serde(default)]
            data: Vec<DevinMessage>,
        }
        let parsed: PaginatedMessages = serde_json::from_str(&text)
            .map_err(|e| anyhow!("Devin API: nie udało się sparsować messages: {e}\nSurowy: {text}"))?;
        Ok(parsed.data)
    }
}

/// Wiadomość z sesji Devina. Struktura uproszczona — interesuje nas głównie
/// `message_type` (assistant/user/system) i `content` (treść).
#[derive(Debug, Clone, Deserialize)]
pub struct DevinMessage {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "type")]
    pub message_type: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub timestamp: Option<String>,
}

impl DevinMessage {
    /// Czy ta wiadomość jest od asystenta (output agenta)?
    fn is_assistant(&self) -> bool {
        matches!(
            self.message_type.as_deref(),
            Some("assistant") | Some("devin") | Some("agent") | Some("final") | Some("result")
        )
    }

    /// Czy to wiadomość, którą chcemy streamować do usera?
    fn is_streamable(&self) -> bool {
        // Streamujemy assistant messages + system messages (postęp, błędy).
        // Pomijamy user messages (to nasz własny prompt, już widoczny w TUI).
        !matches!(self.message_type.as_deref(), Some("user") | None)
    }
}

#[async_trait]
impl Provider for DevinCloudProvider {
    fn name(&self) -> &str {
        "Devin Cloud (api.devin.ai v3 — sesje w chmurze)"
    }

    async fn stream_chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        token_tx: Sender<String>,
    ) -> Result<()> {
        // Użyj ostatniego user promptu jako zadania dla Devina.
        // Devin Cloud ma własną sesję/VM — nie przekazujemy historii opencode-rs.
        let user_prompt = messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.clone())
            .unwrap_or_default();

        if user_prompt.trim().is_empty() {
            anyhow::bail!("Brak promptu usera — Devin Cloud potrzebuje zadania.");
        }

        // Wstrzyknij persistentny kontekst (plan + memory blocks) w prompt.
        // Devin Cloud ma własny VM, ale persistentny plan/pamięć projektu są wspólne.
        let prompt = build_cloud_context_aware_prompt(&self.work_dir, &user_prompt);

        let devin_mode = Self::devin_mode_for_model(model);

        // 1. Utwórz sesję w chmurze
        let _ = token_tx
            .send(format!(
                "☁️  Devin Cloud: tworzenie sesji (mode={devin_mode})...\n"
            ))
            .await;
        let session_id = self.create_session(&prompt, devin_mode).await?;
        let session_url = format!("https://app.devin.ai/sessions/{session_id}");
        let _ = token_tx
            .send(format!(
                "🔗 Sesja: {session_url}\n\n---\n\n"
            ))
            .await;

        // 2. Poll status + messages aż do terminal statusu
        let mut seen_message_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut last_status = String::from("running");
        let terminal_statuses = ["exit", "error", "suspended", "terminated", "cancelled"];

        while !terminal_statuses.contains(&last_status.as_str()) {
            tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;

            // Pobierz status
            match self.get_session_status(&session_id).await {
                Ok(s) => last_status = s,
                Err(e) => {
                    // Błąd pollingu — nie przerywamy, spróbujmy ponownie (chmura może mieć chwilowe problemy)
                    let _ = token_tx
                        .send(format!("⚠️  poll status: {e}\n"))
                        .await;
                    continue;
                }
            }

            // Pobierz messages i streamuj nowe
            match self.list_messages(&session_id).await {
                Ok(msgs) => {
                    for msg in msgs {
                        if !msg.is_streamable() {
                            continue;
                        }
                        let msg_id = msg.id.clone().unwrap_or_else(|| {
                            // Fallback: użyj timestamp + content hash jako ID
                            format!(
                                "{}_{}",
                                msg.timestamp.clone().unwrap_or_default(),
                                msg.content.as_deref().unwrap_or("").len()
                            )
                        });
                        if seen_message_ids.insert(msg_id.clone()) {
                            if let Some(ref content) = msg.content {
                                if !content.is_empty() {
                                    let prefix = if msg.is_assistant() {
                                        String::new() // assistant output bez prefixu
                                    } else {
                                        // system/progress messages z prefixem
                                        format!("[{}] ", msg.message_type.as_deref().unwrap_or("system"))
                                    };
                                    let _ = token_tx
                                        .send(format!("{prefix}{content}\n\n"))
                                        .await;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = token_tx
                        .send(format!("⚠️  poll messages: {e}\n"))
                        .await;
                }
            }
        }

        // 3. Status terminalny — zakończ
        let _ = token_tx
            .send(format!("\n---\n☁️  Devin Cloud: sesja zakończona (status={last_status})\n🔗 {session_url}\n"))
            .await;

        Ok(())
    }
}

/// Buduje prompt z wstrzykniętym persistentnym kontekstem projektu (plan + memory blocks).
/// Identyczna logika jak w `acp.rs::build_context_aware_prompt` — Devin Cloud też widzi
/// persistentny plan i pamięć projektu, nie tylko "własne" modele agenta.
fn build_cloud_context_aware_prompt(work_dir: &std::path::Path, user_prompt: &str) -> String {
    let plan = crate::memory::ProjectPlan::load(work_dir);
    let mb = crate::memory::MemoryBlocks::new(work_dir.to_path_buf());
    let blocks = mb.load_all();
    let blocks_str: String = blocks
        .iter()
        .filter(|(_, c)| !c.is_empty())
        .map(|(label, c)| format!("[{label}]: {}", c.chars().take(2000).collect::<String>()))
        .collect::<Vec<_>>()
        .join("\n");

    let plan_section = if plan.goal.is_empty() && plan.steps.is_empty() && plan.notes.is_empty() {
        "(brak planu)".to_string()
    } else {
        plan.to_prompt_section()
    };

    let blocks_section = if blocks_str.is_empty() {
        "(puste)".to_string()
    } else {
        blocks_str
    };

    format!(
        "[KONTEKST PROJEKTU — persistentny, per-projekt, wczytany z dysku]\n\n\
         PLAN PROJEKTU:\n{plan_section}\n\n\
         MEMORY BLOCKS (persona/human/project):\n{blocks_section}\n\n\
         ---\n\
         ZADANIE:\n{user_prompt}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_devin_mode_mapping() {
        assert_eq!(DevinCloudProvider::devin_mode_for_model("devin-cloud"), "normal");
        assert_eq!(DevinCloudProvider::devin_mode_for_model("devin-cloud-fast"), "fast");
        assert_eq!(DevinCloudProvider::devin_mode_for_model("devin-cloud-lite"), "lite");
        assert_eq!(DevinCloudProvider::devin_mode_for_model("devin-cloud-ultra"), "ultra");
        assert_eq!(DevinCloudProvider::devin_mode_for_model("devin-cloud-fusion"), "fusion");
        // Nieznany model → normal (domyślny)
        assert_eq!(DevinCloudProvider::devin_mode_for_model("devin-cloud-unknown"), "normal");
        assert_eq!(DevinCloudProvider::devin_mode_for_model("anything"), "normal");
    }

    #[test]
    fn test_message_is_assistant() {
        let m = DevinMessage {
            id: Some("1".to_string()),
            message_type: Some("assistant".to_string()),
            content: Some("hello".to_string()),
            timestamp: None,
        };
        assert!(m.is_assistant());
        assert!(m.is_streamable());

        let m_user = DevinMessage {
            id: Some("2".to_string()),
            message_type: Some("user".to_string()),
            content: Some("prompt".to_string()),
            timestamp: None,
        };
        assert!(!m_user.is_assistant());
        assert!(!m_user.is_streamable()); // user messages nie streamujemy
    }

    #[test]
    fn test_message_streamable_filters_user_and_none() {
        let m_system = DevinMessage {
            id: Some("3".to_string()),
            message_type: Some("system".to_string()),
            content: Some("progress".to_string()),
            timestamp: None,
        };
        assert!(m_system.is_streamable());

        let m_none_type = DevinMessage {
            id: Some("4".to_string()),
            message_type: None,
            content: Some("orphan".to_string()),
            timestamp: None,
        };
        assert!(!m_none_type.is_streamable()); // brak typu = pomijamy
    }

    #[test]
    fn test_provider_name() {
        let p = DevinCloudProvider::new("cog_test".to_string(), "org-test".to_string(), std::env::temp_dir());
        assert!(p.name().contains("Devin Cloud"));
    }

    #[test]
    fn test_auth_header_format() {
        let p = DevinCloudProvider::new("cog_secret123".to_string(), "org-abc".to_string(), std::env::temp_dir());
        assert_eq!(p.auth_header(), "Bearer cog_secret123");
    }
}
