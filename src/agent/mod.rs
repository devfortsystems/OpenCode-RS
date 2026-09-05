pub mod checkpoint;
pub mod context;
pub mod mcp;
pub mod subagent;
pub mod tools;

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

use crate::providers::{ChatMessage, ProviderRouter};
use context::ContextManager;
use mcp::McpManager;
use tools::{ToolCall, ToolEngine};

pub struct Agent {
    router: Arc<ProviderRouter>,
    tools: Arc<ToolEngine>,
    context: ContextManager,
    mcp: Arc<McpManager>,
    work_dir: PathBuf,
}

impl Agent {
    pub fn new(router: Arc<ProviderRouter>, work_dir: PathBuf) -> Self {
        let mcp = Arc::new(McpManager::load_from_project_or_global(&work_dir));
        Self {
            router,
            tools: Arc::new(ToolEngine::new(work_dir.clone())),
            context: ContextManager::new(work_dir.clone()),
            mcp,
            work_dir,
        }
    }

    pub fn context(&self) -> &ContextManager {
        &self.context
    }

    /// Wyciąga wywołania narzędzi z tekstu odpowiedzi modelu (<tool_call>...</tool_call>)
    pub fn extract_tool_calls(text: &str) -> Vec<ToolCall> {
        let mut calls = Vec::new();
        let mut remaining = text;

        while let Some(start_idx) = remaining.find("<tool_call>") {
            let after_start = &remaining[start_idx + "<tool_call>".len()..];
            if let Some(end_idx) = after_start.find("</tool_call>") {
                let json_str = after_start[..end_idx].trim();
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if let Some(name) = val.get("name").and_then(|n| n.as_str()) {
                        let args = val.get("arguments").cloned().unwrap_or(serde_json::json!({}));
                        calls.push(ToolCall {
                            name: name.to_string(),
                            arguments: args,
                        });
                    }
                }
                remaining = &after_start[end_idx + "</tool_call>".len()..];
            } else {
                break;
            }
        }

        calls
    }

    /// Autonomiczny proces: wysyła prompt, a jeśli model wywoła narzędzia – wykonuje je i kontynuuje (ReAct loop)
    pub async fn process_user_prompt(
        &self,
        active_model: &str,
        mode: &str,
        history: &[ChatMessage],
        user_input: &str,
        token_tx: Sender<String>,
        context_tx: Sender<(usize, usize)>,
    ) -> Result<String> {
        let system_prompt = self.context.build_system_prompt(active_model, mode);
        let resolved_prompt = self.context.resolve_smart_context(user_input);

        let mut messages = vec![ChatMessage {
            role: "system".to_string(),
            content: system_prompt,
        }];

        // Dołącz historię sesji
        messages.extend_from_slice(history);

        // Dołącz bieżący prompt
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: resolved_prompt,
        });

        let mut used_model = active_model.to_string();
        let max_tool_iterations = 6usize;

        for iteration in 0..max_tool_iterations {
            let (turn_tx, mut turn_rx) = tokio::sync::mpsc::channel::<String>(100);
            let token_tx_clone = token_tx.clone();

            // Przesyłaj tokeny do głównego strumienia UI
            let stream_forwarder = tokio::spawn(async move {
                let mut accumulated = String::new();
                while let Some(tok) = turn_rx.recv().await {
                    accumulated.push_str(&tok);
                    let _ = token_tx_clone.send(tok).await;
                }
                accumulated
            });

            used_model = self
                .router
                .stream_with_failover(active_model, &messages, turn_tx)
                .await?;

            let assistant_reply = stream_forwarder.await.unwrap_or_default();
            messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: assistant_reply.clone(),
            });

            // Sprawdź czy model zażądał wywołania narzędzia
            let tool_calls = Self::extract_tool_calls(&assistant_reply);
            if tool_calls.is_empty() {
                // Brak wywołań narzędzi – model zakończył odpowiedź
                break;
            }

            // Wykonaj wszystkie wykryte narzędzia
            for call in tool_calls {
                let _ = token_tx.send(format!("\n\n⚙️ **[Agent Narzędzie: `{}`]** Wykonywanie...\n", call.name)).await;
                
                let execution_result = match self.execute_tool_call(&call.name, &call.arguments) {
                    Ok(out) => {
                        let _ = token_tx.send(format!("✅ **Wynik `{}`:**\n```\n{}\n```\n", call.name, &out[..out.len().min(4000)])).await;
                        out
                    }
                    Err(err) => {
                        let err_msg = format!("Błąd wykonania narzędzia: {err}");
                        let _ = token_tx.send(format!("❌ **Błąd `{}`:** {}\n", call.name, err_msg)).await;
                        err_msg
                    }
                };

                // Odsyłamy wynik narzędzia do kontekstu modelu na kolejną iterację pętli ReAct
                messages.push(ChatMessage {
                    role: "user".to_string(),
                    content: format!(
                        "[Wynik narzędzia `{}` (iteracja {}/{}):\n{}\n\nKontynuuj zadanie programistyczne na podstawie powyższego wyniku.]",
                        call.name,
                        iteration + 1,
                        max_tool_iterations,
                        execution_result
                    ),
                });
            }

            // Wyślij realny rozmiar contextu do UI (pełne tool outputs, nie ucięte).
            // Token counter w UI będzie pokazywał realne zużycie context window.
            // Używaj TokenEstimator zamiast chars/4 — lepsza estymacja dla kodu.
            let total_chars: usize = messages.iter().map(|m| m.content.len()).sum();
            let total_tokens: usize = crate::cost::TokenEstimator::estimate_messages(&messages);
            let _ = context_tx.send((total_chars, total_tokens)).await;

            let _ = token_tx.send("\n🔄 **[Agent]:** Analizowanie wyników narzędzi i kontynuacja...\n".to_string()).await;
        }

        Ok(used_model)
    }

    /// Wykonuje polecenie narzędzia wbudowanego lub zarejestrowanego serwera MCP
    pub fn execute_tool_call(&self, tool_name: &str, args: &serde_json::Value) -> Result<String> {
        // 1. Sprawdź czy to wywołanie narzędzia serwera MCP (format: "mcp__server__tool" lub server_name w args)
        if tool_name.starts_with("mcp__") {
            let parts: Vec<&str> = tool_name.split("__").collect();
            if parts.len() >= 3 {
                let server_name = parts[1];
                let mcp_tool = parts[2..].join("__");
                return self.mcp.execute_mcp_tool(server_name, &mcp_tool, args);
            }
        }

        // 2. Wbudowane narzędzia agenta
        match tool_name {
            "list_files" | "tree" | "dir" => {
                let subpath = args.get("subpath").and_then(|v| v.as_str());
                let depth = args.get("depth").and_then(|v| v.as_u64()).map(|d| d as usize);
                self.tools.list_files(subpath, depth)
            }
            "read_file" | "read" | "view_file" => {
                let path = args
                    .get("file_path")
                    .or_else(|| args.get("path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let start = args.get("start_line").and_then(|v| v.as_u64()).map(|d| d as usize);
                let end = args.get("end_line").and_then(|v| v.as_u64()).map(|d| d as usize);
                self.tools.read_file(path, start, end)
            }
            "edit_file" | "replace" | "patch" => {
                let path = args
                    .get("file_path")
                    .or_else(|| args.get("path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let target = args
                    .get("target_content")
                    .or_else(|| args.get("old_str"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let replacement = args
                    .get("replacement_content")
                    .or_else(|| args.get("new_str"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                self.tools.edit_file(path, target, replacement)
            }
            "write_file" | "write" | "create_file" => {
                let path = args
                    .get("file_path")
                    .or_else(|| args.get("path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let content = args.get("content").and_then(|v| v.as_str()).unwrap_or_default();
                self.tools.write_file(path, content)
            }
            "bash_exec" | "terminal" | "exec" | "run_command" => {
                let command = args
                    .get("command")
                    .or_else(|| args.get("cmd"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                self.tools.bash_exec(command)
            }
            "grep_search" | "grep" | "search" => {
                let query = args
                    .get("query")
                    .or_else(|| args.get("pattern"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                self.tools.grep_search(query)
            }
            "core_memory_append" => {
                let label = args.get("label").and_then(|v| v.as_str()).unwrap_or_default();
                let content = args.get("content").and_then(|v| v.as_str()).unwrap_or_default();
                if label.is_empty() || content.is_empty() {
                    return Err(anyhow::anyhow!("core_memory_append wymaga 'label' (persona|human|project) i 'content'"));
                }
                let mb = crate::memory::MemoryBlocks::new(self.work_dir.clone());
                let new_content = mb.append_block(label, content)?;
                Ok(format!("✅ Dopisano do bloku pamięci '{label}' (teraz {} znaków).\nNowa treść:\n{}", new_content.len(), new_content))
            }
            "core_memory_replace" => {
                let label = args.get("label").and_then(|v| v.as_str()).unwrap_or_default();
                let old_str = args.get("old_str").and_then(|v| v.as_str()).unwrap_or_default();
                let new_str = args.get("new_str").and_then(|v| v.as_str()).unwrap_or_default();
                if label.is_empty() || old_str.is_empty() {
                    return Err(anyhow::anyhow!("core_memory_replace wymaga 'label' (persona|human|project), 'old_str' i 'new_str'"));
                }
                let mb = crate::memory::MemoryBlocks::new(self.work_dir.clone());
                let new_content = mb.replace_in_block(label, old_str, new_str)?;
                Ok(format!("✅ Zastąpiono fragment w bloku pamięci '{label}' (teraz {} znaków).\nNowa treść:\n{}", new_content.len(), new_content))
            }
            "create_skill" => {
                let name = args.get("name").and_then(|v| v.as_str()).unwrap_or_default();
                let content = args.get("content").and_then(|v| v.as_str()).unwrap_or_default();
                if name.is_empty() || content.is_empty() {
                    return Err(anyhow::anyhow!("create_skill wymaga 'name' i 'content' (treść SKILL.md)"));
                }
                let sm = crate::skills::SkillsManager::new(self.work_dir.clone());
                let path = sm.create_learned_skill(name, content)?;
                Ok(format!("✅ Utworzono learned skill '{name}' → {}\nSkill będzie automatycznie wczytywany w przyszłych sesjach.", path.display()))
            }
            // ── Plan projektu (persistentny, per-projekt) ──────────────────
            "plan_set" => {
                let goal = args.get("goal").and_then(|v| v.as_str()).unwrap_or_default();
                if goal.is_empty() {
                    return Err(anyhow::anyhow!("plan_set wymaga 'goal' (główny cel planu)"));
                }
                let mut plan = crate::memory::ProjectPlan::load(&self.work_dir);
                plan.set_goal(goal);
                plan.save(&self.work_dir)?;
                Ok(format!("✅ Ustawiono cel planu: {}\nPlan zapisany w .opencode/plan.md — przetrwa restart UI.", plan.goal))
            }
            "plan_add_step" => {
                let description = args.get("description").and_then(|v| v.as_str()).unwrap_or_default();
                if description.is_empty() {
                    return Err(anyhow::anyhow!("plan_add_step wymaga 'description' (opis kroku)"));
                }
                let mut plan = crate::memory::ProjectPlan::load(&self.work_dir);
                plan.add_step(description);
                let n = plan.steps.len();
                plan.save(&self.work_dir)?;
                Ok(format!("✅ Dodano krok {n}: {description}\nPlan ma teraz {n} kroków."))
            }
            "plan_complete_step" => {
                let step_number = args.get("step_number")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| anyhow::anyhow!("plan_complete_step wymaga 'step_number' (liczba >= 1)"))?
                    as usize;
                let mut plan = crate::memory::ProjectPlan::load(&self.work_dir);
                plan.toggle_step(step_number)?;
                let (total, done) = plan.stats();
                plan.save(&self.work_dir)?;
                let step = &plan.steps[step_number - 1];
                let status = if step.completed { "ukończony ✅" } else { "cofnięty ⬜" };
                Ok(format!("✅ Krok {step_number}: {status}\nPostęp: {done}/{total} kroków ukończonych."))
            }
            "plan_update_step" => {
                let step_number = args.get("step_number")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| anyhow::anyhow!("plan_update_step wymaga 'step_number' (liczba >= 1)"))?
                    as usize;
                let description = args.get("description").and_then(|v| v.as_str()).unwrap_or_default();
                if description.is_empty() {
                    return Err(anyhow::anyhow!("plan_update_step wymaga 'description' (nowy opis)"));
                }
                let mut plan = crate::memory::ProjectPlan::load(&self.work_dir);
                plan.update_step(step_number, description)?;
                plan.save(&self.work_dir)?;
                Ok(format!("✅ Zaktualizowano krok {step_number}: {}", plan.steps[step_number - 1].description))
            }
            "plan_add_note" => {
                let note = args.get("note").and_then(|v| v.as_str()).unwrap_or_default();
                if note.is_empty() {
                    return Err(anyhow::anyhow!("plan_add_note wymaga 'note' (treść notatki)"));
                }
                let mut plan = crate::memory::ProjectPlan::load(&self.work_dir);
                plan.add_note(note);
                plan.save(&self.work_dir)?;
                Ok(format!("✅ Dodano notatkę do planu:\n{note}"))
            }
            "plan_clear" => {
                let mut plan = crate::memory::ProjectPlan::load(&self.work_dir);
                plan.clear();
                plan.save(&self.work_dir)?;
                Ok("✅ Wyczyszczono cały plan (.opencode/plan.md).".to_string())
            }
            // ── Archival memory (wektorowa pamięć długoterminowa) ──────────
            "archival_search" => {
                let query = args.get("query").and_then(|v| v.as_str()).unwrap_or_default();
                let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
                if query.is_empty() {
                    return Err(anyhow::anyhow!("archival_search wymaga 'query' (tekst do wyszukania)"));
                }
                let am = crate::archival::ArchivalMemory::open(&self.work_dir)?;
                let hits = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(am.search(query, top_k))
                })?;
                if hits.is_empty() {
                    return Ok("Brak wyników w archival memory.".to_string());
                }
                let mut result = format!("Archival memory — {} wyników dla '{query}':\n\n", hits.len());
                for (i, h) in hits.iter().enumerate() {
                    result.push_str(&format!(
                        "### {} [score={:.3}] id={}\nlabels: {}\n{}\n\n---\n\n",
                        i + 1, h.score, h.id, h.labels.join(", "), h.content
                    ));
                }
                Ok(result)
            }
            "archival_add" => {
                let content = args.get("content").and_then(|v| v.as_str()).unwrap_or_default();
                let labels = args.get("labels")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let node_type = args.get("node_type").and_then(|v| v.as_str()).unwrap_or("fact");
                if content.is_empty() {
                    return Err(anyhow::anyhow!("archival_add wymaga 'content' (wiedza do zapisania)"));
                }
                let am = crate::archival::ArchivalMemory::open(&self.work_dir)?;
                let id = format!("arch_{}", uuid::Uuid::new_v4().simple());
                let entry = crate::archival::ArchivalEntry {
                    id: id.clone(),
                    content: content.to_string(),
                    labels,
                    node_type: node_type.to_string(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                    source: "agent".to_string(),
                };
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(am.add(&entry))
                })?;
                let count = am.count();
                Ok(format!("✅ Zapisano w archival memory (id={id}). W bazie: {count} wpisów."))
            }
            "archival_list" => {
                let am = crate::archival::ArchivalMemory::open(&self.work_dir)?;
                let list = am.list()?;
                if list.is_empty() {
                    return Ok("Archival memory jest pusta.".to_string());
                }
                let mut result = format!("Archival memory — {} wpisów:\n\n", list.len());
                for e in &list {
                    result.push_str(&format!(
                        "• [{}] {} (labels: {})\n  {}\n\n",
                        e.id, e.node_type, e.labels.join(", "), e.content.chars().take(100).collect::<String>()
                    ));
                }
                Ok(result)
            }
            _ => {
                // Sprawdź fallback do narzędzia MCP jeśli przekazano pole "server"
                if let Some(server) = args.get("server").and_then(|s| s.as_str()) {
                    return self.mcp.execute_mcp_tool(server, tool_name, args);
                }
                Err(anyhow::anyhow!("Nieznane narzędzie agenta: '{}'", tool_name))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tool_calls() {
        let sample = r#"
Oto moje wyjaśnienie problemu. Muszę zmodyfikować plik.
<tool_call>
{"name": "write_file", "arguments": {"file_path": "test.txt", "content": "Hello World"}}
</tool_call>
A teraz sprawdzę status:
<tool_call>
{"name": "bash_exec", "arguments": {"command": "dir"}}
</tool_call>
"#;
        let calls = Agent::extract_tool_calls(sample);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "write_file");
        assert_eq!(calls[0].arguments["file_path"], "test.txt");
        assert_eq!(calls[1].name, "bash_exec");
        assert_eq!(calls[1].arguments["command"], "dir");
    }
}
