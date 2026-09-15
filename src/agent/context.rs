use std::fs;
use std::path::PathBuf;

use crate::agent::mcp::McpManager;
use crate::git::GitAssistant;

pub struct ContextManager {
    work_dir: PathBuf,
}

impl ContextManager {
    pub fn new(work_dir: PathBuf) -> Self {
        Self { work_dir }
    }

    /// Automatycznie wykrywa technologie używane w projekcie
    pub fn detect_project_stack(&self) -> String {
        let mut stacks = Vec::new();

        if self.work_dir.join("Cargo.toml").exists() {
            stacks.push("Rust (Cargo)");
        }
        if self.work_dir.join("package.json").exists() {
            if self.work_dir.join("next.config.js").exists() || self.work_dir.join("next.config.ts").exists() {
                stacks.push("TypeScript (Next.js)");
            } else if self.work_dir.join("vite.config.js").exists() || self.work_dir.join("vite.config.ts").exists() {
                stacks.push("TypeScript (Vite)");
            } else {
                stacks.push("JavaScript/Node.js");
            }
        }
        if self.work_dir.join("pyproject.toml").exists() || self.work_dir.join("requirements.txt").exists() {
            stacks.push("Python");
        }
        if self.work_dir.join("go.mod").exists() {
            stacks.push("Go");
        }
        if self.work_dir.join("pubspec.yaml").exists() {
            stacks.push("Flutter/Dart");
        }
        if self.work_dir.join("Dockerfile").exists() || self.work_dir.join("docker-compose.yml").exists() {
            stacks.push("Docker");
        }

        if stacks.is_empty() {
            "Niezdefiniowany / Uniwersalny".to_string()
        } else {
            stacks.join(", ")
        }
    }

    /// Inteligentnie rozwija tagi @plik, @git, @tree, @terminal, @problems w tekście promptu użytkownika
    pub fn resolve_smart_context(&self, input: &str) -> String {
        let mut enriched = String::new();
        let mut appended_context = String::new();

        let words: Vec<&str> = input.split_whitespace().collect();

        for word in &words {
            if word.starts_with('@') && word.len() > 1 {
                let tag = &word[1..];

                if tag == "git" {
                    if let Ok((status, diff)) = GitAssistant::get_status_and_diff(&self.work_dir) {
                        appended_context.push_str("\n\n[Kontekst Git]:\n");
                        appended_context.push_str(&format!("Status zmienionych plików:\n{}\n\n", status));
                        if !diff.is_empty() {
                            let diff_trimmed = if diff.len() > 10000 {
                                format!("{}...\n[przycięto diff]", &diff[..10000])
                            } else {
                                diff
                            };
                            appended_context.push_str(&format!("Diff zmian:\n```diff\n{}\n```\n", diff_trimmed));
                        }
                    }
                    continue;
                }

                if tag == "tree" || tag == "dir" {
                    if let Ok(tree_out) = crate::agent::tools::ToolEngine::new(self.work_dir.clone()).list_files(None, Some(3)) {
                        appended_context.push_str("\n\n[Drzewo Projektu]:\n```\n");
                        appended_context.push_str(&tree_out);
                        appended_context.push_str("```\n");
                    }
                    continue;
                }

                if tag == "terminal" || tag == "term" {
                    // @terminal — ostatni output terminala (Cline/Roo)
                    let log = self.work_dir.join(".opencode").join("terminal.log");
                    if let Ok(c) = fs::read_to_string(&log) {
                        let tail = if c.len() > 8000 { &c[c.len() - 8000..] } else { &c };
                        appended_context.push_str(&format!("\n\n[Terminal Output @terminal]:\n```\n{}\n```\n", tail));
                    } else {
                        appended_context.push_str("\n\n[Terminal Output @terminal]: (brak .opencode/terminal.log — uruchom !<cmd>)\n");
                    }
                    continue;
                }

                if tag == "problems" || tag == "diagnostics" || tag == "lsp" {
                    // @problems — diagnostyka LSP (VS Code)
                    let diag = self.load_problems_context();
                    if !diag.is_empty() {
                        appended_context.push_str(&diag);
                    }
                    continue;
                }

                // @agent-name — delegacja do subagenta (opencode/commandcode)
                // Jeśli tag pasuje do nazwy subagenta, wstrzyknij informację o delegacji.
                // Pełne uruchomienie subagenta odbywa się w App::submit_prompt przez SubagentManager.
                let compat = crate::opencode_compat::OpenCodeCompat::load(&self.work_dir);
                if let Some(agent) = compat.subagents().iter().find(|a| a.name == tag) {
                    appended_context.push_str(&format!(
                        "\n\n[Delegacja do subagenta @{} ({}): {}]\nSubagent zostanie uruchomiony z własną pętlą ReAct.\nPrompt subagenta: {}\n",
                        agent.name, agent.source_tool, agent.description, agent.prompt
                    ));
                    if let Some(model) = &agent.model {
                        appended_context.push_str(&format!("Model subagenta: {}\n", model));
                    }
                    continue;
                }

                // Sprawdź czy to ścieżka do pliku
                let file_path = self.work_dir.join(tag);
                if file_path.exists() && file_path.is_file() {
                    if let Ok(content) = fs::read_to_string(&file_path) {
                        let trimmed_content = if content.len() > 15000 {
                            format!("{}...\n[przycięto plik]", &content[..15000])
                        } else {
                            content
                        };
                        appended_context.push_str(&format!("\n\n[Zawartość pliku `{}`]:\n```\n{}\n```\n", tag, trimmed_content));
                    }
                }
            }
        }

        enriched.push_str(input);
        if !appended_context.is_empty() {
            enriched.push_str(&appended_context);
        }

        enriched
    }

    pub fn load_taste_context(&self) -> String {
        let tm = crate::taste::TasteManager::new(self.work_dir.clone());
        if !tm.is_enabled() {
            return String::new();
        }
        let c = tm.collect_taste_content();
        if c.is_empty() {
            String::new()
        } else {
            format!("\n[CommandCode TASTE-1 (taste.md) — personal coding taste, do NOT ignore]:\n{}\n", c)
        }
    }

    pub fn load_skills_context(&self) -> String {
        crate::skills::SkillsManager::new(self.work_dir.clone()).collect_skills_context()
    }

    pub fn load_problems_context(&self) -> String {
        // Zbierz diagnostykę z LSP + cargo check
        let mut out = String::new();
        // cargo check cache w .opencode/problems.log (zapisywany przez hooks)
        let probs = self.work_dir.join(".opencode").join("problems.log");
        if let Ok(c) = fs::read_to_string(&probs) {
            out.push_str(&format!("\n[Diagnostics @problems {}]:\n```\n{}\n```\n", probs.display(), &c[..c.len().min(8000)]));
        }
        // Dodatkowo: szybki cargo check live jeśli brak pliku
        if out.is_empty() {
            if let Ok(o) = std::process::Command::new("cargo").arg("check").arg("--message-format=short").current_dir(&self.work_dir).output() {
                let s = String::from_utf8_lossy(&o.stderr).to_string();
                if !s.trim().is_empty() {
                    out.push_str(&format!("\n[Diagnostics cargo check @problems]:\n```\n{}\n```\n", &s[..s.len().min(6000)]));
                }
            }
        }
        out
    }

    pub fn load_memories_context(&self) -> String {
        // Windsurf Memories + Trae BUILD + generic memories
        let mut out = String::new();
        for p in [
            self.work_dir.join(".windsurf").join("memories.md"),
            self.work_dir.join(".trae").join("memories.md"),
            self.work_dir.join(".memory").join("memories.md"),
            self.work_dir.join("MEMORIES.md"),
            self.work_dir.join(".roo").join("memories.md"),
            self.work_dir.join(".cline").join("memories.md"),
        ] {
            if let Ok(c) = fs::read_to_string(&p) {
                out.push_str(&format!("\n[Memories {}]:\n{}\n", p.display(), c.trim()));
            }
        }
        out
    }

    pub fn load_project_rules(&self) -> String {
        let mut rules = String::new();

        // 1. .cursorrules & .windsurfrules
        let cursorrules_path = self.work_dir.join(".cursorrules");
        if let Ok(content) = fs::read_to_string(&cursorrules_path) {
            rules.push_str(&format!("\n[Cursor Rules (.cursorrules)]:\n{}\n", content.trim()));
        }

        let windsurfrules_path = self.work_dir.join(".windsurfrules");
        if let Ok(content) = fs::read_to_string(&windsurfrules_path) {
            rules.push_str(&format!("\n[Windsurf Rules (.windsurfrules)]:\n{}\n", content.trim()));
        }

        // 2. AGENTS.md, CLAUDE.md, PROJECT.md
        for doc in &["AGENTS.md", "CLAUDE.md", "PROJECT.md", "CODING_STANDARDS.md"] {
            let doc_path = self.work_dir.join(doc);
            if let Ok(content) = fs::read_to_string(&doc_path) {
                rules.push_str(&format!("\n[Project Guidelines ({doc})]:\n{}\n", content.trim()));
            }
        }

        // 3. .opencode/rules.md oraz pliki w .opencode/rules/*.md
        let opencode_rules_file = self.work_dir.join(".opencode").join("rules.md");
        if let Ok(content) = fs::read_to_string(&opencode_rules_file) {
            rules.push_str(&format!("\n[OpenCode Project Rules]:\n{}\n", content.trim()));
        }

        let opencode_rules_dir = self.work_dir.join(".opencode").join("rules");
        if let Ok(entries) = fs::read_dir(&opencode_rules_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "md") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let fname = path.file_name().unwrap_or_default().to_string_lossy();
                        rules.push_str(&format!("\n[Rule: {fname}]:\n{}\n", content.trim()));
                    }
                }
            }
        }

        // 4. .github/copilot-instructions.md
        let copilot_rules = self.work_dir.join(".github").join("copilot-instructions.md");
        if let Ok(content) = fs::read_to_string(&copilot_rules) {
            rules.push_str(&format!("\n[GitHub Copilot Instructions]:\n{}\n", content.trim()));
        }

        // 5. Kiro Agent Hooks (.kiro/hooks/*.md) i Steering (.kiro/steering/*.md)
        let kiro_rules = crate::importer::OpenCodeMigration::load_kiro_rules(&self.work_dir);
        if !kiro_rules.is_empty() {
            rules.push_str(&kiro_rules);
        }

        if rules.is_empty() {
            "Domyślne standardy czystego kodu (brak zdefiniowanych plików reguł)".to_string()
        } else {
            rules
        }
    }

    /// Szacuje liczbę tokenów w historii (1 token ≈ 4 znaki)
    pub fn estimate_tokens(messages: &[crate::providers::ChatMessage]) -> usize {
        messages.iter().map(|m| m.content.len() / 4).sum()
    }

    /// Kompresuje kontekst – streszcza starą historię gdy przekracza limit
    pub fn compress_context_if_needed(
        messages: &mut Vec<crate::providers::ChatMessage>,
        max_tokens: usize,
    ) -> bool {
        let current = Self::estimate_tokens(messages);
        if current <= max_tokens {
            return false;
        }

        // Zachowaj: system prompt (idx 0) + ostatnie 6 wiadomości
        if messages.len() <= 8 {
            return false;
        }

        let keep_recent = 6usize;
        let system_prompt = messages.first().cloned();
        let recent: Vec<_> = messages.iter().rev().take(keep_recent).rev().cloned().collect();
        let old: Vec<_> = messages.iter().skip(1).take(messages.len() - 1 - keep_recent).collect();

        // Twórz streszczenie starych wiadomości
        let summary_content = format!(
            "[Skompresowana historia – {} wiadomości, ~{} tokenów]:\n{}",
            old.len(),
            old.iter().map(|m| m.content.len() / 4).sum::<usize>(),
            old.iter()
                .map(|m| format!("[{}]: {}...", m.role, &m.content[..m.content.len().min(200)]))
                .collect::<Vec<_>>()
                .join("\n")
        );

        messages.clear();
        if let Some(sys) = system_prompt {
            messages.push(sys);
        }
        messages.push(crate::providers::ChatMessage {
            role: "system".to_string(),
            content: summary_content,
        });
        messages.extend(recent);

        true
    }

    pub fn build_system_prompt(&self, active_operator: &str, mode: &str) -> String {
        self.build_system_prompt_with_agent(active_operator, mode, None)
    }

    /// Buduje system prompt z opcjonalnym promptem agenta (z opencode/commandcode agents).
    /// `agent_prompt` — dodatkowy prompt z `.opencode/agents/*.md` lub `.commandcode/agents/*.md`.
    pub fn build_system_prompt_with_agent(&self, active_operator: &str, mode: &str, agent_prompt: Option<&str>) -> String {
        let project_rules = self.load_project_rules();
        let stack = self.detect_project_stack();
        let current_branch = GitAssistant::get_status_and_diff(&self.work_dir)
            .map(|_| "git repository active")
            .unwrap_or("no git");

        let mcp = McpManager::load_from_project_or_global(&self.work_dir);
        let mcp_report = mcp.get_status_report();

        let taste_ctx = self.load_taste_context();
        let skills_ctx = self.load_skills_context();
        let memories_ctx = self.load_memories_context();
        let memory_blocks = crate::memory::MemoryBlocks::new(self.work_dir.clone()).inject_into_prompt();
        let project_plan = crate::memory::ProjectPlan::load(&self.work_dir).to_prompt_section();
        let agent_section = match agent_prompt {
            Some(p) if !p.is_empty() => format!("AGENT (opencode/commandcode):\n{}\n\n", p),
            _ => String::new(),
        };
        let mode_instructions = match mode {
            "architect" => "TRYB ARCHITEKTA: Skup się na planowaniu, projektowaniu architektury, modularności i analizie zależności. Przygotuj plan działania przed wprowadzaniem zmian.",
            "ask" => "TRYB PYTANIA (Read-only): Odpowiadaj na pytania i wyjaśniaj kod. Nie proponuj modyfikacji plików dopóki użytkownik o to wprost nie poprosi.",
            "interactive" => "TRYB INTERACTIVE (Cline-like): Działaj interaktywnie — przed każdym bash_exec/edit_file pokaż inline diff i proś o potwierdzenie (Enter). Tryb idealny do pełnej kontroli, jak Cline Act z approval.",
            "auto" => "TRYB AUTO (Unattended/Nadzorowany): Pełna autonomia — wykonuj narzędzia bez pytań, trust_mode=ON, maksymalnie 6 iteracji ReAct, jak Cline auto-approve + Trae BUILD.",
            _ => "TRYB KODERA (Domyślny): Analizuj kod, precyzyjnie edytuj pliki i rozwiązuj zgłoszone zadania programistyczne.",
        };

        format!(
            r#"Jesteś OpenCode-RS – zaawansowanym, autonomicznym terminalowym agentem programistycznym (AI Software Engineer) napisanym w 100% w Rust.
Działasz bezpośrednio w terminalu programisty.
Aktywny operator: {active_operator}
Tryb: [{mode}] - {mode_instructions}

KONTEKST PROJEKTU:
- Katalog projektu: {work_dir}
- Wykryty stos technologiczny: {stack}
- Stan repozytorium: {current_branch}

REGUŁY I WYTYCZNE PROJEKTU:
{project_rules}

TASTE-1 (CommandCode personal taste):
{taste_ctx}

SKILLS (Roo/Cline/Trae):
{skills_ctx}

MEMORIES (Windsurf/Trae):
{memories_ctx}

MEMORY BLOCKS (Letta-style, edytowalne — ucz się między sesjami):
{memory_blocks}

PLAN PROJEKTU (persistentny, per-projekt — `.opencode/plan.md`):
{project_plan}

{agent_section}SERWERY MODEL CONTEXT PROTOCOL (MCP):
{mcp_report}

TWOJE MOŻLIWOŚCI I NARZĘDZIA:
1. Narzędzia agenta:
   - `read_file(path, start_line?, end_line?)` – odczyt plików,
   - `edit_file(path, target_content, replacement_content)` – precyzyjna podmiana kodu,
   - `write_file(path, content)` – tworzenie nowych plików,
   - `bash_exec(command)` – wykonywanie komend powłoki (Host / WSL / Docker),
   - `grep_search(query)` – szybkie przeszukiwanie bazy kodu.
2. Pamięć (uczenie się między sesjami, Letta-style):
   - `core_memory_append(label, content)` – dopisz wiedzę do bloku (label: persona | human | project),
   - `core_memory_replace(label, old_str, new_str)` – podmień fragment bloku,
   - `create_skill(name, content)` – utwórz learned skill z doświadczenia (zapis do .opencode/skills/learned/<name>/SKILL.md).
   Używaj ich gdy odkryjesz wzorzec/preferencję/wiedzę o projekcie, którą przyszła sesja powinna znać. Generalizuj, nie loguj pojedynczych zdarzeń. Skille tworzy po skończeniu złożonego zadania (np. procedura DB migration w tym projekcie).
2b. Plan projektu (persistentny, per-projekt — `.opencode/plan.md`):
   - `plan_set(goal)` – ustaw główny cel planu (nadpisuje poprzedni),
   - `plan_add_step(description)` – dodaj krok na końcu listy,
   - `plan_complete_step(step_number)` – oznacz krok (1-based) jako ukończony lub cofnij,
   - `plan_update_step(step_number, description)` – zaktualizuj opis kroku,
   - `plan_add_note(note)` – dodaj notatkę/decyzję do planu,
   - `plan_clear()` – wyczyść cały plan.
   Używaj planu gdy zadanie jest złożone (wiele kroków, sesji, modeli). Plan przetrwa zamknięcie UI — każda przyszła sesja (i każdy model) go zobaczy. Aktualizuj plan po ukończeniu kroku.
2c. Archival memory (wektorowa pamięć długoterminowa, HNSW):
   - `archival_search(query, top_k=5)` – wyszukaj podobną wiedzę (vector + keyword),
   - `archival_add(content, labels=[], node_type="fact")` – zapisz długoterminową wiedzę,
   - `archival_list()` – wylistuj wszystkie wpisy.
   Używaj archival memory dla faktów/wzorców które nie pasują do bloków memory (np. "ten projekt używa PostgreSQL 16 z schematem multi-tenant", "auth via Keycloak realm X"). Wpisów nie widać w system prompcie — wyszukuj on-demand.
3. Gdy chcesz użyć narzędzia, wygeneruj blok:
<tool_call>
{{"name": "nazwa_narzędzia", "arguments": {{"parametr": "wartość"}}}}
</tool_call>

ZASADY:
- Pisz kod czysty, bezpieczny i wydajny dopasowany do stosu ({stack}).
- Zawsze dopasowuj się do stylu projektu i istniejącej architektury.
- Zwięzłe wyjaśnienia, kod w blokach markdown ze specyfikacją języka."#,
            active_operator = active_operator,
            mode = mode,
            mode_instructions = mode_instructions,
            work_dir = self.work_dir.display(),
            stack = stack,
            current_branch = current_branch,
            project_rules = project_rules,
            taste_ctx = if taste_ctx.is_empty() { "(brak taste.md — uruchom `npx taste push --all` lub /taste)" } else { &taste_ctx },
            skills_ctx = if skills_ctx.is_empty() { "(brak skilli — dodaj .roo/skills/*/SKILL.md)" } else { &skills_ctx },
            memories_ctx = if memories_ctx.is_empty() { "(brak memories.md)" } else { &memories_ctx },
            memory_blocks = if memory_blocks.is_empty() { "(brak bloków — użyj /remember lub core_memory_append by zacząć się uczyć)" } else { &memory_blocks },
            mcp_report = mcp_report
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_detection_and_tag_resolver() {
        let temp_dir = std::env::temp_dir().join(format!("opencode_ctx_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();

        // 1. Stwórz plik Cargo.toml
        fs::write(temp_dir.join("Cargo.toml"), "[package]\nname=\"test\"").unwrap();
        let cm = ContextManager::new(temp_dir.clone());
        assert_eq!(cm.detect_project_stack(), "Rust (Cargo)");

        // 2. Stwórz plik do testu @tagu
        fs::write(temp_dir.join("test_file.rs"), "fn hello() {}").unwrap();
        let resolved = cm.resolve_smart_context("Sprawdź @test_file.rs i popraw kod");
        assert!(resolved.contains("fn hello() {}"));
        assert!(resolved.contains("[Zawartość pliku `test_file.rs`]"));

        // Posprzątaj
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_taste_and_skills_and_terminal_tags() {
        let dir = std::env::temp_dir().join(format!("opencode_ctx2_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join(".commandcode").join("taste").join("cli")).unwrap();
        fs::write(dir.join(".commandcode").join("taste").join("cli").join("taste.md"), "prefer tabs").unwrap();
        fs::create_dir_all(dir.join(".roo").join("skills").join("s1")).unwrap();
        fs::write(dir.join(".roo").join("skills").join("s1").join("SKILL.md"), "# S1").unwrap();
        let cm = ContextManager::new(dir.clone());
        let taste = cm.load_taste_context();
        assert!(taste.contains("prefer tabs"));
        let skills = cm.load_skills_context();
        assert!(skills.contains("S1"));
        // @terminal tag
        fs::create_dir_all(dir.join(".opencode")).unwrap();
        fs::write(dir.join(".opencode").join("terminal.log"), "hello terminal").unwrap();
        let res = cm.resolve_smart_context("check @terminal");
        assert!(res.contains("hello terminal"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_build_system_prompt_contains_interactive_and_taste() {
        let dir = std::env::temp_dir().join(format!("opencode_ctx3_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let cm = ContextManager::new(dir.clone());
        let p = cm.build_system_prompt("cursor-claude-3-7-sonnet", "interactive");
        assert!(p.contains("INTERACTIVE"));
        assert!(p.contains("TASTE-1"));
        let p2 = cm.build_system_prompt("opencode-acp", "auto");
        assert!(p2.contains("AUTO"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_load_project_rules_and_memories() {
        let dir = std::env::temp_dir().join(format!("opencode_ctx4_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("AGENTS.md"), "rule1").unwrap();
        fs::create_dir_all(dir.join(".windsurf")).unwrap();
        fs::write(dir.join(".windsurf").join("memories.md"), "mem1").unwrap();
        let cm = ContextManager::new(dir.clone());
        assert!(cm.load_project_rules().contains("rule1"));
        assert!(cm.load_memories_context().contains("mem1"));
        fs::remove_dir_all(&dir).ok();
    }
}
