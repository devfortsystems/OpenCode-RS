#![allow(dead_code)]

mod agent;
mod app;
mod auth;
mod config;
mod cost;
mod environment;
mod diff;
mod file_manager;
mod pty;
mod git;
mod hermes;
mod live_grep;
mod lsp;
mod i18n;
mod importer;
mod palette;
mod plugins;
mod providers;
mod runtime;
mod search;
mod session;
mod sync;
mod syntax;
mod templates;
mod theme;
mod ui;
mod update;
mod web;
mod taste;
mod skills;
mod e2e;

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

use app::App;
use auth::AuthManager;
use config::AppConfig;
use importer::OpenCodeMigration;
use providers::{ChatMessage, ProviderRouter};
use session::SessionManager;

#[derive(Parser, Debug)]
#[command(author, version, about = "⚡ OpenCode-RS: Universal Terminal AI Coding Agent in Rust", long_about = None)]
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

    /// Uruchom jako serwer Model Context Protocol (MCP stdio) dla Cursor, Claude Desktop, Antigravity itp.
    #[arg(long)]
    mcp: bool,

    /// Jednorazowy prompt CLI bez uruchamiania pełnego TUI
    #[arg(short = 'p', long)]
    prompt: Option<String>,

    /// Kontynuuj ostatnią sesję w trybie CLI
    #[arg(short = 'c', long)]
    continue_session: bool,

    /// Bezpośredni prompt podany jako argumenty (np. opencode "napisz funkcję...")
    #[arg(trailing_var_arg = true)]
    direct_prompt: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = CliArgs::parse();

    let work_dir = args
        .workdir
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Tryb serwera MCP (stdio JSON-RPC)
    if args.mcp {
        return crate::agent::mcp::McpServerHub::run_stdio(work_dir);
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
    let router = Arc::new(ProviderRouter::new(config.clone()));
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

    let exec_res = agent_clone
        .process_user_prompt(&model_clone, "coder", &history, &prompt, token_tx)
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

async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    let mut event_stream = EventStream::new();

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
        }
    }

    Ok(())
}
