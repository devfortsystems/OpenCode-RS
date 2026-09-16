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
    /// Uprawnienia z opencode.json/commandcode (ask/allow/deny per tool).
    permissions: Option<crate::opencode_compat::PermissionConfig>,
    /// Pluginy opencode (.opencode/plugins/*.js|ts) — uruchamiane przez node.
    plugins: Vec<crate::opencode_compat::LoadedPlugin>,
    /// Mody commandcode (.commandcode/mods/*.ts) — uruchamiane przez node.
    mods: Vec<crate::opencode_compat::LoadedMod>,
    /// Kanał do TUI dla żądań uprawnień (permission "ask").
    /// None = tryb headless (ask = allow).
    permission_tx: Option<tokio::sync::mpsc::Sender<crate::app::AppEvent>>,
    /// Persistent bridge dla modów commandcode (lazy init).
    /// Mod bridges są uruchamiane raz i utrzymywane przez całą sesję.
    mod_bridges: std::sync::Mutex<Vec<std::sync::Arc<std::sync::Mutex<crate::mod_bridge::ModBridge>>>>,
}

impl Agent {
    pub fn new(router: Arc<ProviderRouter>, work_dir: PathBuf) -> Self {
        let mcp = Arc::new(McpManager::load_from_project_or_global(&work_dir));
        let compat = crate::opencode_compat::OpenCodeCompat::load(&work_dir);
        Self {
            router,
            tools: Arc::new(ToolEngine::new(work_dir.clone())),
            context: ContextManager::new(work_dir.clone()),
            mcp,
            work_dir,
            permissions: compat.permissions.clone().into(),
            plugins: compat.plugins.clone(),
            mods: compat.mods.clone(),
            permission_tx: None,
            mod_bridges: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Ustaw kanał do TUI dla żądań uprawnień (permission "ask").
    pub fn set_permission_channel(&mut self, tx: tokio::sync::mpsc::Sender<crate::app::AppEvent>) {
        self.permission_tx = Some(tx);
    }

    /// Sprawdza uprawnienie dla toola. Zwraca "allow", "deny", lub "ask".
    pub fn check_tool_permission(&self, tool_name: &str) -> &str {
        match &self.permissions {
            Some(p) => match tool_name {
                "edit_file" | "replace" | "patch" | "write_file" | "create_file" => {
                    p.edit.as_deref().unwrap_or("ask")
                }
                "bash_exec" | "bash" | "shell" | "run_command" => {
                    p.bash.as_deref().unwrap_or("ask")
                }
                "web_fetch" | "webfetch" | "fetch" => {
                    p.webfetch.as_deref().unwrap_or("ask")
                }
                _ => "allow", // read-only tools zawsze allow
            },
            None => "allow", // brak konfiguracji = pełny dostęp (domyślne zachowanie)
        }
    }

    /// Uruchamia plugin opencode przez node (bezpiecznie — błędy nie przerywają agenta).
    fn execute_plugin_safe(&self, plugin: &crate::opencode_compat::LoadedPlugin, hook: &str, payload: &serde_json::Value) -> Result<String> {
        if plugin.path.as_os_str().is_empty() {
            return Ok(String::new()); // npm plugin — pomiń (wymaga instalacji)
        }
        let compat = crate::opencode_compat::OpenCodeCompat::load(&self.work_dir);
        compat.execute_plugin(&plugin.path, hook, payload)
    }

    /// Uruchamia mod commandcode przez persistent ModBridge (pełny ModApi).
    /// Bridge jest uruchamiany raz (lazy init) i utrzymywany przez całą sesję.
    fn execute_mod_safe(&self, m: &crate::opencode_compat::LoadedMod, event: &str, payload: &serde_json::Value) -> Result<String> {
        // Spróbuj użyć istniejącego bridge dla tego moda
        let bridges = self.mod_bridges.lock().unwrap();
        for bridge_arc in bridges.iter() {
            if bridge_arc.lock().unwrap().mod_path == m.path {
                let mut bridge = bridge_arc.lock().unwrap();
                match bridge.send_event(event, payload) {
                    Ok(result) => return Ok(serde_json::to_string(&result).unwrap_or_default()),
                    Err(e) => return Err(e),
                }
            }
        }
        drop(bridges);

        // Bridge nie istnieje — uruchom nowy
        match crate::mod_bridge::ModBridge::start(&m.path, &self.work_dir) {
            Ok(bridge) => {
                let bridge_arc = std::sync::Arc::new(std::sync::Mutex::new(bridge));
                self.mod_bridges.lock().unwrap().push(bridge_arc.clone());
                let mut b = bridge_arc.lock().unwrap();
                match b.send_event(event, payload) {
                    Ok(result) => Ok(serde_json::to_string(&result).unwrap_or_default()),
                    Err(e) => Err(e),
                }
            }
            Err(e) => Err(e),
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
        self.process_user_prompt_with_agent(active_model, mode, history, user_input, token_tx, context_tx, None).await
    }

    /// process_user_prompt z opcjonalnym promptem agenta (z opencode/commandcode agents).
    pub async fn process_user_prompt_with_agent(
        &self,
        active_model: &str,
        mode: &str,
        history: &[ChatMessage],
        user_input: &str,
        token_tx: Sender<String>,
        context_tx: Sender<(usize, usize)>,
        agent_prompt: Option<&str>,
    ) -> Result<String> {
        let system_prompt = self.context.build_system_prompt_with_agent(active_model, mode, agent_prompt);
        let mut resolved_prompt = self.context.resolve_smart_context(user_input);

        // ── Plugin/Mod hooks: transformInput ──
        // Mody commandcode mogą transformować prompt przed wysłaniem do modelu.
        // Hook zwraca nowy prompt (jeśli nie pusty) lub undefined (pozostaw oryginalny).
        let transform_payload = serde_json::json!({
            "text": &resolved_prompt,
            "session_id": "current",
        });
        for m in &self.mods {
            if let Ok(transformed) = self.execute_mod_safe(m, "transformInput", &transform_payload) {
                if !transformed.trim().is_empty() && !transformed.contains("undefined") {
                    resolved_prompt = transformed;
                    break; // pierwszy mod który transformuje wygrywa
                }
            }
        }

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
                // ── Plugin/Mod hooks: tool.execute.before ──
                // Uruchom pluginy opencode i mody commandcode przed wywołaniem toola.
                // Hook może zablokować wykonanie (zwrócić block: true).
                let hook_payload = serde_json::json!({
                    "tool": call.name,
                    "args": call.arguments,
                    "session_id": "current",
                });
                let mut blocked = false;
                // Pluginy opencode (.opencode/plugins/*.js|ts)
                for plugin in &self.plugins {
                    if let Ok(result) = self.execute_plugin_safe(plugin, "tool.execute.before", &hook_payload) {
                        if result.contains("\"block\":true") || result.contains("block: true") {
                            let _ = token_tx.send(format!("\n🚫 **[Plugin {}]** Zablokowano `{}`\n", plugin.name, call.name)).await;
                            blocked = true;
                            break;
                        }
                    }
                }
                // Mody commandcode (.commandcode/mods/*.ts)
                if !blocked {
                    for m in &self.mods {
                        if let Ok(result) = self.execute_mod_safe(m, "beforeToolCall", &hook_payload) {
                            if result.contains("\"block\":true") || result.contains("block: true") {
                                let _ = token_tx.send(format!("\n🚫 **[Mod {}]** Zablokowano `{}`\n", m.name, call.name)).await;
                                blocked = true;
                                break;
                            }
                        }
                    }
                }
                if blocked {
                    messages.push(ChatMessage {
                        role: "user".to_string(),
                        content: format!("[Narzędzie `{}` zablokowane przez plugin/mod hook. Kontynuuj bez tego narzędzia.]", call.name),
                    });
                    continue;
                }

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

                // ── Plugin/Mod hooks: tool.execute.after ──
                let after_payload = serde_json::json!({
                    "tool": call.name,
                    "args": call.arguments,
                    "result": &execution_result[..execution_result.len().min(2000)],
                    "session_id": "current",
                });
                for plugin in &self.plugins {
                    let _ = self.execute_plugin_safe(plugin, "tool.execute.after", &after_payload);
                }
                for m in &self.mods {
                    let _ = self.execute_mod_safe(m, "afterToolCall", &after_payload);
                }

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
        // 0. Sprawdź uprawnienia z opencode.json/commandcode (permission section)
        let perm = self.check_tool_permission(tool_name);
        if perm == "deny" {
            return Err(anyhow::anyhow!(
                "🚫 Narzędzie '{}' zablokowane przez konfigurację uprawnień (permission: deny).\n\
                 Zmień w opencode.json/commandcode.json: \"permission\": {{\"{}\": \"allow\"}}",
                tool_name,
                match tool_name {
                    "edit_file" | "replace" | "patch" | "write_file" | "create_file" => "edit",
                    "bash_exec" | "bash" | "shell" | "run_command" => "bash",
                    "web_fetch" | "webfetch" | "fetch" => "webfetch",
                    _ => "edit",
                }
            ));
        }
        // "ask" — w trybie TUI wyślij powiadomienie (nie blokuj!)
        // Pełny dialog wymagałby async execute_tool_call — to duża zmiana architektury.
        // Na razie: "ask" = allow (nie blokuje TUI), ale loguj że tool został wykonany.
        // TODO: async execute_tool_call z dialogiem uprawnień
        if perm == "ask" {
            if let Some(ref tx) = self.permission_tx {
                let args_summary = serde_json::to_string(args)
                    .unwrap_or_default()
                    .chars()
                    .take(200)
                    .collect::<String>();
                // Wyślij powiadomienie (nie blokuj) — użytkownik widzi co się dzieje
                let _ = tx.try_send(crate::app::AppEvent::StatusNotification(format!(
                    "⚠️ Wykonuję '{}' (permission: ask)\nArgs: {}",
                    tool_name, args_summary
                )));
            }
        }
        // "allow" — kontynuuj normalnie

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
                let res = self.tools.edit_file(path, target, replacement);
                if res.is_ok() {
                    if let Some(tx) = &self.permission_tx {
                        let full_path = self.work_dir.join(path);
                        let _ = tx.try_send(crate::app::AppEvent::FileModified(full_path));
                    }
                }
                res
            }
            "write_file" | "write" | "create_file" => {
                let path = args
                    .get("file_path")
                    .or_else(|| args.get("path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let content = args.get("content").and_then(|v| v.as_str()).unwrap_or_default();
                let res = self.tools.write_file(path, content);
                if res.is_ok() {
                    if let Some(tx) = &self.permission_tx {
                        let full_path = self.work_dir.join(path);
                        let _ = tx.try_send(crate::app::AppEvent::FileModified(full_path));
                    }
                }
                res
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
            "web_fetch" | "webfetch" | "fetch" => {
                let url = args
                    .get("url")
                    .or_else(|| args.get("uri"))
                    .or_else(|| args.get("link"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                let max_chars = args.get("max_chars").and_then(|v| v.as_u64()).map(|n| n as usize);
                if url.is_empty() {
                    return Err(anyhow::anyhow!("web_fetch wymaga parametru 'url'"));
                }
                self.tools.web_fetch(url, max_chars)
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
