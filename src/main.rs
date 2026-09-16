#![allow(dead_code)]

// Wszystkie moduły upublicznione — lib.rs re-eksportuje je dla testów integracyjnych.
// `#![allow(dead_code)]` eliminuje ostrzeżenia o nieużywanych w binary.
pub mod agent;
pub mod app;
pub mod auth;
pub mod config;
pub mod cost;
pub mod environment;
pub mod diff;
pub mod file_manager;
pub mod pty;
pub mod git;
pub mod hermes;
pub mod live_grep;
pub mod lsp;
pub mod lsp_client;
pub mod i18n;
pub mod importer;
pub mod palette;
pub mod plugins;
pub mod providers;
pub mod runtime;
pub mod search;
pub mod session;
pub mod sync;
pub mod syntax;
pub mod templates;
pub mod theme;
pub mod ui;
pub mod update;
pub mod web;
pub mod taste;
pub mod skills;
pub mod memory;
pub mod e2e;
pub mod acp_server;
pub mod database;
pub mod archival;
pub mod transfer;
pub mod opencode_compat;
pub mod mod_bridge;
pub mod repomap;
pub mod vsix;

use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use app::App;
use auth::AuthManager;
use config::AppConfig;
use importer::OpenCodeMigration;
use providers::{ChatMessage, ProviderRouter};
use session::SessionManager;

// ─── CLI subkomendy (łącząc najlepsze z opencode + devin + commandcode) ───

/// Subkomendy opencode-rs (inspirowane opencode, devin, commandcode).
#[derive(Subcommand, Debug)]
enum Command {
    /// Start TUI (domyślnie) — alias dla `opencode` bez subkomendy
    #[command(hide = true)]
    Tui,

    /// Run z promptem bez TUI (non-interactive, jak `opencode run` / `devin -p`)
    Run {
        /// Prompt do wykonania
        #[arg(trailing_var_arg = true)]
        message: Vec<String>,
        /// Kontynuuj ostatnią sesję
        #[arg(short = 'c', long)]
        continue_session: bool,
    },

    /// Start ACP (Agent Client Protocol) server over stdio (jak `devin acp` / `opencode acp`)
    Acp,

    /// Start MCP (Model Context Protocol) server over stdio (jak `opencode mcp` / `devin mcp`)
    Mcp,

    /// List dostępnych modeli (jak `opencode models` / `devin models list`)
    Models {
        /// Filtruj po providerze (np. cursor, windsurf, antigravity, gemini)
        provider: Option<String>,
    },

    /// Zarządzaj autentykacją / providerami (jak `opencode auth` / `devin auth`)
    Auth {
        /// Subkomenda: status, list, add, remove
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Diagnose środowiska (jak `devin doctor`) — providerzy, API keys, MDBX, git
    Doctor {
        /// Wyjście JSON (machine-readable)
        #[arg(long)]
        json: bool,
    },

    /// Zarządzaj sesjami (jak `opencode session` / `devin list`)
    Session {
        /// Subkomenda: list, show, delete, export
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Statystyki użycia tokenów i kosztów (jak `opencode stats`)
    Stats {
        /// Wyjście JSON
        #[arg(long)]
        json: bool,
    },

    /// Export sesji jako JSON (jak `opencode export`)
    Export {
        /// ID sesji (puste = ostatnia)
        session_id: Option<String>,
    },

    /// Import sesji z JSON (jak `opencode import`)
    Import {
        /// Ścieżka do pliku JSON
        file: String,
    },

    /// Zarządzaj skillami (jak `devin skills`)
    Skills {
        /// Subkomenda: list, show, create, delete
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Zarządzaj regułami / memory blocks (jak `devin rules` / `opencode memory`)
    Rules {
        /// Subkomenda: list, show, set, clear
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Zarządzaj pluginami (jak `opencode plugin` / `devin plugins`)
    Plugins {
        /// Subkomenda: list, install, remove
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Tryb refleksji w tle (sleeptime dreaming)
    Sleeptime {
        /// Interwał w sekundach (puste = jednorazowo)
        interval: Option<u64>,
    },

    /// Sprawdź aktualizacje (jak `opencode upgrade` / `devin update`)
    Upgrade {
        /// Wymuś aktualizację
        #[arg(short = 'f', long)]
        force: bool,
    },

    /// Wersja (jak `devin version`)
    Version,

    /// Shell completion (jak `opencode completion`)
    Completion {
        /// Shell: bash, zsh, fish, powershell
        shell: Option<String>,
    },
}

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "⚡ OpenCode-RS: Universal Terminal AI Coding Agent in Rust",
    long_about = None,
    subcommand_required = false,
)]
struct CliArgs {
    /// Katalog roboczy projektu
    #[arg(short, long)]
    workdir: Option<PathBuf>,

    /// Domyślny model/operator (np. cursor-claude-3-7-sonnet, windsurf-cascade-sonnet, commandcode, gemini-3.7-flash)
    #[arg(short, long)]
    model: Option<String>,

    /// Adres mostka do edytorów
    #[arg(short, long)]
    bridge_url: Option<String>,

    /// Uruchom jako serwer Model Context Protocol (MCP stdio) — alias dla `opencode mcp`
    #[arg(long)]
    mcp: bool,

    /// Jednorazowy prompt CLI bez uruchamiania pełnego TUI (alias dla `opencode run`)
    #[arg(short = 'p', long)]
    prompt: Option<String>,

    /// Kontynuuj ostatnią sesję w trybie CLI
    #[arg(short = 'c', long)]
    continue_session: bool,

    /// Bezpośredni prompt podany jako argumenty (np. opencode "napisz funkcję...")
    #[arg(trailing_var_arg = true)]
    direct_prompt: Vec<String>,

    /// Przetestuj wszystkie 75+ modeli live (ping) i wypisz tabelę OK/FAIL
    #[arg(long)]
    test_all: bool,

    /// Tryb refleksji w tle (sleeptime dreaming) — alias dla `opencode sleeptime`
    #[arg(long)]
    sleeptime: Option<Option<u64>>,

    /// Uruchom jako serwer ACP (Agent Client Protocol) over stdio — alias dla `opencode acp`
    #[arg(long)]
    acp: bool,

    /// Subkomenda (opencode run / acp / mcp / models / auth / doctor / session / stats / ...)
    #[command(subcommand)]
    command: Option<Command>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = CliArgs::parse();

    let work_dir = args
        .workdir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // ─── Subkomendy (nowe) ───────────────────────────────────────────────
    if let Some(cmd) = &args.command {
        match cmd {
            Command::Tui => { /* default TUI flow — kontynuuj poniżej */ }
            _ => return run_subcommand(cmd, &work_dir, &args).await,
        }
    }

    // ─── Flagi (backward compat — stare wywołania nadal działają) ───────

    // Tryb serwera MCP (stdio JSON-RPC) — `opencode --mcp` (alias dla `opencode mcp`)
    if args.mcp {
        return crate::agent::mcp::McpServerHub::run_stdio(work_dir);
    }

    // Tryb serwera ACP — `opencode --acp` (alias dla `opencode acp`)
    if args.acp {
        return crate::acp_server::run_acp_server(work_dir).await;
    }

    // Tryb sleeptime — `opencode --sleeptime` (alias dla `opencode sleeptime`)
    if let Some(interval) = args.sleeptime {
        AuthManager::auto_load_credentials(&work_dir);
        let config = AppConfig::load_for_project(&work_dir);
        return run_sleeptime(work_dir.clone(), config, interval).await;
    }

    // Automatyczne wczytanie danych uwierzytelniających z .env (lokalnych i globalnych)
    AuthManager::auto_load_credentials(&work_dir);

    let mut config = AppConfig::load_for_project(&work_dir);

    // Pełna auto-migracja z oryginalnego OpenCode (auth + provider + sesje) — bez inercji
    OpenCodeMigration::import_credentials(&mut config);
    OpenCodeMigration::import_providers(&mut config, &work_dir);
    OpenCodeMigration::import_sessions(&work_dir);
    if let Some(m) = args.model {
        config.default_model = m;
    }
    if let Some(b) = args.bridge_url {
        config.bridge_url = b;
    }

    // Sprawdź czy przekazano dane przez potok stdin (np. `cat file.rs | opencode "co to robi"`)
    let mut stdin_input = String::new();
    if !io::stdin().is_terminal() {
        let mut buffer = Vec::new();
        if io::stdin().read_to_end(&mut buffer).is_ok() {
            if let Ok(text) = String::from_utf8(buffer) {
                stdin_input = text.trim().to_string();
            }
        }
    }

    // Zbuduj połączony prompt CLI jeśli został przekazany
    let mut cli_prompt_parts = Vec::new();
    if let Some(p) = args.prompt {
        cli_prompt_parts.push(p);
    }
    if !args.direct_prompt.is_empty() {
        cli_prompt_parts.push(args.direct_prompt.join(" "));
    }

    let cli_prompt = if !cli_prompt_parts.is_empty() || !stdin_input.is_empty() {
        let text_prompt = cli_prompt_parts.join(" ");
        if !stdin_input.is_empty() {
            if !text_prompt.is_empty() {
                Some(format!("{}\n\nKontekst (stdin):\n```\n{}\n```", text_prompt, stdin_input))
            } else {
                Some(format!("Przeanalizuj poniższy kod/dane ze strumienia wejściowego:\n```\n{}\n```", stdin_input))
            }
        } else {
            Some(text_prompt)
        }
    } else {
        None
    };

    if args.test_all {
        return run_test_all(work_dir, config).await;
    }

    // ── TRYB SLEEPTIME (refleksja w tle nad pamięcią) ────────────────────
    if let Some(interval) = args.sleeptime {
        return run_sleeptime(work_dir, config, interval).await;
    }

    // ── TRYB LINII KOMEND (CLI / ONE-SHOT EXECUTION) ────────────────────────
    if let Some(prompt) = cli_prompt {
        return run_cli_mode(work_dir, config, prompt, args.continue_session).await;
    }

    // ── INTERAKTYWNY TRYB TUI ───────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(work_dir, config);
    let res = run_app(&mut terminal, &mut app).await;

    // Przywrócenie standardowego trybu terminala
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Błąd OpenCode-RS: {:?}", err);
    }

    Ok(())
}

/// Jednorazowe wykonanie w linii komend (CLI stream do stdout jak w oryginalnym OpenCode)
async fn run_cli_mode(
    work_dir: PathBuf,
    config: AppConfig,
    prompt: String,
    continue_session: bool,
) -> Result<()> {
    let router = Arc::new(ProviderRouter::new(config.clone(), work_dir.clone()));
    let agent = Arc::new(crate::agent::Agent::new(router.clone(), work_dir.clone()));
    let session_manager = SessionManager::new(work_dir.clone(), &config.storage_mode);
    let active_model = config.default_model.clone();

    let mut session = if continue_session {
        session_manager
            .get_latest_session()
            .unwrap_or_else(|| session_manager.create_session(&active_model))
    } else {
        session_manager.create_session(&active_model)
    };

    let (token_tx, mut token_rx) = tokio::sync::mpsc::channel::<String>(100);

    // Wypisuj tokeny na żywo do stdout
    let printer_handle = tokio::spawn(async move {
        let mut stdout = io::stdout();
        let mut accumulated = String::new();
        while let Some(tok) = token_rx.recv().await {
            accumulated.push_str(&tok);
            print!("{}", tok);
            let _ = stdout.flush();
        }
        println!();
        accumulated
    });

    let agent_clone = agent.clone();
    let history = session.messages.clone();
    let model_clone = active_model.clone();

    let (ctx_tx, _ctx_rx) = tokio::sync::mpsc::channel::<(usize, usize)>(10);
    let exec_res = agent_clone
        .process_user_prompt(&model_clone, "coder", &history, &prompt, token_tx, ctx_tx)
        .await;

    let full_reply = printer_handle.await.unwrap_or_default();

    match exec_res {
        Ok(used_model) => {
            session.messages.push(ChatMessage {
                role: "user".to_string(),
                content: prompt,
            });
            session.messages.push(ChatMessage {
                role: "assistant".to_string(),
                content: full_reply,
            });
            session.model = used_model;
            session_manager.save_session(&session).ok();
        }
        Err(e) => {
            eprintln!("\n❌ Błąd wykonania CLI: {e}");
            return Err(e);
        }
    }

    Ok(())
}

async fn run_test_all(work_dir: PathBuf, mut config: AppConfig) -> Result<()> {
    use tokio::sync::mpsc;
    // Załaduj klucze tak jak TUI (auth.json + .env + OpenCode import)
    crate::auth::AuthManager::auto_load_credentials(&work_dir);
    crate::importer::OpenCodeMigration::import_credentials(&mut config);
    let has_gemini = config.direct_gemini_api_key.is_some();
    let has_openai = config.direct_openai_api_key.is_some();
    let has_groq = config.direct_groq_api_key.is_some();
    let has_deepseek = config.direct_deepseek_api_key.is_some();
    let has_openrouter = config.direct_openrouter_api_key.is_some();
    println!("🔍 Test wszystkich modeli — klucze: gemini={} openai={} groq={} deepseek={} openrouter={} bridge={}", has_gemini, has_openai, has_groq, has_deepseek, has_openrouter, config.bridge_url);
    // Szybki check mostka — próbuj 8765,8766,8767 (multi-edytory Trae+Antigravity+Devin)
    let base = config.bridge_url.trim_end_matches("/v1").trim_end_matches("/").to_string();
    let mut bridge_ok = false;
    let mut bridge_port = String::new();
    for p in ["8765","8766","8767"] {
        let url = base.replace("8765", p);
        let health = format!("{}/health", url.trim_end_matches('/'));
        if reqwest::Client::new().get(&health).timeout(std::time::Duration::from_millis(500)).send().await.is_ok() {
            bridge_ok = true; bridge_port = p.to_string(); break;
        }
    }
    println!("   Bridge 8765-8767: {}", if bridge_ok { format!("✅ reachable :{} ({})", bridge_port, base.replace("8765", &bridge_port)) } else { "⚪ offline (Bridge models będą FAIL — uruchom wtyczkę Cursor/Antigravity/Trae, sprawdź curl http://127.0.0.1:8765/health)".to_string() });
    let router = ProviderRouter::new(config, std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")));
    let models = router.get_available_models();
    println!("{:<35} {:<15} {:<10} {}", "MODEL", "PROVIDER", "STATUS", "DETAIL");
    println!("{}", "-".repeat(110));
    for (id, name, prov) in models {
        // Pre-check: jeśli direct bez klucza to od razu NO_KEY, nie uderzaj w API
        let no_key = match prov {
            "gemini" if !has_gemini => true,
            "openai" if !has_openai => true,
            "groq" if !has_groq => true,
            "deepseek" if !has_deepseek => true,
            "openrouter" if !has_openrouter => true,
            _ if ["opencode","antigravity","trae","cursor","windsurf","copilot","amazon-q","augment"].contains(&prov) && !bridge_ok => true,
            _ => false,
        };
        if no_key {
            let reason = if ["opencode","antigravity","trae","cursor","windsurf","copilot","amazon-q","augment"].contains(&prov) { "bridge offline" } else { "brak klucza w .env/auth.json" };
            println!("{:<35} {:<15} {:<10} {}", id, prov, "⚪ NO_KEY", reason);
            continue;
        }
        let (tx, mut rx) = mpsc::channel::<String>(10);
        let msgs = vec![ChatMessage { role: "user".to_string(), content: "ping".to_string() }];
        let fut = router.stream_with_failover(id, &msgs, tx);
        let res = tokio::time::timeout(std::time::Duration::from_secs(5), fut).await;
        match res {
            Ok(Ok(used)) => {
                let _ = tokio::time::timeout(std::time::Duration::from_millis(300), async { while rx.recv().await.is_some() {} }).await;
                println!("{:<35} {:<15} {:<10} {}", id, prov, "✅ OK", format!("used {}", used));
            },
            Ok(Err(e)) => {
                let msg = e.to_string().replace('\n', " ");
                let short = msg.chars().take(100).collect::<String>();
                let status = if short.contains("401") || short.contains("403") || short.contains("No key") || short.contains("klucza") { "⚪ NO_KEY" } else if short.contains("mostkiem") || short.contains("bridge") { "⚪ BRIDGE" } else { "❌ FAIL" };
                println!("{:<35} {:<15} {:<10} {}", id, prov, status, short);
            },
            Err(_) => println!("{:<35} {:<15} {:<10} {}", id, prov, "⏱ TIMEOUT", "5s"),
        }
        let _ = name;
    }
    Ok(())
}

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    let mut event_stream = EventStream::new();
    // Timer do sprawdzania timeoutu streamingu (co 10s)
    let mut timeout_ticker = tokio::time::interval(tokio::time::Duration::from_secs(10));

    loop {
        terminal.draw(|f| ui::render(f, app))?;

        tokio::select! {
            // Zdarzenia z klawiatury / myszy
            maybe_event = event_stream.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) => {
                        if app.handle_key_event(key).await? {
                            break; // Wyjście z aplikacji (Ctrl+C)
                        }
                    }
                    Some(Ok(Event::Mouse(mouse))) => {
                        app.handle_mouse_event(mouse);
                    }
                    _ => {}
                }
            }

            // Zdarzenia wewnętrzne (Tokeny streamingu, błędy, zakończenie zadania)
            Some(app_event) = app.event_rx.recv() => {
                app.handle_app_event(app_event);
            }

            // Timer sprawdzający czy streaming się nie zawiesił (auto-recovery)
            _ = timeout_ticker.tick() => {
                app.check_streaming_timeout();
            }
        }
    }

    Ok(())
}

/// Tryb refleksji w tle (sleeptime dreaming).
///
/// Agent przegląda swoje memory blocks (persona, human, project) oraz plan projektu,
/// konsoliduje wiedzę, wykrywa wzorce, i sugeruje aktualizacje.
///
/// Tryb jednorazowy: `opencode --sleeptime`
/// Tryb cykliczny:   `opencode --sleeptime 300` (co 5 minut)
///
/// Refleksja używa domyślnego modelu z config. Wynik jest wypisywany na stdout
/// i zapisywany do .opencode/sleeptime_log.md.
async fn run_sleeptime(
    work_dir: PathBuf,
    config: AppConfig,
    interval: Option<u64>,
) -> Result<()> {
    use crate::memory::MemoryBlocks;

    let router = Arc::new(ProviderRouter::new(config.clone(), work_dir.clone()));
    let memory = MemoryBlocks::new(work_dir.clone());
    let active_model = config.default_model.clone();

    // Plik logu refleksji
    let sleeptime_log = work_dir.join(".opencode").join("sleeptime_log.md");
    std::fs::create_dir_all(sleeptime_log.parent().unwrap())?;

    match interval {
        None => {
            // Tryb jednorazowy
            run_sleeptime_cycle(&router, &memory, &work_dir, &active_model, &sleeptime_log, 1).await;
        }
        Some(secs) => {
            // Tryb cykliczny
            println!("🌙 Tryb cykliczny: refleksja co {}s (Ctrl+C by zatrzymać)", secs);
            let mut cycle_num = 0;
            loop {
                cycle_num += 1;
                run_sleeptime_cycle(&router, &memory, &work_dir, &active_model, &sleeptime_log, cycle_num).await;
                println!("\n⏳ Następna refleksja za {}s...\n", secs);
                tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;
            }
        }
    }

    Ok(())
}

/// Pojedynczy cykl refleksji sleeptime — przegląda pamięć i generuje sugestie.
async fn run_sleeptime_cycle(
    router: &Arc<ProviderRouter>,
    memory: &crate::memory::MemoryBlocks,
    work_dir: &std::path::Path,
    active_model: &str,
    sleeptime_log: &std::path::Path,
    cycle_num: usize,
) {
    println!("🌙 Sleeptime cycle #{} — refleksja nad pamięcią...", cycle_num);
    println!("   Model: {}", active_model);
    println!("   Katalog: {}", work_dir.display());

    // Wczytaj aktualny stan pamięci
    let blocks = memory.load_all();
    let memory_text = memory.inject_into_prompt();

    // Wczytaj plan projektu
    let plan = crate::memory::ProjectPlan::load(work_dir);
    let plan_text = if !plan.goal.is_empty() || !plan.steps.is_empty() {
        format!(
            "\n📋 Plan projektu:\n Cel: {}\n Kroki ({}):\n{}\n",
            plan.goal,
            plan.steps.len(),
            plan.steps
                .iter()
                .enumerate()
                .map(|(i, s)| format!("  {}. [{}] {}", i + 1, if s.completed { "x" } else { " " }, s.description))
                .collect::<Vec<_>>()
                .join("\n")
        )
    } else {
        String::new()
    };

    // Zbuduj prompt refleksji
    let reflection_prompt = format!(
        "Jesteś w trybie SLEEPTIME — refleksji w tle nad swoją pamięcią.\n\
         Przeglądasz swoje memory blocks i plan projektu, konsolidujesz wiedzę,\n\
         wykrywasz wzorce i sugerujesz aktualizacje.\n\n\
         ## Aktualny stan pamięci:\n\
         {memory_text}\n\
         {plan_text}\n\n\
         ## Twoje zadanie:\n\
         1. Przeanalizuj każdy blok pamięci (persona, human, project).\n\
         2. Zidentyfikuj wzorce, duplikaty, sprzeczności, luki w wiedzy.\n\
         3. Zaproponuj konkretne aktualizacje (co dodać, usunąć, zmienić).\n\
         4. Jeśli plan projektu ma ukończone kroki, zasugeruj aktualizację.\n\
         5. Wypisz sugestie w formacie:\n\n\
         ### Sugestie aktualizacji pamięci:\n\
         - [block:persona] sugestia...\n\
         - [block:human] sugestia...\n\
         - [block:project] sugestia...\n\
         - [plan] sugestia...\n\n\
         Bądź zwięzły i konkretny. To refleksja, nie rozmowa."
    );

    // Wyślij do modelu
    let (token_tx, mut token_rx) = tokio::sync::mpsc::channel::<String>(100);

    let router_clone = Arc::clone(router);
    let model_clone = active_model.to_string();
    let prompt_clone = reflection_prompt.clone();

    let exec_handle = tokio::spawn(async move {
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: prompt_clone,
        }];
        router_clone
            .stream_with_failover(&model_clone, &messages, token_tx)
            .await
    });

    // Zbierz odpowiedź
    let mut reflection = String::new();
    while let Some(tok) = token_rx.recv().await {
        reflection.push_str(&tok);
        print!("{}", tok);
        let _ = io::stdout().flush();
    }
    println!();

    // Czekaj na zakończenie exec
    match exec_handle.await {
        Ok(Ok(_used_model)) => {
            // Zapisz refleksję do logu
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let log_entry = format!(
                "## Sleeptime cycle #{cycle_num} — {timestamp}\n\n\
                 Model: {active_model}\n\n\
                 {reflection}\n\n---\n\n"
            );

            let mut existing_log = std::fs::read_to_string(sleeptime_log).unwrap_or_default();
            existing_log = format!("# Sleeptime Dream Log\n\n{log_entry}{existing_log}");
            if let Err(e) = std::fs::write(sleeptime_log, &existing_log) {
                eprintln!("❌ Błąd zapisu logu: {e}");
            }

            println!(
                "✅ Refleksja zapisana do {}",
                sleeptime_log.display()
            );

            // Statystyki bloków
            let non_empty = blocks.iter().filter(|(_, c)| !c.is_empty()).count();
            let total_chars: usize = blocks.iter().map(|(_, c)| c.len()).sum();
            println!(
                "📊 Memory: {non_empty}/{} bloków niepustych, {total_chars} znaków łącznie",
                crate::memory::BLOCK_LABELS.len()
            );
        }
        Ok(Err(e)) => {
            eprintln!("❌ Błąd refleksji: {e}");
        }
        Err(e) => {
            eprintln!("❌ Błąd task: {e}");
        }
    }
}

// ─── Subkomendy — implementacje ──────────────────────────────────────

/// Dispatcher subkomend.
async fn run_subcommand(cmd: &Command, work_dir: &Path, args: &CliArgs) -> Result<()> {
    match cmd {
        Command::Tui => {
            // Default — TUI flow kontynuuje w main() (gdy command == Tui, nie return)
            // To nie powinno być osiągnięte bo Tui jest obsługiwane w main
            run_main_tui_flow(args, work_dir).await
        }

        Command::Run { message, continue_session } => {
            let prompt = message.join(" ");
            if prompt.is_empty() {
                eprintln!("Użycie: opencode run <prompt>");
                eprintln!("  opencode run \"napisz funkcję sort\"");
                return Ok(());
            }
            AuthManager::auto_load_credentials(work_dir);
            let mut config = AppConfig::load_for_project(work_dir);
            OpenCodeMigration::import_credentials(&mut config);
            OpenCodeMigration::import_providers(&mut config, work_dir);
            if let Some(m) = &args.model {
                config.default_model = m.clone();
            }
            run_cli_mode(work_dir.to_path_buf(), config, prompt, *continue_session || args.continue_session).await
        }

        Command::Acp => {
            crate::acp_server::run_acp_server(work_dir.to_path_buf()).await
        }

        Command::Mcp => {
            crate::agent::mcp::McpServerHub::run_stdio(work_dir.to_path_buf())
        }

        Command::Models { provider } => {
            run_models_list(work_dir, provider.as_deref()).await
        }

        Command::Auth { args: sub_args } => {
            run_auth_cmd(work_dir, sub_args)
        }

        Command::Doctor { json } => {
            run_doctor(work_dir, *json)
        }

        Command::Session { args: sub_args } => {
            run_session_cmd(work_dir, sub_args)
        }

        Command::Stats { json } => {
            run_stats(work_dir, *json)
        }

        Command::Export { session_id } => {
            run_export(work_dir, session_id.as_deref())
        }

        Command::Import { file } => {
            run_import(work_dir, file)
        }

        Command::Skills { args: sub_args } => {
            run_skills_cmd(work_dir, sub_args)
        }

        Command::Rules { args: sub_args } => {
            run_rules_cmd(work_dir, sub_args)
        }

        Command::Plugins { args: sub_args } => {
            run_plugins_cmd(work_dir, sub_args)
        }

        Command::Sleeptime { interval } => {
            run_sleeptime_wrapper(work_dir, *interval).await
        }

        Command::Upgrade { force } => {
            run_upgrade(*force).await
        }

        Command::Version => {
            let version = env!("CARGO_PKG_VERSION");
            println!("opencode-rs v{}", version);
            println!("OpenCode {} compatible", version);
            Ok(())
        }

        Command::Completion { shell } => {
            run_completion(shell.as_deref())
        }
    }
}

/// `opencode sleeptime` — wrapper który tworzy config
async fn run_sleeptime_wrapper(work_dir: &Path, interval: Option<u64>) -> Result<()> {
    AuthManager::auto_load_credentials(work_dir);
    let config = AppConfig::load_for_project(work_dir);
    run_sleeptime(work_dir.to_path_buf(), config, interval).await
}

/// `opencode models [provider]` — list dostępnych modeli
async fn run_models_list(work_dir: &Path, provider_filter: Option<&str>) -> Result<()> {
    AuthManager::auto_load_credentials(work_dir);
    let config = AppConfig::load_for_project(work_dir);
    let router = Arc::new(ProviderRouter::new(config, work_dir.to_path_buf()));

    // Najpierw statyczne modele, potem dynamiczne (Antigravity, opencode, kilo...)
    let mut models: Vec<(String, String, String)> = router
        .get_available_models()
        .into_iter()
        .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
        .collect();

    let dynamic = router.discover_models().await;
    let seen: std::collections::HashSet<String> = models.iter().map(|(id, _, _)| id.clone()).collect();
    for (id, name, prov) in dynamic {
        if !seen.contains(&id) {
            models.push((id, name, prov));
        }
    }

    let filtered: Vec<_> = match provider_filter {
        Some(f) => {
            let fl = f.to_lowercase();
            models.into_iter().filter(|(name, provider, _)| {
                name.to_lowercase().contains(&fl) || provider.to_lowercase().contains(&fl)
            }).collect()
        }
        None => models,
    };

    if filtered.is_empty() {
        println!("Brak modeli{}.", provider_filter.map(|f| format!(" dla '{f}'")).unwrap_or_default());
        return Ok(());
    }

    println!("Dostępne modele ({}):", filtered.len());
    for (name, provider, _) in &filtered {
        println!("  • {name} [{provider}]");
    }
    Ok(())
}

/// `opencode auth [status|list|add|remove]`
fn run_auth_cmd(work_dir: &Path, sub_args: &[String]) -> Result<()> {
    AuthManager::auto_load_credentials(work_dir);
    let config = AppConfig::load_for_project(work_dir);
    let action = sub_args.first().map(|s| s.as_str()).unwrap_or("status");

    match action {
        "status" => {
            let report = AuthManager::get_auth_status_report(&config, work_dir);
            println!("{report}");
        }
        "list" => {
            println!("Skonfigurowani providerzy (klucze API):");
            let keys = [
                ("openai", &config.direct_openai_api_key),
                ("anthropic", &config.direct_anthropic_api_key),
                ("gemini", &config.direct_gemini_api_key),
                ("groq", &config.direct_groq_api_key),
                ("deepseek", &config.direct_deepseek_api_key),
                ("mistral", &config.direct_mistral_api_key),
                ("openrouter", &config.direct_openrouter_api_key),
                ("commandcode", &config.commandcode_api_key),
                ("devin", &config.devin_api_key),
            ];
            for (name, key) in &keys {
                let status = if key.as_ref().map(|k| !k.is_empty()).unwrap_or(false) { "✅" } else { "❌ brak key" };
                println!("  {status} {name}");
            }
            if !config.custom_endpoints.is_empty() {
                println!("  ✅ {} custom endpoints", config.custom_endpoints.len());
            }
        }
        "add" => {
            println!("Dodawanie providera przez CLI — użyj TUI: opencode → Ctrl+M");
            println!("Albo edytuj .opencode/config.json ręcznie.");
        }
        "remove" => {
            if sub_args.len() < 2 {
                eprintln!("Użycie: opencode auth remove <name>");
                return Ok(());
            }
            println!("Usunięcie providera '{}' — edytuj .opencode/config.json", sub_args[1]);
        }
        _ => {
            println!("Użycie: opencode auth [status|list|add|remove]");
        }
    }
    Ok(())
}

/// `opencode doctor [--json]` — diagnose środowiska
fn run_doctor(work_dir: &Path, json: bool) -> Result<()> {
    AuthManager::auto_load_credentials(work_dir);
    let config = AppConfig::load_for_project(work_dir);

    let mut checks: Vec<(String, String, bool)> = Vec::new();

    // 1. Git
    let git_ok = std::process::Command::new("git").arg("--version").output().is_ok();
    checks.push(("git".into(), if git_ok { "✅ zainstalowany" } else { "❌ brak" }.into(), git_ok));

    // 2. Rust
    let rust_ok = std::process::Command::new("rustc").arg("--version").output().is_ok();
    checks.push(("rustc".into(), if rust_ok { "✅ zainstalowany" } else { "❌ brak" }.into(), rust_ok));

    // 3. Providerzy — sprawdź czy jakikolwiek API key jest skonfigurowany
    let api_keys = [
        &config.direct_openai_api_key,
        &config.direct_anthropic_api_key,
        &config.direct_gemini_api_key,
        &config.direct_groq_api_key,
        &config.direct_deepseek_api_key,
        &config.direct_mistral_api_key,
        &config.direct_openrouter_api_key,
        &config.commandcode_api_key,
        &config.devin_api_key,
    ];
    let keys_count = api_keys.iter().filter(|k| k.as_ref().map(|v| !v.is_empty()).unwrap_or(false)).count();
    let providers_ok = keys_count > 0 || !config.custom_endpoints.is_empty();
    checks.push(("providers".into(), format!("{keys_count} kluczy API + {} custom endpoints", config.custom_endpoints.len()), providers_ok));

    // 4. Antigravity
    let antigravity_path = r"C:\Users\Daniel\AppData\Local\Programs\antigravity\resources\bin\language_server.exe";
    let ag_ok = Path::new(antigravity_path).exists();
    checks.push(("antigravity".into(), if ag_ok { "✅ language_server.exe znaleziony" } else { "⚠️ nie znaleziony" }.into(), ag_ok));

    // 5. DevFortDB
    let db_path = work_dir.join(".opencode").join("db").join("opencode.mdb");
    let db_ok = db_path.exists();
    let db_status = if db_ok { format!("✅ {}", db_path.display()) } else { "ℹ️ nie zainicjowana (utworzy się przy pierwszym użyciu)".to_string() };
    checks.push(("devfortdb".into(), db_status, true));

    // 6. Memory blocks
    let memory = crate::memory::MemoryBlocks::new(work_dir.to_path_buf());
    let blocks = memory.load_all();
    let non_empty = blocks.iter().filter(|(_, c)| !c.is_empty()).count();
    checks.push(("memory".into(), format!("{}/{} bloków niepustych", non_empty, crate::memory::BLOCK_LABELS.len()), true));

    // 7. Work dir
    checks.push(("workdir".into(), work_dir.display().to_string(), true));

    if json {
        let json_checks: Vec<serde_json::Value> = checks.iter().map(|(name, status, ok)| {
            serde_json::json!({"name": name, "status": status, "ok": ok})
        }).collect();
        println!("{}", serde_json::to_string_pretty(&json_checks)?);
    } else {
        println!("🔍 opencode-rs doctor — diagnostyka środowiska\n");
        for (name, status, _) in &checks {
            println!("  {name:15} {status}");
        }
        let all_ok = checks.iter().filter(|(_, _, ok)| !ok).count();
        if all_ok == 0 {
            println!("\n✅ Wszystko OK!");
        } else {
            println!("\n⚠️ {} problemów wykrytych", all_ok);
        }
    }
    Ok(())
}

/// `opencode session [list|show|delete|export]`
fn run_session_cmd(work_dir: &Path, sub_args: &[String]) -> Result<()> {
    let config = AppConfig::load_for_project(work_dir);
    let sm = SessionManager::new(work_dir.to_path_buf(), &config.storage_mode);
    let action = sub_args.first().map(|s| s.as_str()).unwrap_or("list");

    match action {
        "list" | "ls" => {
            let sessions = sm.list_sessions()?;
            if sessions.is_empty() {
                println!("Brak sesji.");
                return Ok(());
            }
            println!("Sesje ({}):", sessions.len());
            for s in &sessions {
                println!("  {} | {} | {} msgs | {}", s.id, s.title, s.message_count, s.updated_at);
            }
        }
        "show" => {
            if sub_args.len() < 2 {
                eprintln!("Użycie: opencode session show <id>");
                return Ok(());
            }
            let sess = sm.load_session(&sub_args[1])?;
            println!("Sesja: {} ({})", sess.title, sess.id);
            println!("Model: {}", sess.model);
            println!("Utworzona: {}", sess.created_at);
            println!("Wiadomości: {}", sess.messages.len());
            for m in &sess.messages {
                println!("\n[{}]", m.role);
                println!("  {}", m.content.chars().take(200).collect::<String>());
            }
        }
        "delete" | "rm" => {
            if sub_args.len() < 2 {
                eprintln!("Użycie: opencode session delete <id>");
                return Ok(());
            }
            sm.delete_session(&sub_args[1])?;
            println!("✅ Sesja {} usunięta", sub_args[1]);
        }
        "export" => {
            if sub_args.len() < 2 {
                eprintln!("Użycie: opencode session export <id>");
                return Ok(());
            }
            let sess = sm.load_session(&sub_args[1])?;
            let json = serde_json::to_string_pretty(&sess)?;
            println!("{json}");
        }
        _ => {
            println!("Użycie: opencode session [list|show <id>|delete <id>|export <id>]");
        }
    }
    Ok(())
}

/// `opencode stats [--json]`
fn run_stats(work_dir: &Path, json: bool) -> Result<()> {
    let config = AppConfig::load_for_project(work_dir);
    let sm = SessionManager::new(work_dir.to_path_buf(), &config.storage_mode);
    let sessions = sm.list_sessions()?;

    let total_sessions = sessions.len();
    let total_messages: usize = sessions.iter().map(|s| s.message_count).sum();

    if json {
        let stats = serde_json::json!({
            "sessions": total_sessions,
            "messages": total_messages,
        });
        println!("{}", serde_json::to_string_pretty(&stats)?);
    } else {
        println!("📊 Statystyki opencode-rs\n");
        println!("  Sesje: {total_sessions}");
        println!("  Wiadomości: {total_messages}");
    }
    Ok(())
}

/// `opencode export [session_id]`
fn run_export(work_dir: &Path, session_id: Option<&str>) -> Result<()> {
    let config = AppConfig::load_for_project(work_dir);
    let sm = SessionManager::new(work_dir.to_path_buf(), &config.storage_mode);

    let id = match session_id {
        Some(id) => id.to_string(),
        None => {
            let sessions = sm.list_sessions()?;
            if sessions.is_empty() {
                eprintln!("Brak sesji do eksportu.");
                return Ok(());
            }
            sessions[0].id.clone()
        }
    };

    let sess = sm.load_session(&id)?;
    let json = serde_json::to_string_pretty(&sess)?;
    println!("{json}");
    Ok(())
}

/// `opencode import <file>`
fn run_import(_work_dir: &Path, file: &str) -> Result<()> {
    let content = std::fs::read_to_string(file)?;
    let _sess: serde_json::Value = serde_json::from_str(&content)?;
    println!("✅ Import sesji z {file} — format JSON rozpoznany");
    println!("Pełny import: użyj TUI lub `opencode` w katalogu docelowym");
    Ok(())
}

/// `opencode skills [list|show|create|delete]`
fn run_skills_cmd(work_dir: &Path, sub_args: &[String]) -> Result<()> {
    let action = sub_args.first().map(|s| s.as_str()).unwrap_or("list");
    let sm = crate::skills::SkillsManager::new(work_dir.to_path_buf());
    let skills = sm.list_skills();

    match action {
        "list" => {
            println!("Skille ({}):", skills.len());
            for s in &skills {
                println!("  • {} ({})", s.name, s.source);
            }
        }
        "show" => {
            if sub_args.len() < 2 {
                eprintln!("Użycie: opencode skills show <name>");
                return Ok(());
            }
            if let Some(s) = skills.iter().find(|s| s.name == sub_args[1]) {
                println!("Skill: {} ({})", s.name, s.source);
                println!("---");
                println!("{}", s.preview);
            } else {
                eprintln!("Skill '{}' nie znaleziony", sub_args[1]);
            }
        }
        _ => {
            println!("Użycie: opencode skills [list|show <name>]");
        }
    }
    Ok(())
}

/// `opencode rules [list|show|set|clear]` — memory blocks
fn run_rules_cmd(work_dir: &Path, sub_args: &[String]) -> Result<()> {
    let memory = crate::memory::MemoryBlocks::new(work_dir.to_path_buf());
    let action = sub_args.first().map(|s| s.as_str()).unwrap_or("list");

    match action {
        "list" => {
            println!("Memory blocks:");
            for label in crate::memory::BLOCK_LABELS {
                let content = memory.load_block(label);
                let scope = match crate::memory::BlockScope::for_label(label) {
                    crate::memory::BlockScope::Global => "global",
                    crate::memory::BlockScope::Project => "project",
                };
                if content.is_empty() {
                    println!("  • {label} [{scope}] — pusty");
                } else {
                    println!("  • {label} [{scope}] — {} znaków", content.len());
                }
            }
        }
        "show" => {
            if sub_args.len() < 2 {
                eprintln!("Użycie: opencode rules show <label>");
                return Ok(());
            }
            let content = memory.load_block(&sub_args[1]);
            println!("{content}");
        }
        "clear" => {
            if sub_args.len() < 2 {
                eprintln!("Użycie: opencode rules clear <label>");
                return Ok(());
            }
            memory.save_block(&sub_args[1], "")?;
            println!("✅ Blok '{}' wyczyszczony", sub_args[1]);
        }
        _ => {
            println!("Użycie: opencode rules [list|show <label>|clear <label>]");
        }
    }
    Ok(())
}

/// `opencode plugins [list|install|remove]`
fn run_plugins_cmd(_work_dir: &Path, sub_args: &[String]) -> Result<()> {
    let action = sub_args.first().map(|s| s.as_str()).unwrap_or("list");

    match action {
        "list" => {
            println!("Pluginy: użyj TUI (Ctrl+P → plugins) do zarządzania");
        }
        _ => {
            println!("Użycie: opencode plugins [list]");
        }
    }
    Ok(())
}

/// `opencode upgrade [--force]`
async fn run_upgrade(force: bool) -> Result<()> {
    println!("Sprawdzanie aktualizacji...");
    if force {
        println!("Wymuszanie aktualizacji...");
    }
    match crate::update::Updater::check_for_update().await {
        Ok(Some((version, _url))) => {
            println!("Dostępna nowa wersja: {version}");
            println!("Pobierz z: https://github.com/devfortsystems/OpenCode-RS/releases");
        }
        Ok(None) => {
            println!("✅ Masz najnowszą wersję ({})", env!("CARGO_PKG_VERSION"));
        }
        Err(e) => {
            eprintln!("⚠️ Nie można sprawdzić aktualizacji: {e}");
        }
    }
    Ok(())
}

/// `opencode completion [bash|zsh|fish|powershell]`
fn run_completion(shell: Option<&str>) -> Result<()> {
    let shell = shell.unwrap_or("bash");
    // Generuj completion via clap
    use clap::CommandFactory;
    let mut cmd = CliArgs::command();
    let shell_enum = match shell {
        "bash" => clap_complete::Shell::Bash,
        "zsh" => clap_complete::Shell::Zsh,
        "fish" => clap_complete::Shell::Fish,
        "powershell" | "pwsh" => clap_complete::Shell::PowerShell,
        _ => {
            eprintln!("Nieobsługiwany shell: {shell}. Dostępne: bash, zsh, fish, powershell");
            return Ok(());
        }
    };
    clap_complete::generate(shell_enum, &mut cmd, "opencode", &mut std::io::stdout());
    Ok(())
}

/// `opencode run <prompt>` — CLI prompt bez TUI (deleguje do run_cli_mode)
async fn run_cli_prompt(_prompt: &str, _work_dir: &Path, _continue_session: bool, _args: &CliArgs) -> Result<()> {
    // Deprecated — używaj run_cli_mode bezpośrednio z dispatcher
    Ok(())
}

/// `opencode tui` — default TUI flow (nie powinno być wywołane, Tui jest obsługiwane w main)
async fn run_main_tui_flow(_args: &CliArgs, _work_dir: &Path) -> Result<()> {
    // Tui subkomenda jest hide=true i obsługiwana w main() przez kontynuację flow
    Ok(())
}
