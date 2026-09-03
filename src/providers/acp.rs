//! ACP (Agent Client Protocol) client — pełna integracja z agentami ACP przez JSON-RPC over stdio.
//!
//! W przeciwieństwie do `CliSubprocessProvider` (który używa `devin -p` — tryb one-shot),
//! ten provider używa `devin acp` (lub dowolnego innego agenta ACP) z pełnym protokołem:
//! - handshake `initialize` (negocjacja wersji protokołu + capabilities)
//! - `session/new` (nowa sesja z cwd)
//! - `session/prompt` (wysyłka promptu, streaming odpowiedzi)
//! - `session/update` notifications (agent_message_chunk, agent_thought_chunk, tool_call, plan)
//! - `session/request_permission` (auto-approve w trybie YOLO)
//!
//! Bogatsza integracja niż subprocess: streaming token-po-tokenie, plan updates,
//! tool call visibility, thought/reasoning streaming. Agent może też delegować
//! operacje fs/terminal do nas (na razie odrzucamy — agent używa własnego środowiska).
//!
//! Wymaga: agent-CLI z subkomendą `acp` (np. `devin acp`, lub dowolny ACP-speaking agent).

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use std::str::FromStr;
use tokio::sync::mpsc::Sender;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    ContentBlock, InitializeRequest, NewSessionRequest, PromptRequest, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome,
    SessionNotification, SessionUpdate, TextContent,
};
use agent_client_protocol::{AcpAgent, Agent, Client, ConnectionTo};

use super::{ChatMessage, Provider};

/// Provider ACP — łączy się z agentem ACP (np. `devin acp`) przez JSON-RPC over stdio.
/// Pełny protokół: initialize → session/new → session/prompt z streamingiem.
pub struct AcpClientProvider {
    /// Komenda uruchamiająca agenta ACP (np. "devin acp", "opencode acp").
    command: String,
    /// Zmienne środowiskowe dla subprocessa (np. OPENCODE_MODEL=opencode/ling-...).
    env: Vec<(String, String)>,
    /// Nazwa wyświetlana (np. "Devin ACP", "OpenCode ACP").
    display_name: &'static str,
    /// Katalog projektu — do wstrzykiwania planu + memory blocks w prompt delegata.
    /// None = bez wstrzykiwania (delegat nie widzi persistentnego kontekstu).
    work_dir: Option<PathBuf>,
}

impl AcpClientProvider {
    pub fn new(command: String, display_name: &'static str, work_dir: Option<PathBuf>) -> Self {
        Self {
            command,
            env: Vec::new(),
            display_name,
            work_dir,
        }
    }

    /// Tworzy provider dla `devin acp` z opcjonalnym modelem.
    /// `model_suffix` to np. "opus", "sonnet", "codex" — dodawane jako `--model <suffix>`.
    /// Jeśli None, używa domyślnego modelu agenta.
    /// `work_dir` — katalog projektu, do wstrzykiwania planu + memory blocks.
    pub fn devin(model_suffix: Option<&str>, work_dir: PathBuf) -> Self {
        let command = match model_suffix {
            Some(m) if !m.is_empty() => format!("devin acp --model {m}"),
            _ => "devin acp".to_string(),
        };
        Self::new(command, "Devin ACP (JSON-RPC over stdio)", Some(work_dir))
    }

    /// Tworzy provider dla `opencode acp` (oryginalny opencode, 127 modeli w tym darmowe).
    /// `model` — opcjonalny model w formacie opencode (np. "opencode/ling-3.0-flash-fin-free").
    /// Jeśli None, używa domyślnego modelu opencode (konfigurowanego przez `opencode auth`).
    /// Model przekazywany przez env var OPENCODE_MODEL (ACP nie ma pola model w protokole).
    pub fn opencode(model: Option<&str>, work_dir: PathBuf) -> Self {
        let mut env = Vec::new();
        if let Some(m) = model {
            if !m.is_empty() {
                env.push(("OPENCODE_MODEL".to_string(), m.to_string()));
            }
        }
        Self {
            command: "opencode acp".to_string(),
            env,
            display_name: "OpenCode ACP (oryginalny, 127 modeli)",
            work_dir: Some(work_dir),
        }
    }

    /// Tworzy provider dla `gemini --acp` (Gemini CLI, darmowy tier 60 req/min).
    /// Wymaga zalogowania przez `gemini` (Google account) — darmowe.
    pub fn gemini(work_dir: PathBuf) -> Self {
        Self::new(
            "gemini --acp".to_string(),
            "Gemini CLI ACP (darmowy tier, Google account)",
            Some(work_dir),
        )
    }

    /// Tworzy provider dla `claude-code-acp` (Claude Code przez ACP adapter).
    /// Wymaga ANTHROPIC_API_KEY albo Claude Pro/Max subscription.
    pub fn claude_code(work_dir: PathBuf) -> Self {
        Self::new(
            "claude-code-acp".to_string(),
            "Claude Code ACP (Anthropic, Pro/Max lub API key)",
            Some(work_dir),
        )
    }

    /// Tworzy provider dla `codex-acp` (OpenAI Codex przez ACP adapter).
    /// Wymaga OPENAI_API_KEY albo CODEX_API_KEY.
    pub fn codex(work_dir: PathBuf) -> Self {
        Self::new(
            "codex-acp".to_string(),
            "Codex ACP (OpenAI, wymaga API key)",
            Some(work_dir),
        )
    }

    /// Dodaj zmienną środowiskową dla subprocessa.
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_string(), value.to_string()));
        self
    }
}

#[async_trait]
impl Provider for AcpClientProvider {
    fn name(&self) -> &str {
        self.display_name
    }

    async fn stream_chat(
        &self,
        _model: &str,
        messages: &[ChatMessage],
        token_tx: Sender<String>,
    ) -> Result<()> {
        // Użyj ostatniego user promptu jako zadania.
        // ACP agent ma własną sesję — nie przekazujemy historii opencode-rs.
        let user_prompt = messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.clone())
            .unwrap_or_default();

        if user_prompt.trim().is_empty() {
            anyhow::bail!("Brak promptu usera — agent ACP potrzebuje zadania.");
        }

        // Wstrzyknij persistentny kontekst projektu (plan + memory blocks) w prompt.
        // Dzięki temu delegat (Devin) widzi ten sam plan i pamięć co "własny" agent.
        // Bez tego "wspólny czat" byłby wspólny tylko między modelami własnymi.
        let prompt_text = match &self.work_dir {
            Some(wd) => build_context_aware_prompt(wd, &user_prompt),
            None => user_prompt,
        };

        // Spawn agenta ACP jako subprocess (JSON-RPC over stdio).
        // Jeśli mamy env vars (np. OPENCODE_MODEL), użyj from_args z leading NAME=value.
        // Jeśli nie, użyj from_str (prostsze).
        let agent = if self.env.is_empty() {
            AcpAgent::from_str(&self.command).map_err(|e| {
                anyhow!(
                    "Nie udało się uruchomić agenta ACP '{}': {e}\n\
                     Sprawdź czy komenda jest poprawna i agent jest w PATH.",
                    self.command
                )
            })?
        } else {
            // Zbuduj args: [ENV1=val1, ENV2=val2, command_word, arg1, arg2, ...]
            let mut args: Vec<String> = self.env.iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect();
            // Prosty split po whitespace — komendy ACP są proste (np. "opencode acp")
            args.extend(self.command.split_whitespace().map(|s| s.to_string()));
            let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            AcpAgent::from_args(args_ref).map_err(|e| {
                anyhow!(
                    "Nie udało się uruchomić agenta ACP '{}': {e}\n\
                     Sprawdź czy komenda jest poprawna i agent jest w PATH.",
                    self.command
                )
            })?
        };

        let _ = token_tx
            .send(format!("🔌 ACP: łączenie z agentem ({})...\n\n", self.command))
            .await;

        // Klonuj token_tx dla callbacku notyfikacji (Sender jest Clone)
        let tx_for_notifications = token_tx.clone();
        let tx_for_init = token_tx.clone();

        // Buduj klienta ACP z callbackami
        let result = Client
            .builder()
            // Callback dla session/update notyfikacji — streamuj treść do TUI
            .on_receive_notification(
                async move |notification: SessionNotification, _cx| {
                    stream_session_update(&tx_for_notifications, &notification.update).await;
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            // Callback dla session/request_permission — auto-approve (YOLO mode)
            .on_receive_request(
                async move |request: RequestPermissionRequest, responder, _connection| {
                    // Auto-approve: wybierz pierwszą opcję (zazwyczaj "Allow")
                    let option_id = request.options.first().map(|opt| opt.option_id.clone());
                    if let Some(id) = option_id {
                        responder.respond(RequestPermissionResponse::new(
                            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(id)),
                        ))
                    } else {
                        // Brak opcji — cancel (nie blokuj agenta)
                        responder.respond(RequestPermissionResponse::new(
                            RequestPermissionOutcome::Cancelled,
                        ))
                    }
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_with(agent, |connection: ConnectionTo<Agent>| async move {
                // 1. Initialize — negocjacja protokołu
                let _init_response = connection
                    .send_request(InitializeRequest::new(ProtocolVersion::V1))
                    .block_task()
                    .await
                    .map_err(|e| anyhow!("ACP initialize failed: {e}"))?;

                let _ = tx_for_init
                    .send(format!(
                        "🤝 ACP: połączono (protocol v1)\n\n"
                    ))
                    .await;

                // 2. New session — z cwd jako workspace
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
                let new_session_response = connection
                    .send_request(NewSessionRequest::new(cwd))
                    .block_task()
                    .await
                    .map_err(|e| anyhow!("ACP session/new failed: {e}"))?;

                let session_id = new_session_response.session_id;

                // 3. Prompt — wyślij zadanie, czekaj na zakończenie turn
                // Notyfikacje session/update przyjdą przez callback w międzyczasie.
                let prompt_response = connection
                    .send_request(PromptRequest::new(
                        session_id,
                        vec![ContentBlock::Text(TextContent::new(prompt_text.clone()))],
                    ))
                    .block_task()
                    .await
                    .map_err(|e| anyhow!("ACP session/prompt failed: {e}"))?;

                let _ = token_tx
                    .send(format!(
                        "\n\n---\n✅ ACP: zakończono (stop_reason: {:?})\n",
                        prompt_response.stop_reason
                    ))
                    .await;

                Ok(())
            })
            .await;

        result.map_err(|e| anyhow!("ACP connection error: {e}"))?;

        Ok(())
    }
}

/// Buduje prompt z wstrzykniętym persistentnym kontekstem projektu.
///
/// Format (tekstowo, nie jako wykonywalny kod):
/// ```text
/// [KONTEKST PROJEKTU — persistentny, per-projekt]
///
/// PLAN PROJEKTU:
/// <plan lub "(brak planu)">
///
/// MEMORY BLOCKS:
/// <persona/human/project lub "(puste)">
///
/// ---
/// ZADANIE:
/// <user prompt>
/// ```
///
/// Dzięki temu delegat (Devin ACP/Cloud) widzi ten sam plan i pamięć co "własny" agent.
/// Persistentny kontekst przetrwa restart UI i jest wspólny dla wszystkich modeli.
fn build_context_aware_prompt(work_dir: &std::path::Path, user_prompt: &str) -> String {
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

/// Streamuje SessionUpdate do TUI przez token_tx.
/// Wyciąga tekst z agent_message_chunk (główna odpowiedź) i agent_thought_chunk (reasoning).
/// Tool calls i plan updates pokazuje jako wskaźniki postępu.
async fn stream_session_update(token_tx: &Sender<String>, update: &SessionUpdate) {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => {
            if let ContentBlock::Text(ref text) = chunk.content {
                if !text.text.is_empty() {
                    let _ = token_tx.send(text.text.clone()).await;
                }
            }
        }
        SessionUpdate::AgentThoughtChunk(chunk) => {
            if let ContentBlock::Text(ref text) = chunk.content {
                if !text.text.is_empty() {
                    // Thought/reasoning — pokaż z prefixem (jak "thinking" w Claude)
                    let _ = token_tx
                        .send(format!("\n💭 {}\n", text.text))
                        .await;
                }
            }
        }
        SessionUpdate::ToolCall(tool_call) => {
            // Pokaż tool call jako wskaźnik postępu (title = human-readable opis)
            let _ = token_tx
                .send(format!("\n🔧 [tool] {}\n", tool_call.title))
                .await;
        }
        SessionUpdate::Plan(plan) => {
            // Pokaż plan jako wskaźnik postępu (entries = lista zadań)
            if !plan.entries.is_empty() {
                let steps_preview: Vec<String> = plan
                    .entries
                    .iter()
                    .take(5)
                    .map(|e| e.content.clone())
                    .collect();
                let _ = token_tx
                    .send(format!("\n📋 [plan] {}\n", steps_preview.join(" → ")))
                    .await;
            }
        }
        // Pozostałe typy update — pomijamy (plan_update, plan_removed, mode changes, etc.)
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_devin_acp_default_command() {
        let p = AcpClientProvider::devin(None, std::env::temp_dir());
        assert_eq!(p.command, "devin acp");
        assert!(p.name().contains("Devin ACP"));
    }

    #[test]
    fn test_devin_acp_with_model() {
        let p = AcpClientProvider::devin(Some("opus"), std::env::temp_dir());
        assert_eq!(p.command, "devin acp --model opus");

        let p2 = AcpClientProvider::devin(Some("sonnet"), std::env::temp_dir());
        assert_eq!(p2.command, "devin acp --model sonnet");
    }

    #[test]
    fn test_devin_acp_empty_model_uses_default() {
        let p = AcpClientProvider::devin(Some(""), std::env::temp_dir());
        assert_eq!(p.command, "devin acp");
    }

    #[test]
    fn test_custom_command() {
        let p = AcpClientProvider::new("my-agent acp".to_string(), "My Agent ACP", None);
        assert_eq!(p.command, "my-agent acp");
        assert_eq!(p.name(), "My Agent ACP");
    }
}
