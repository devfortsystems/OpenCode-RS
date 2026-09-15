use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use uuid::Uuid;

use crate::agent::Agent;
use crate::app::AppEvent;
use crate::opencode_compat::LoadedAgent;
use crate::providers::ProviderRouter;

#[derive(Debug, Clone)]
pub struct SubagentTask {
    pub id: String,
    pub task_description: String,
    pub status: String, // "running", "completed", "failed"
    pub created_at: String,
}

pub struct SubagentManager {
    work_dir: PathBuf,
    router: Arc<ProviderRouter>,
}

impl SubagentManager {
    pub fn new(work_dir: PathBuf, router: Arc<ProviderRouter>) -> Self {
        Self { work_dir, router }
    }

    /// Uruchamia nazwanego subagenta z opencode/commandcode agents (.opencode/agents/*.md).
    /// Subagent dostaje własny system prompt (z pliku .md), własny model (jeśli zdefiniowany),
    /// własne uprawnienia (permission), i wykonuje się w osobnej pętli ReAct.
    /// Wynik jest wstrzykiwany do głównej sesji przez AppEvent::StatusNotification.
    pub fn spawn_named_subagent(
        &self,
        agent: &LoadedAgent,
        task: &str,
        active_model: &str,
        event_tx: Sender<AppEvent>,
    ) -> String {
        let task_id = format!("subagent-{}-{}", agent.name, &Uuid::new_v4().to_string()[..8]);
        let subagent = Arc::new(Agent::new(self.router.clone(), self.work_dir.clone()));
        let id_clone = task_id.clone();
        let agent_name = agent.name.clone();
        let agent_prompt = agent.prompt.clone();
        let agent_model = agent.model.clone().unwrap_or_else(|| active_model.to_string());
        let agent_mode = agent.mode.clone();
        let task_text = task.to_string();

        tokio::spawn(async move {
            let (token_tx, mut token_rx) = tokio::sync::mpsc::channel::<String>(100);

            // Informacja o rozpoczęciu
            let _ = event_tx
                .send(AppEvent::StatusNotification(format!(
                    "👥 [Subagent @{agent_name} `{id_clone}`]: Rozpoczęto: _{task_text}_\nModel: {agent_model}"
                )))
                .await;

            // Buduj prompt subagenta: jego własny prompt + zadanie
            let full_prompt = format!(
                "{agent_prompt}\n\n---\nZADANIE UŻYTKOWNIKA:\n{task_text}\n\nWykonaj zadanie autonomicznie używając dostępnych narzędzi. Na końcu przedstaw zwięzły raport."
            );

            let (ctx_tx, _ctx_rx) = tokio::sync::mpsc::channel::<(usize, usize)>(10);
            let res = subagent
                .process_user_prompt_with_agent(
                    &agent_model,
                    &agent_mode,
                    &[],
                    &full_prompt,
                    token_tx,
                    ctx_tx,
                    Some(&agent_prompt),
                )
                .await;

            // Zbierz wynik
            let mut final_response = String::new();
            while let Ok(token) = token_rx.try_recv() {
                final_response.push_str(&token);
            }

            match res {
                Ok(_) => {
                    let summary = if final_response.trim().is_empty() {
                        "Zadanie zrealizowane pomyślnie.".to_string()
                    } else if final_response.len() > 2000 {
                        format!("{}...\n[pełny raport w logach]", &final_response[..2000])
                    } else {
                        final_response
                    };

                    let _ = event_tx
                        .send(AppEvent::StatusNotification(format!(
                            "✅ [Subagent @{agent_name} UKOŃCZYŁ]:\n{summary}"
                        )))
                        .await;
                }
                Err(e) => {
                    let _ = event_tx
                        .send(AppEvent::StatusNotification(format!(
                            "❌ [Subagent @{agent_name} BŁĄD]: {e}"
                        )))
                        .await;
                }
            }
        });

        task_id
    }

    /// Uruchamia nowego podagenta w tle do wykonania równoległego zadania
    pub fn spawn_task(
        &self,
        task_prompt: String,
        active_model: String,
        event_tx: Sender<AppEvent>,
    ) -> String {
        let task_id = format!("subagent-{}", &Uuid::new_v4().to_string()[..8]);
        let agent = Arc::new(Agent::new(self.router.clone(), self.work_dir.clone()));
        let id_clone = task_id.clone();
        let prompt_clone = task_prompt.clone();

        tokio::spawn(async move {
            let (token_tx, mut token_rx) = tokio::sync::mpsc::channel::<String>(100);

            // Informacja o rozpoczęciu
            let _ = event_tx
                .send(AppEvent::StatusNotification(format!(
                    "👥 [Podagent `{id_clone}`]: Rozpoczęto zadanie w tle: _{prompt_clone}_"
                )))
                .await;

            let prompt_for_agent = format!(
                "Jesteś wyspecjalizowanym podagentem (Subagent). Twoje zadanie: {prompt_clone}\nWykonaj je autonomicznie, używając dostępnych narzędzi, a na końcu przedstaw zwięzły raport."
            );

            let (ctx_tx, _ctx_rx) = tokio::sync::mpsc::channel::<(usize, usize)>(10);
            let res = agent
                .process_user_prompt(
                    &active_model,
                    "coder",
                    &[],
                    &prompt_for_agent,
                    token_tx,
                    ctx_tx,
                )
                .await;

            let mut final_response = String::new();
            while let Ok(token) = token_rx.try_recv() {
                final_response.push_str(&token);
            }

            match res {
                Ok(_) => {
                    let summary = if final_response.trim().is_empty() {
                        "Zadanie zrealizowane pomyślnie.".to_string()
                    } else if final_response.len() > 1200 {
                        format!("{}...\n[zobacz raport]", &final_response[..1200])
                    } else {
                        final_response
                    };

                    let _ = event_tx
                        .send(AppEvent::StatusNotification(format!(
                            "✅ [Podagent `{id_clone}` UKOŃCZYŁ ZADANIE]:\n{summary}"
                        )))
                        .await;
                }
                Err(e) => {
                    let _ = event_tx
                        .send(AppEvent::StatusNotification(format!(
                            "❌ [Podagent `{id_clone}` BŁĄD]: {e}"
                        )))
                        .await;
                }
            }
        });

        task_id
    }
}
