use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;
use uuid::Uuid;

use crate::agent::Agent;
use crate::app::AppEvent;
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
