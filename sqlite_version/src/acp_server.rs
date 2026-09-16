//! ACP (Agent Client Protocol) server — opencode-rs jako agent sterowany przez edytory.
//!
//! To odwrotna integracja niż `providers/acp.rs` (klient ACP):
//! - `providers/acp.rs` — opencode-rs jako KLIENT steruje innymi agentami ACP (devin acp, gemini --acp)
//! - `acp_server.rs` — opencode-rs jako AGENT jest sterowany przez edytory (Zed, Windsurf)
//!
//! Uruchomienie: `opencode --acp`
//! Edytor łączy się przez stdio (JSON-RPC), wysyła `initialize` → `session/new` → `session/prompt`.
//! opencode-rs wykonuje prompt używając własnych modeli (ProviderRouter) + tools (read/edit/write/bash/grep).
//! Streaming odpowiedzi wysyłany przez `session/update` notifications (AgentMessageChunk).
//!
//! Protokół: https://agentclientprotocol.com/

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::Mutex;

use agent_client_protocol::schema::v1::{
    AgentCapabilities, CancelNotification, ContentBlock, ContentChunk, EmbeddedResourceResource,
    InitializeRequest, InitializeResponse, Implementation, NewSessionRequest, NewSessionResponse,
    PromptCapabilities, PromptRequest, PromptResponse, SessionCapabilities, SessionId,
    SessionNotification, SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::{Agent, Stdio};

use crate::agent::Agent as OpencodeAgent;
use crate::config::AppConfig;
use crate::providers::ChatMessage;
use crate::providers::ProviderRouter;

/// Stan sesji ACP — historia czatu + cwd.
#[derive(Clone)]
struct AcpSession {
    #[allow(dead_code)]
    cwd: PathBuf,
    history: Vec<ChatMessage>,
    active_model: String,
    agent_mode: String,
}

/// Współdzielony stan serwera ACP — mapuje session_id → sesja.
type SessionMap = Arc<Mutex<HashMap<String, AcpSession>>>;

/// Punkt wejścia serwera ACP — uruchamiany przez `opencode --acp`.
///
/// Stdio transport, JSON-RPC over stdio. Edytor (Zed/Windsurf) jest klientem,
/// opencode-rs jest agentem.
pub async fn run_acp_server(work_dir: PathBuf) -> Result<()> {
    // Wczytaj config (klucze API, domyślny model, etc.)
    crate::auth::AuthManager::auto_load_credentials(&work_dir);
    let config = AppConfig::load_for_project(&work_dir);

    let active_model = config.default_model.clone();
    let agent_mode = "coder".to_string();

    // Stan sesji — współdzielony między handlerami przez Arc<Mutex>
    let sessions: SessionMap = Arc::new(Mutex::new(HashMap::new()));

    // Router + Agent opencode-rs — do wykonywania promptów
    let router = Arc::new(ProviderRouter::new(config.clone(), work_dir.clone()));
    let agent = Arc::new(OpencodeAgent::new(router.clone(), work_dir.clone()));

    // Klonowanie do handlerów
    let sessions_new = sessions.clone();
    let sessions_prompt = sessions.clone();
    let agent_prompt = agent.clone();

    Agent
        .builder()
        .name("opencode-rs")
        // ── Handler: initialize ──────────────────────────────────────────────
        // Klient negocjuje wersję protokołu + capabilities.
        .on_receive_request(
            async move |req: InitializeRequest, responder, _cx| {
                let response = InitializeResponse::new(req.protocol_version)
                    .agent_capabilities(
                        AgentCapabilities::new()
                            .prompt_capabilities(PromptCapabilities::new())
                            .session_capabilities(SessionCapabilities::new()),
                    )
                    .agent_info(Implementation::new("opencode-rs", env!("CARGO_PKG_VERSION")));
                responder.respond(response)
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── Handler: session/new ─────────────────────────────────────────────
        // Klient tworzy nową sesję z cwd.
        .on_receive_request(
            async move |req: NewSessionRequest, responder, _cx| {
                let session_id = uuid::Uuid::new_v4().to_string();
                let session = AcpSession {
                    cwd: req.cwd.clone(),
                    history: Vec::new(),
                    active_model: active_model.clone(),
                    agent_mode: agent_mode.clone(),
                };
                sessions_new.lock().await.insert(session_id.clone(), session);
                responder.respond(NewSessionResponse::new(SessionId::new(session_id)))
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── Handler: session/prompt ──────────────────────────────────────────
        // Klient wysyła prompt, agent wykonuje go używając własnych modeli + tools.
        // Streaming odpowiedzi przez session/update notifications (AgentMessageChunk).
        .on_receive_request(
            async move |req: PromptRequest, responder, cx| {
                // Wyciągnij tekst promptu z ContentBlocks
                let prompt_text = extract_text_from_blocks(&req.prompt);

                // Pobierz sesję
                let session_id_str = req.session_id.0.to_string();
                let session = {
                    let mut sessions = sessions_prompt.lock().await;
                    sessions.get_mut(&session_id_str).cloned()
                };

                let session = match session {
                    Some(s) => s,
                    None => {
                        // Sesja nie istnieje — zwróć błąd
                        return responder.respond(PromptResponse::new(StopReason::Refusal));
                    }
                };

                // Kanał do odbierania tokenów z agenta
                let (token_tx, mut token_rx) = tokio::sync::mpsc::channel::<String>(100);

                // Kanał context (nieużywany ale wymagany przez API)
                let (ctx_tx, _ctx_rx) = tokio::sync::mpsc::channel::<(usize, usize)>(10);

                // Klonowanie danych sesji
                let history = session.history.clone();
                let active_model = session.active_model.clone();
                let agent_mode = session.agent_mode.clone();
                let session_id_for_update = session_id_str.clone();
                let session_id_for_save = session_id_str.clone();
                let sessions_save = sessions.clone();
                let cx_clone = cx.clone();

                // Wątek: streaming tokenów → session/update notifications
                let streamer = tokio::spawn(async move {
                    let mut full_response = String::new();
                    while let Some(token) = token_rx.recv().await {
                        full_response.push_str(&token);
                        // Wyślij AgentMessageChunk notification
                        let notif = SessionNotification::new(
                            SessionId::new(session_id_for_update.clone()),
                            SessionUpdate::AgentMessageChunk(ContentChunk::new(
                                ContentBlock::Text(TextContent::new(token)),
                            )),
                        );
                        let _ = cx_clone.send_notification(notif);
                    }
                    full_response
                });

                // Wykonaj prompt przez agenta opencode-rs (ReAct loop z tools)
                let result = agent_prompt
                    .process_user_prompt(
                        &active_model,
                        &agent_mode,
                        &history,
                        &prompt_text,
                        token_tx,
                        ctx_tx,
                    )
                    .await;

                // Czekaj na zakończenie streamera
                let full_response = streamer.await.unwrap_or_default();

                // Zapisz historię sesji
                {
                    let mut sessions = sessions_save.lock().await;
                    if let Some(session) = sessions.get_mut(&session_id_for_save) {
                        session.history.push(ChatMessage {
                            role: "user".to_string(),
                            content: prompt_text,
                        });
                        session.history.push(ChatMessage {
                            role: "assistant".to_string(),
                            content: full_response,
                        });
                    }
                }

                let stop_reason = match result {
                    Ok(_) => StopReason::EndTurn,
                    Err(_) => StopReason::Refusal,
                };
                responder.respond(PromptResponse::new(stop_reason))
            },
            agent_client_protocol::on_receive_request!(),
        )
        // ── Handler: session/cancel (notification) ───────────────────────────
        // Klient anuluje bieżący prompt.
        .on_receive_notification(
            async move |_notif: CancelNotification, _cx| {
                // Na razie tylko logujemy — pełna anulacja wymagałaby CancellationToken
                // w process_user_prompt. TODO: dodać cancellation support.
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_to(Stdio::new())
        .await
        .map_err(|e| anyhow::anyhow!("ACP server error: {}", e))?;

    Ok(())
}

/// Wyciąga tekst z ContentBlocks (Text + Resource z tekstem).
fn extract_text_from_blocks(blocks: &[ContentBlock]) -> String {
    let mut text = String::new();
    for block in blocks {
        match block {
            ContentBlock::Text(t) => text.push_str(&t.text),
            ContentBlock::Resource(r) => {
                if let EmbeddedResourceResource::TextResourceContents(t) = &r.resource {
                    text.push_str(&t.text);
                }
            }
            _ => {}
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text_from_blocks_text() {
        let blocks = vec![ContentBlock::Text(TextContent::new("Hello world".to_string()))];
        assert_eq!(extract_text_from_blocks(&blocks), "Hello world");
    }

    #[test]
    fn test_extract_text_from_blocks_multiple() {
        let blocks = vec![
            ContentBlock::Text(TextContent::new("Hello ".to_string())),
            ContentBlock::Text(TextContent::new("world".to_string())),
        ];
        assert_eq!(extract_text_from_blocks(&blocks), "Hello world");
    }

    #[test]
    fn test_extract_text_from_blocks_empty() {
        let blocks: Vec<ContentBlock> = vec![];
        assert_eq!(extract_text_from_blocks(&blocks), "");
    }

    #[test]
    fn test_extract_text_from_blocks_mixed() {
        let blocks = vec![
            ContentBlock::Text(TextContent::new("Prompt: ".to_string())),
            ContentBlock::Text(TextContent::new("write a function".to_string())),
        ];
        assert_eq!(extract_text_from_blocks(&blocks), "Prompt: write a function");
    }
}
