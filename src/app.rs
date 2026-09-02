use anyhow::Result;
use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::agent::mcp::McpManager;
use crate::agent::Agent;
use crate::config::AppConfig;
use crate::cost::CostEstimator;
use crate::environment::EnvironmentManager;
use crate::file_manager::FileManagerState;
use crate::git::GitAssistant;
use crate::palette::{CommandPalette, PaletteItem};
use crate::providers::{ChatMessage, ProviderRouter};
use crate::runtime::{RuntimeEngine, RuntimeTarget};
use crate::search::WebSearch;
use crate::session::{ChatSession, SessionManager, SessionMetadata};
use crate::sync::SyncEngine;
use crate::templates::TemplateManager;
use crate::theme::AppTheme;

pub enum AppEvent {
    Token(String),
    StreamFinished(String),
    StreamError(String),
    StatusNotification(String),
    ModelsDiscovered(Vec<(String, String, String)>),
}

pub struct App {
    pub config: AppConfig,
    pub work_dir: PathBuf,
    pub router: Arc<ProviderRouter>,
    pub agent: Arc<Agent>,
    pub session_manager: Arc<SessionManager>,
    pub env_manager: EnvironmentManager,
    pub runtime_target: RuntimeTarget,
    pub current_theme: AppTheme,
    pub current_lang: crate::i18n::Language,
    pub current_session: ChatSession,
    pub messages: Vec<ChatMessage>,
    pub input_text: String,
    pub prompt_history: Vec<String>,
    pub history_index: Option<usize>,

    pub active_model: String,
    pub agent_mode: String, // "coder", "architect", "ask"
    pub auto_check: bool,
    pub is_streaming: bool,
    pub streaming_buffer: String,
    pub scroll_offset: u16,

    pub show_model_picker: bool,
    pub model_picker_index: usize,
    pub model_filter_index: usize,
    pub available_models: Vec<(&'static str, &'static str, &'static str)>,
    pub dynamic_models: Vec<(String, String, String)>,

    pub show_session_picker: bool,
    pub session_picker_index: usize,
    pub available_sessions: Vec<SessionMetadata>,

    pub show_file_manager: bool,
    pub file_manager: FileManagerState,

    pub show_command_palette: bool,
    pub palette_index: usize,
    pub palette_query: String,
    pub filtered_palette: Vec<PaletteItem>,

    pub show_theme_picker: bool,
    pub theme_picker_index: usize,

    pub show_sidebar: bool,
    pub spinner_frame: usize,
    pub checkpoint_manager: crate::agent::checkpoint::CheckpointManager,
    pub subagent_manager: Arc<crate::agent::subagent::SubagentManager>,

    pub event_tx: Sender<AppEvent>,
    pub event_rx: Receiver<AppEvent>,
}

impl App {
    pub fn new(work_dir: PathBuf, config: AppConfig) -> Self {
        let (event_tx, event_rx) = mpsc::channel(100);
        let router = Arc::new(ProviderRouter::new(config.clone()));
        let agent = Arc::new(Agent::new(router.clone(), work_dir.clone()));
        let session_manager = Arc::new(SessionManager::new(work_dir.clone(), &config.storage_mode));
        let subagent_manager = Arc::new(crate::agent::subagent::SubagentManager::new(work_dir.clone(), router.clone()));
        let available_models = router.get_available_models();
        let active_model = config.default_model.clone();
        let file_manager = FileManagerState::new(work_dir.clone());
        let filtered_palette = CommandPalette::get_all();
        let env_manager = EnvironmentManager::new(work_dir.clone());
        let runtime_target = RuntimeTarget::Host;
        let theme_mode = AppTheme::parse(&config.theme).unwrap_or_default();
        let current_theme = AppTheme::get(&theme_mode);
        let current_lang = crate::i18n::Language::parse(&config.language).unwrap_or_default();
        let checkpoint_manager = crate::agent::checkpoint::CheckpointManager::new(work_dir.clone());

        // Asynchroniczne dynamiczne wykrywanie modeli w tle przy starcie
        let router_bg = router.clone();
        let tx_bg = event_tx.clone();
        tokio::spawn(async move {
            let discovered = router_bg.discover_models().await;
            let _ = tx_bg.send(AppEvent::ModelsDiscovered(discovered)).await;
        });

        // Uruchomienie serwera Web Companion w tle (jeśli włączony)
        if config.web_companion_enabled {
            let companion = Arc::new(crate::web::WebCompanionServer::new(
                config.web_companion_port,
                work_dir.clone(),
                config.clone(),
            ));
            companion.start_background(event_tx.clone());
        }

        // Wczytaj ostatnią sesję lub utwórz nową
        let current_session = session_manager
            .get_latest_session()
            .unwrap_or_else(|| session_manager.create_session(&active_model));

        let messages = current_session.messages.clone();
        let available_sessions = session_manager.list_sessions().unwrap_or_default();

        Self {
            config,
            work_dir,
            router,
            agent,
            session_manager,
            env_manager,
            runtime_target,
            current_theme,
            current_lang,
            current_session,
            messages,
            input_text: String::new(),
            prompt_history: Vec::new(),
            history_index: None,
            active_model,
            agent_mode: "coder".to_string(),
            auto_check: false,
            is_streaming: false,
            streaming_buffer: String::new(),
            scroll_offset: 0,
            show_model_picker: false,
            model_picker_index: 0,
            model_filter_index: 1,
            available_models,
            dynamic_models: Vec::new(),
            show_session_picker: false,
            session_picker_index: 0,
            available_sessions,
            show_file_manager: false,
            file_manager,
            show_command_palette: false,
            palette_index: 0,
            palette_query: String::new(),
            filtered_palette,
            show_theme_picker: false,
            theme_picker_index: 0,
            show_sidebar: true,
            spinner_frame: 0,
            checkpoint_manager,
            subagent_manager,
            event_tx,
            event_rx,
        }
    }

    pub fn model_provider_tabs() -> Vec<(&'static str, &'static str)> {
        vec![
            ("★ Favorites", "fav"),
            ("All Models", "all"),
            ("OpenCode", "opencode"),
            ("Antigravity", "antigravity"),
            ("Trae AI", "trae"),
            ("Cursor Pro", "cursor"),
            ("Windsurf", "windsurf"),
            ("CommandCode", "commandcode"),
            ("OpenAI", "openai"),
            ("DeepSeek", "deepseek"),
            ("Gemini API", "gemini"),
            ("Groq Speed", "groq"),
            ("Mistral", "mistral"),
            ("OpenRouter", "openrouter"),
            ("LM Studio", "lmstudio"),
            ("Llama.cpp", "llamacpp"),
            ("Ollama Local", "ollama"),
        ]
    }

    pub fn filtered_models(&self) -> Vec<(String, String, String)> {
        let tabs = Self::model_provider_tabs();
        let (_, tag) = tabs[self.model_filter_index % tabs.len()];

        // Użyj modeli dynamicznie wykrytych, a jeśli jeszcze nie gotowe — modeli bazowych
        let source_models = if !self.dynamic_models.is_empty() {
            self.dynamic_models.clone()
        } else {
            self.available_models
                .iter()
                .map(|(id, name, prov)| (id.to_string(), name.to_string(), prov.to_string()))
                .collect()
        };

        source_models
            .into_iter()
            .filter(|(id, _, prov)| {
                if tag == "fav" {
                    self.config.favorite_models.contains(id)
                } else if tag == "all" {
                    true
                } else {
                    prov == tag
                }
            })
            .collect()
    }

    pub fn refresh_sessions_list(&mut self) {
        if let Ok(list) = self.session_manager.list_sessions() {
            self.available_sessions = list;
            if self.session_picker_index >= self.available_sessions.len() && !self.available_sessions.is_empty() {
                self.session_picker_index = self.available_sessions.len() - 1;
            }
        }
    }

    pub fn save_current_session(&mut self) {
        self.current_session.messages = self.messages.clone();
        self.current_session.updated_at = Utc::now().to_rfc3339();
        self.current_session.model = self.active_model.clone();
        self.session_manager.save_session(&self.current_session).ok();
    }

    pub fn estimate_tokens_and_cost(&self) -> (usize, usize, usize, String) {
        let mut total_chars = 0;
        for m in &self.messages {
            total_chars += m.content.len();
        }
        total_chars += self.streaming_buffer.len();

        let est_tokens = total_chars / 4;
        let max_tokens = 128_000;
        let percent = ((est_tokens as f32 / max_tokens as f32) * 100.0).min(100.0) as usize;

        let cost = CostEstimator::estimate_cost(&self.active_model, total_chars);
        let cost_str = CostEstimator::format_cost(cost);

        (est_tokens, max_tokens, percent, cost_str)
    }

    pub fn refresh_palette_filter(&mut self) {
        self.filtered_palette = CommandPalette::filter(&self.palette_query);
        if self.palette_index >= self.filtered_palette.len() && !self.filtered_palette.is_empty() {
            self.palette_index = self.filtered_palette.len() - 1;
        }
    }

    pub async fn handle_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        // Ignoruj zdarzenia puszczenia klawisza (Release/Repeat) – przetwarzaj TYLKO jedno wciśnięcie (Press)
        if key.kind != KeyEventKind::Press {
            return Ok(false);
        }

        // Wyjście Ctrl+C -> zapisz sesję i wyjdź
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.save_current_session();
            return Ok(true);
        }

        // Przełączanie widoczności prawego panelu bocznego Ctrl+B
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('b') {
            self.show_sidebar = !self.show_sidebar;
            return Ok(false);
        }

        // Paleta Motywów i Kolorów Ctrl+K
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('k') {
            self.show_theme_picker = !self.show_theme_picker;
            if self.show_theme_picker {
                self.show_model_picker = false;
                self.show_session_picker = false;
                self.show_file_manager = false;
                self.show_command_palette = false;
            }
            return Ok(false);
        }

        // Paleta Komend Ctrl+P
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p') {
            self.show_command_palette = !self.show_command_palette;
            if self.show_command_palette {
                self.show_model_picker = false;
                self.show_session_picker = false;
                self.show_file_manager = false;
                self.show_theme_picker = false;
                self.palette_query.clear();
                self.palette_index = 0;
                self.refresh_palette_filter();
            }
            return Ok(false);
        }

        // Otwieranie / zamykanie eksploratora plików Ctrl+E
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('e') {
            self.show_file_manager = !self.show_file_manager;
            if self.show_file_manager {
                self.show_model_picker = false;
                self.show_session_picker = false;
                self.show_command_palette = false;
                self.show_theme_picker = false;
                self.file_manager.refresh_all();
            }
            return Ok(false);
        }

        // Przełączanie trybu agenta Ctrl+T (Coder -> Architect -> Ask -> Interactive (Cline) -> Auto (Unattended))
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('t') {
            self.agent_mode = match self.agent_mode.as_str() {
                "coder" => "architect".to_string(),
                "architect" => "ask".to_string(),
                "ask" => "interactive".to_string(),
                "interactive" => "auto".to_string(),
                _ => "coder".to_string(),
            };
            // Auto = unattended -> trust_mode ON, Interactive = Cline-like z podglądem
            if self.agent_mode == "auto" {
                self.config.trust_mode = true;
            }
            let desc = match self.agent_mode.as_str() {
                "interactive" => "Interactive (Cline-like) — wykonuje narzędzia z podglądem inline diff, potwierdzenie Enter",
                "auto" => "Auto (Unattended) — pełna autonomia, bez pytań, trust_mode=ON",
                _ => "",
            };
            self.messages.push(ChatMessage {
                role: "system".to_string(),
                content: if desc.is_empty() {
                    format!("Przełączono tryb agenta na: [{}]", self.agent_mode.to_uppercase())
                } else {
                    format!("Przełączono tryb agenta na: [{}] — {}", self.agent_mode.to_uppercase(), desc)
                },
            });
            return Ok(false);
        }

        // Live Grep Ctrl+F — pełna implementacja ripgrep + preview
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('f') {
            self.input_text = "/grep ".to_string();
            return Ok(false);
        }

        // TC: Multi-rename Ctrl+M w file managerze, inaczej model picker
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('m') {
            if self.show_file_manager {
                // Ctrl+M w file managerze → multi-rename dialog
                let sel = self.file_manager.active().selected_or_current_paths();
                if sel.is_empty() {
                    self.messages.push(ChatMessage { role: "system".to_string(), content: "Brak plików do przemianowania (Space/Ins zaznacz)".to_string() });
                } else {
                    self.input_text = "/rename *.* ".to_string();
                    self.show_file_manager = false;
                }
                return Ok(false);
            }
            self.show_model_picker = !self.show_model_picker;
            if self.show_model_picker {
                self.show_session_picker = false;
                self.show_file_manager = false;
                self.show_command_palette = false;
            }
            return Ok(false);
        }

        // TC: Hotlist Ctrl+D — ulubione ścieżki
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('d') {
            let hotlist = self.file_manager.active().drive_hotlist.clone();
            if hotlist.is_empty() {
                self.messages.push(ChatMessage { role: "system".to_string(), content: "Hotlist pusty — dodaj via /hotlist add".to_string() });
            } else {
                let list = hotlist.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n");
                self.messages.push(ChatMessage { role: "system".to_string(), content: format!("⭐ Hotlist (Ctrl+D):\n{}", list) });
            }
            return Ok(false);
        }

        // Otwieranie / zamykanie historii sesji Ctrl+H
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('h') {
            self.show_session_picker = !self.show_session_picker;
            if self.show_session_picker {
                self.show_model_picker = false;
                self.show_file_manager = false;
                self.show_command_palette = false;
                self.refresh_sessions_list();
            }
            return Ok(false);
        }

        // Nowa sesja Ctrl+N
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('n') {
            self.start_new_session();
            return Ok(false);
        }

        // Czyszczenie czatu Ctrl+L
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('l') {
            self.messages.clear();
            self.streaming_buffer.clear();
            self.save_current_session();
            return Ok(false);
        }

        // Modalne okno wyboru motywu (Ctrl+K)
        if self.show_theme_picker {
            let themes = AppTheme::list_all();
            match key.code {
                KeyCode::Esc => self.show_theme_picker = false,
                KeyCode::Up => {
                    if self.theme_picker_index > 0 {
                        self.theme_picker_index -= 1;
                    }
                }
                KeyCode::Down => {
                    if self.theme_picker_index + 1 < themes.len() {
                        self.theme_picker_index += 1;
                    }
                }
                KeyCode::Enter => {
                    if let Some((id, _, _)) = themes.get(self.theme_picker_index) {
                        if let Some(mode) = AppTheme::parse(id) {
                            self.current_theme = AppTheme::get(&mode);
                            self.config.theme = id.to_string();
                            self.config.save().ok();
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("🎨 Zmieniono motyw na: [{}]", self.current_theme.name),
                            });
                        }
                    }
                    self.show_theme_picker = false;
                }
                _ => {}
            }
            return Ok(false);
        }

        // Modalne okno Palety Komend (Ctrl+P)
        if self.show_command_palette {
            match key.code {
                KeyCode::Esc => self.show_command_palette = false,
                KeyCode::Up => {
                    if self.palette_index > 0 {
                        self.palette_index -= 1;
                    }
                }
                KeyCode::Down => {
                    if self.palette_index + 1 < self.filtered_palette.len() {
                        self.palette_index += 1;
                    }
                }
                KeyCode::Backspace => {
                    if self.palette_query.is_empty() {
                        self.show_command_palette = false;
                    } else {
                        self.palette_query.pop();
                        self.refresh_palette_filter();
                    }
                }
                KeyCode::Char(c) => {
                    self.palette_query.push(c);
                    self.refresh_palette_filter();
                }
                KeyCode::Enter => {
                    if let Some(item) = self.filtered_palette.get(self.palette_index) {
                        let cmd = item.command;
                        self.show_command_palette = false;

                        match cmd {
                            "/search" => {
                                self.input_text = "/search ".to_string();
                            }
                            "/ssh" => {
                                self.input_text = "/ssh ".to_string();
                            }
                            "/remote" | "/remotes" => {
                                self.input_text = "/remote add private ".to_string();
                            }
                            "/files" => {
                                self.file_manager.refresh_all();
                                self.show_file_manager = true;
                            }
                            "/theme" => {
                                self.show_theme_picker = true;
                            }
                            "/model" => {
                                self.show_model_picker = true;
                            }
                            "/history" | "/sessions" => {
                                self.refresh_sessions_list();
                                self.show_session_picker = true;
                            }
                            _ => {
                                // Wykonaj natychmiast każdą komendę (/branch, /wsl, /target, /env, /docker, /local, /commit, /review, /undo, /mode, /new, /vault itp.)
                                self.submit_prompt(cmd.to_string()).await?;
                            }
                        }
                    }
                }
                _ => {}
            }
            return Ok(false);
        }

        // Modalne okno Dwupanelowego Eksploratora Plików (Ctrl+E / /files)
        // Tab często przechwytywany przez Windows Terminal — dodano alternatywy `o`/`]`/`[` i `Ctrl+O`
        if self.show_file_manager {
            let root = self.work_dir.clone();
            match key.code {
                KeyCode::Esc => self.show_file_manager = false,
                KeyCode::Tab | KeyCode::Char('\t') | KeyCode::Char('o') | KeyCode::Char(']') | KeyCode::Char('[') => self.file_manager.switch_pane(),
                KeyCode::F(2) => { self.file_manager.refresh_all(); self.messages.push(ChatMessage { role: "system".to_string(), content: "🔄 Odświeżono (F2)".to_string() }); },
                KeyCode::F(3) => {
                    if let Some(item) = self.file_manager.active().get_selected_item() {
                        if !item.is_dir && !item.is_parent {
                            let content = std::fs::read_to_string(&item.path).unwrap_or_else(|_| "[binarny]".to_string());
                            let preview: String = content.lines().take(40).collect::<Vec<_>>().join("\n");
                            self.messages.push(ChatMessage { role: "system".to_string(), content: format!("👁️ Quick View F3 `{}`:\n```\n{}\n```", item.name, preview) });
                        } else if item.path.extension().and_then(|e| e.to_str()) == Some("zip") {
                            if let Ok(list) = crate::file_manager::FileManagerState::zip_preview(&item.path) {
                                self.messages.push(ChatMessage { role: "system".to_string(), content: format!("📦 ZIP `{}`:\n{}", item.name, list.join("\n")) });
                            }
                        }
                    }
                },
                KeyCode::F(4) => {
                    if let Some(item) = self.file_manager.active().get_selected_item() {
                        if !item.is_dir {
                            let editor = std::env::var("EDITOR").unwrap_or_else(|_| if cfg!(target_os = "windows") { "notepad".to_string() } else { "nano".to_string() });
                            let _ = std::process::Command::new(editor).arg(&item.path).status();
                            self.file_manager.refresh_all();
                        }
                    }
                },
                KeyCode::Up => self.file_manager.active_mut().navigate_up(),
                KeyCode::Down => self.file_manager.active_mut().navigate_down(),
                KeyCode::Backspace => self.file_manager.active_mut().go_to_parent(&root),
                KeyCode::Char('c') | KeyCode::F(5) => {
                    match self.file_manager.copy_to_other_pane() {
                        Ok(msg) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("📋 {}", msg),
                            });
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("❌ Błąd kopiowania: {}", e),
                            });
                        }
                    }
                }
                KeyCode::Char('m') | KeyCode::F(6) => {
                    match self.file_manager.move_to_other_pane() {
                        Ok(msg) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("🚚 {}", msg),
                            });
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("❌ Błąd przenoszenia: {}", e),
                            });
                        }
                    }
                }
                KeyCode::Char('d') | KeyCode::Delete | KeyCode::F(8) => {
                    match self.file_manager.delete_selected() {
                        Ok(msg) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("🗑️ {}", msg),
                            });
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("❌ Błąd usuwania: {}", e),
                            });
                        }
                    }
                }
                KeyCode::Char('t') => {
                    self.file_manager.active_mut().toggle_view_mode(&root);
                }
                // TC: Space/Ins multi-select, Ctrl+M multi-rename, Ctrl+D hotlist, Alt+F1/F2 drive
                KeyCode::Char(' ') => {
                    // TC: Space = zaznacz + następny (jeśli Ins nie działa)
                    self.file_manager.active_mut().toggle_select_current();
                    // Jeśli nic nie zaznaczono wcześniej, to jednak wklej @file (legacy)
                    if self.file_manager.active().selection_count() == 0 {
                        if let Some(item) = self.file_manager.active().get_selected_item() {
                            if !item.is_parent {
                                // cofnięto toggle, przywróć single paste? nie — Space już zaznaczył, nie wklejaj
                            }
                        }
                    }
                },
                KeyCode::Insert => { self.file_manager.active_mut().toggle_select_current(); },
                KeyCode::Enter => {
                    if let Some(file_path) = self.file_manager.active_mut().enter_selected(&root) {
                        let rel = if let Ok(r) = file_path.strip_prefix(&self.work_dir) {
                            r.to_string_lossy().replace('\\', "/")
                        } else {
                            file_path.to_string_lossy().to_string()
                        };
                        self.input_text.push_str(&format!("@{} ", rel));
                        self.show_file_manager = false;
                    }
                }
                _ => {}
            }
            return Ok(false);
        }

        // Modalne okno wyboru modelu (Ctrl+M)
        if self.show_model_picker {
            let filtered = self.filtered_models();
            let total_tabs = Self::model_provider_tabs().len();

            match key.code {
                KeyCode::Esc => self.show_model_picker = false,
                KeyCode::Left | KeyCode::BackTab | KeyCode::Char('[') => {
                    self.model_filter_index = if self.model_filter_index > 0 {
                        self.model_filter_index - 1
                    } else {
                        total_tabs.saturating_sub(1)
                    };
                    self.model_picker_index = 0;
                }
                KeyCode::Right | KeyCode::Tab | KeyCode::Char('\t') | KeyCode::Char(']') => {
                    self.model_filter_index = (self.model_filter_index + 1) % total_tabs;
                    self.model_picker_index = 0;
                }
                KeyCode::Up => {
                    if self.model_picker_index > 0 {
                        self.model_picker_index -= 1;
                    }
                }
                KeyCode::Down => {
                    if self.model_picker_index + 1 < filtered.len() {
                        self.model_picker_index += 1;
                    }
                }
                KeyCode::Char('f') | KeyCode::Char(' ') => {
                    if let Some((id, _, _)) = filtered.get(self.model_picker_index) {
                        let id_str = id.to_string();
                        if let Some(pos) = self.config.favorite_models.iter().position(|x| x == &id_str) {
                            self.config.favorite_models.remove(pos);
                        } else {
                            self.config.favorite_models.push(id_str);
                        }
                        let _ = self.config.save();
                    }
                }
                KeyCode::Enter => {
                    if let Some((id, _, _)) = filtered.get(self.model_picker_index) {
                        self.active_model = id.to_string();
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("◈ Przełączono operatora na: {}", self.active_model),
                        });
                        self.save_current_session();
                    }
                    self.show_model_picker = false;
                }
                _ => {}
            }
            return Ok(false);
        }

        // Modalne okno historii sesji (Ctrl+H)
        if self.show_session_picker {
            match key.code {
                KeyCode::Esc => self.show_session_picker = false,
                KeyCode::Up => {
                    if self.session_picker_index > 0 {
                        self.session_picker_index -= 1;
                    }
                }
                KeyCode::Down => {
                    if self.session_picker_index + 1 < self.available_sessions.len() {
                        self.session_picker_index += 1;
                    }
                }
                KeyCode::Enter => {
                    if let Some(meta) = self.available_sessions.get(self.session_picker_index) {
                        let id = meta.id.clone();
                        self.load_session_by_id(&id);
                    }
                    self.show_session_picker = false;
                }
                KeyCode::Delete | KeyCode::Char('d') => {
                    if let Some(meta) = self.available_sessions.get(self.session_picker_index) {
                        let id = meta.id.clone();
                        self.session_manager.delete_session(&id).ok();
                        self.refresh_sessions_list();
                        if self.current_session.id == id {
                            self.start_new_session();
                        }
                    }
                }
                _ => {}
            }
            return Ok(false);
        }

        // Blokada wpisywania podczas streamingu
        if self.is_streaming {
            return Ok(false);
        }

        // Wprowadzanie tekstu, nawigacja historii i zaawansowane przewijanie
        match key.code {
            KeyCode::Char(c) => {
                if c == '/' && self.input_text.is_empty() {
                    self.show_command_palette = true;
                    self.palette_query.clear();
                    self.palette_index = 0;
                    self.refresh_palette_filter();
                } else {
                    self.input_text.push(c);
                    self.history_index = None;
                }
            }
            KeyCode::Backspace => {
                self.input_text.pop();
                self.history_index = None;
            }
            KeyCode::Up => {
                if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Płynne przewijanie czatu w górę o 3 linie
                    self.scroll_offset = self.scroll_offset.saturating_add(3);
                } else if !self.prompt_history.is_empty() {
                    let next_idx = match self.history_index {
                        Some(i) => if i > 0 { i - 1 } else { 0 },
                        None => self.prompt_history.len().saturating_sub(1),
                    };
                    self.history_index = Some(next_idx);
                    if let Some(prev) = self.prompt_history.get(next_idx) {
                        self.input_text = prev.clone();
                    }
                }
            }
            KeyCode::Down => {
                if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Płynne przewijanie czatu w dół o 3 linie
                    self.scroll_offset = self.scroll_offset.saturating_sub(3);
                } else if let Some(i) = self.history_index {
                    if i + 1 < self.prompt_history.len() {
                        let next_idx = i + 1;
                        self.history_index = Some(next_idx);
                        if let Some(next) = self.prompt_history.get(next_idx) {
                            self.input_text = next.clone();
                        }
                    } else {
                        self.history_index = None;
                        self.input_text.clear();
                    }
                }
            }
            KeyCode::PageUp => {
                // Skok o pół strony w górę
                self.scroll_offset = self.scroll_offset.saturating_add(8);
            }
            KeyCode::PageDown => {
                // Skok o pół strony w dół
                self.scroll_offset = self.scroll_offset.saturating_sub(8);
            }
            KeyCode::Home if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Skok na samą górę historii
                self.scroll_offset = 10_000;
            }
            KeyCode::End if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Skok na sam dół (najnowsze wiadomości)
                self.scroll_offset = 0;
            }
            KeyCode::Enter => {
                let prompt = self.input_text.trim().to_string();
                if !prompt.is_empty() {
                    self.prompt_history.push(prompt.clone());
                    self.history_index = None;
                    self.scroll_offset = 0; // Przywróć widok na sam dół przy nowej wiadomości
                    self.submit_prompt(prompt).await?;
                }
            }
            _ => {}
        }

        Ok(false)
    }

    pub fn handle_mouse_event(&mut self, mouse: crossterm::event::MouseEvent) {
        use crossterm::event::MouseEventKind;
        match mouse.kind {
            MouseEventKind::ScrollDown => {
                // Kółko w dół = fizycznie w dół (poprzednio odwrotnie na Windows Terminal)
                if self.show_file_manager {
                    self.file_manager.active_mut().navigate_up();
                } else if self.show_theme_picker {
                    if self.theme_picker_index > 0 {
                        self.theme_picker_index -= 1;
                    }
                } else if self.show_command_palette {
                    if self.palette_index > 0 {
                        self.palette_index -= 1;
                    }
                } else if self.show_model_picker {
                    if self.model_picker_index > 0 {
                        self.model_picker_index -= 1;
                    }
                } else if self.show_session_picker {
                    if self.session_picker_index > 0 {
                        self.session_picker_index -= 1;
                    }
                } else {
                    self.scroll_offset = self.scroll_offset.saturating_add(3);
                }
            }
            MouseEventKind::ScrollUp => {
                // Kółko w górę = fizycznie w górę
                if self.show_file_manager {
                    self.file_manager.active_mut().navigate_down();
                } else if self.show_theme_picker {
                    let total = AppTheme::list_all().len();
                    if self.theme_picker_index + 1 < total {
                        self.theme_picker_index += 1;
                    }
                } else if self.show_command_palette {
                    if self.palette_index + 1 < self.filtered_palette.len() {
                        self.palette_index += 1;
                    }
                } else if self.show_model_picker {
                    let total = self.filtered_models().len();
                    if self.model_picker_index + 1 < total {
                        self.model_picker_index += 1;
                    }
                } else if self.show_session_picker {
                    if self.session_picker_index + 1 < self.available_sessions.len() {
                        self.session_picker_index += 1;
                    }
                } else {
                    self.scroll_offset = self.scroll_offset.saturating_sub(3);
                }
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Left)
                if self.show_file_manager => {
                    if mouse.column > 50 {
                        self.file_manager.active_pane = crate::file_manager::ActivePane::Right;
                    } else {
                        self.file_manager.active_pane = crate::file_manager::ActivePane::Left;
                    }
                }
            _ => {}
        }
    }

    pub fn start_new_session(&mut self) {
        self.save_current_session();
        let new_session = self.session_manager.create_session(&self.active_model);
        self.current_session = new_session;
        self.messages = self.current_session.messages.clone();
        self.session_manager.save_session(&self.current_session).ok();
        self.refresh_sessions_list();
    }

    pub fn load_session_by_id(&mut self, id: &str) {
        self.save_current_session();
        if let Ok(session) = self.session_manager.load_session(id) {
            self.active_model = session.model.clone();
            self.messages = session.messages.clone();
            self.current_session = session;
            self.messages.push(ChatMessage {
                role: "system".to_string(),
                content: format!("Wczytano sesję: {} [{}]", self.current_session.title, self.current_session.id),
            });
        }
    }

    async fn submit_prompt(&mut self, mut prompt: String) -> Result<()> {
        self.input_text.clear();

        // Jeśli to szablon (/refactor, /tests, /doc, /explain), rozwiń go
        let first_word = prompt.split_whitespace().next().unwrap_or_default();
        if let Some(template_prompt) = TemplateManager::resolve_template(first_word) {
            let parts: Vec<&str> = prompt.split_whitespace().collect();
            let user_arg = if parts.len() > 1 { parts[1..].join(" ") } else { String::new() };
            prompt = if user_arg.is_empty() {
                template_prompt.to_string()
            } else {
                format!("{}\n\nDodatkowy kontekst użytkownika: {}", template_prompt, user_arg)
            };
        } else if prompt.starts_with('/') {
            return self.handle_command(&prompt).await;
        } else if prompt.starts_with('!') {
            // Wykrzyknik: wykonaj shell bezpośrednio (jak w opencode: !ls, !cargo test)
            let shell_cmd = prompt.trim_start_matches('!').trim();
            if !shell_cmd.is_empty() {
                let out = crate::runtime::RuntimeEngine::exec(&self.work_dir, &self.runtime_target, shell_cmd);
                let msg = match out {
                    Ok(o) => format!("$ {}\n{}", shell_cmd, o),
                    Err(e) => format!("$ {}  → błąd:\n{}", shell_cmd, e),
                };
                self.messages.push(ChatMessage { role: "system".to_string(), content: msg });
                self.save_current_session();
                return Ok(());
            }
        }

        self.start_agent_stream(prompt);
        Ok(())
    }

    fn start_agent_stream(&mut self, prompt: String) {
        // Aktualizacja tytułu nowej sesji na podstawie promptu
        if self.current_session.title == "Nowy czat" {
            let clean_title: String = prompt.chars().take(40).collect();
            self.current_session.title = clean_title;
        }

        self.messages.push(ChatMessage {
            role: "user".to_string(),
            content: prompt.clone(),
        });
        self.save_current_session();

        self.is_streaming = true;
        self.streaming_buffer.clear();

        let agent = self.agent.clone();
        let model = self.active_model.clone();
        let mode = self.agent_mode.clone();
        let history = self.messages.clone();
        let (token_tx, mut token_rx) = mpsc::channel(100);
        let app_tx = self.event_tx.clone();

        // Task streamujący tokeny
        let app_tx_clone = app_tx.clone();
        tokio::spawn(async move {
            while let Some(token) = token_rx.recv().await {
                let _ = app_tx_clone.send(AppEvent::Token(token)).await;
            }
        });

        // Task pętli agenta
        tokio::spawn(async move {
            match agent.process_user_prompt(&model, &mode, &history, &prompt, token_tx).await {
                Ok(used_model) => {
                    let _ = app_tx.send(AppEvent::StreamFinished(used_model)).await;
                }
                Err(err) => {
                    let _ = app_tx.send(AppEvent::StreamError(err.to_string())).await;
                }
            }
        });
    }

    async fn handle_command(&mut self, cmd: &str) -> Result<()> {
        let parts: Vec<&str> = cmd.split_whitespace().collect();

        match parts[0] {
            "/lang" | "/language" => {
                if parts.len() > 1 {
                    if parts[1] == "export" {
                        let path = if parts.len() > 2 {
                            std::path::PathBuf::from(parts[2])
                        } else {
                            self.work_dir.join(".opencode").join("locales").join("locale_template.json")
                        };
                        match crate::i18n::Language::export_template(&path) {
                            Ok(_) => {
                                self.messages.push(ChatMessage {
                                    role: "system".to_string(),
                                    content: format!("📄 Wyeksportowano szablon tłumaczenia do:\n`{}`\nMożesz go przetłumaczyć i załadować przez: `/lang load <ścieżka>`", path.display()),
                                });
                            }
                            Err(e) => {
                                self.messages.push(ChatMessage {
                                    role: "system".to_string(),
                                    content: format!("❌ Błąd eksportu szablonu: {e}"),
                                });
                            }
                        }
                    } else if parts[1] == "load" {
                        if parts.len() < 3 {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: "Użycie: `/lang load <ścieżka_do_pliku.json>`".to_string(),
                            });
                        } else {
                            let path = std::path::Path::new(parts[2]);
                            match crate::i18n::Language::from_custom_file(path) {
                                Ok(custom_lang) => {
                                    let name = custom_lang.display_name().to_string();
                                    self.current_lang = custom_lang;
                                    self.messages.push(ChatMessage {
                                        role: "system".to_string(),
                                        content: format!("🌍 Załadowano i aktywowano własny język: [{name}]"),
                                    });
                                }
                                Err(e) => {
                                    self.messages.push(ChatMessage {
                                        role: "system".to_string(),
                                        content: format!("❌ Błąd ładowania pliku językowego: {e}"),
                                    });
                                }
                            }
                        }
                    } else if let Some(lang) = crate::i18n::Language::parse(parts[1]) {
                        self.current_lang = lang.clone();
                        self.config.language = lang.code().to_string();
                        self.config.save().ok();
                        let msg = match lang {
                            crate::i18n::Language::English => "🌍 Language switched to: [English]",
                            crate::i18n::Language::Polish => "🌍 Zmieniono język interfejsu na: [Polski]",
                            crate::i18n::Language::Chinese => "🌍 界面语言已切换为: [简体中文]",
                            crate::i18n::Language::German => "🌍 Sprache geändert zu: [Deutsch]",
                            crate::i18n::Language::Spanish => "🌍 Idioma cambiado a: [Español]",
                            crate::i18n::Language::French => "🌍 Langue changée en: [Français]",
                            crate::i18n::Language::Ukrainian => "🌍 Мову змінено на: [Українська]",
                            crate::i18n::Language::Custom(_) => "🌍 Własny język",
                        };
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: msg.to_string(),
                        });
                    } else {
                        let list = crate::i18n::Language::list_all()
                            .into_iter()
                            .map(|(code, name)| format!("{code} ({name})"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("Unknown language. Available: {}\nOr load your custom JSON: `/lang load <path.json>`", list),
                        });
                    }
                } else {
                    let list = crate::i18n::Language::list_all()
                        .into_iter()
                        .map(|(code, name)| format!("• `/lang {}` - {}", code, name))
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!("🌍 Available languages / Dostępne języki:\n{}\n\n✨ Własne języki:\n• `/lang export` - wygeneruj szablon JSON do tłumaczenia\n• `/lang load <plik.json>` - załaduj własne tłumaczenie", list),
                    });
                }
            }
            "/checkpoint" => {
                let name = if parts.len() > 1 { parts[1] } else { "snapshot" };
                match self.checkpoint_manager.create_checkpoint(name) {
                    Ok(msg) => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("💾 {}", msg),
                        });
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("❌ Błąd tworzenia punktu kontrolnego: {}", e),
                        });
                    }
                }
            }
            "/rollback" | "/restore" => {
                match self.checkpoint_manager.rollback_latest() {
                    Ok(msg) => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("⏪ {}", msg),
                        });
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("❌ Błąd cofania: {}", e),
                        });
                    }
                }
            }
            "/memory" => {
                let mb = crate::memory::MemoryBlocks::new(self.work_dir.clone());
                if parts.len() > 1 {
                    match parts[1] {
                        "show" => {
                            let label = parts.get(2).copied().unwrap_or("");
                            let content = mb.load_block(label);
                            let msg = if content.is_empty() {
                                format!("🧠 Blok '{label}' jest pusty.")
                            } else {
                                format!("🧠 Blok '{label}' ({} znaków):\n```\n{}\n```", content.len(), content)
                            };
                            self.messages.push(ChatMessage { role: "system".to_string(), content: msg });
                        }
                        "clear" => {
                            let label = parts.get(2).copied().unwrap_or("");
                            if label.is_empty() {
                                self.messages.push(ChatMessage { role: "system".to_string(), content: "❌ Użycie: /memory clear <label>". to_string() });
                            } else {
                                match mb.save_block(label, "") {
                                    Ok(_) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("🧹 Wyczyszczono blok '{label}'.") }),
                                    Err(e) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("❌ {e}") }),
                                }
                            }
                        }
                        "push" => {
                            let msg = parts[2..].join(" ");
                            match crate::memory::MemoryBlocks::git_push_memory(&msg) {
                                Ok(o) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("📤 /memory push:\n{o}") }),
                                Err(e) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("❌ /memory push: {e}") }),
                            }
                        }
                        "pull" => {
                            match crate::memory::MemoryBlocks::git_pull_memory() {
                                Ok(o) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("📥 /memory pull:\n{o}") }),
                                Err(e) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("❌ /memory pull: {e}") }),
                            }
                        }
                        "status" => {
                            match crate::memory::MemoryBlocks::git_status_memory() {
                                Ok(o) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("📋 /memory status:\n```\n{o}\n```") }),
                                Err(e) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("❌ /memory status: {e}") }),
                            }
                        }
                        "remote" => {
                            let url = parts.get(2).copied().unwrap_or("");
                            if url.is_empty() {
                                self.messages.push(ChatMessage { role: "system".to_string(), content: "❌ Użycie: /memory remote <git-url> (np. git@github.com:user/opencode-memory.git)".to_string() });
                            } else {
                                match crate::memory::MemoryBlocks::git_set_remote(url) {
                                    Ok(o) => self.messages.push(ChatMessage { role: "system".to_string(), content: o }),
                                    Err(e) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("❌ {e}") }),
                                }
                            }
                        }
                        _ => {
                            self.messages.push(ChatMessage { role: "system".to_string(), content: "❌ Użycie: /memory | /memory show <label> | /memory clear <label> | /memory push [msg] | /memory pull | /memory status | /memory remote <url>".to_string() });
                        }
                    }
                } else {
                    self.messages.push(ChatMessage { role: "system".to_string(), content: mb.status_report() });
                }
            }
            "/doctor" => {
                let mb = crate::memory::MemoryBlocks::new(self.work_dir.clone());
                self.messages.push(ChatMessage { role: "system".to_string(), content: mb.doctor_report() });
            }
            "/skill-learn" => {
                // Refleksja: każe agentowi przemyśleć sesję i utworzyć learned skill z doświadczenia.
                let all_msgs: Vec<&ChatMessage> = self.messages.iter()
                    .filter(|m| m.role == "user" || m.role == "assistant")
                    .collect();
                let start_idx = all_msgs.len().saturating_sub(20);
                let history_summary: String = all_msgs[start_idx..]
                    .iter()
                    .map(|m| format!("[{}]: {}", m.role, m.content.chars().take(400).collect::<String>()))
                    .collect::<Vec<_>>()
                    .join("\n");
                let skill_name = parts.get(1).copied().unwrap_or("");
                let name_hint = if skill_name.is_empty() {
                    "Wybierz sam krótką, kebab-case nazwę (np. db-migration, deploy-vercel).".to_string()
                } else {
                    format!("Użyj nazwy: '{skill_name}'.", )
                };
                let learn_prompt = format!(
                    "SKILL LEARNING /skill-learn — przeanalizuj poniższą sesję i utwórz learned skill używając create_skill(name, content).\n\nZasady:\n1. Wyciągnij UOGÓLNIONĄ procedurę z tej sesji — nie loguj konkretów, zrób reużywalny skill.\n2. {name_hint}\n3. Treść SKILL.md w markdown: krótki opis kiedy używać + ponumerowane kroki + przykłady komend/kodu.\n4. Po utworzeniu skilla, krótko podsumuj co zapisałeś.\n\nPodsumowanie ostatnich wiadomości sesji:\n{history_summary}\n\nZacznij analizę i utwórz skill."
                );
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: "🎓 /skill-learn — tworzenie learned skilla z doświadczenia...".to_string(),
                });
                self.start_agent_stream(learn_prompt);
                return Ok(());
            }
            "/remember" => {
                // Refleksja: każe agentowi przemyśleć sesję i zapisać wnioski do memory blocks.
                let all_msgs: Vec<&ChatMessage> = self.messages.iter()
                    .filter(|m| m.role == "user" || m.role == "assistant")
                    .collect();
                let start_idx = all_msgs.len().saturating_sub(20);
                let history_summary: String = all_msgs[start_idx..]
                    .iter()
                    .map(|m| format!("[{}]: {}", m.role, m.content.chars().take(300).collect::<String>()))
                    .collect::<Vec<_>>()
                    .join("\n");
                let reflection_prompt = format!(
                    "REFLEKSJA /remember — przeanalizuj poniższą sesję i zapisz wnioski do memory blocks używając core_memory_append (lub core_memory_replace jeśli aktualizujesz istniejącą wiedzę).\n\nZasady:\n1. GENERALIZUJ — nie loguj pojedynczych zdarzeń, wyciągnij wzorce/preferencje/wiedzę o projekcie.\n2. Wybierz odpowiedni blok: 'human' (preferencje usera), 'project' (wiedza o tym projekcie), 'persona' (jak Ty jako agent powinieneś działać).\n3. Nie duplikuj wiedzy już zapisanej — jeśli coś się zmieniło, użyj core_memory_replace.\n4. Krótko i konkretnie — bloki to cenny real estate (limit 4000 znaków).\n5. Po zapisaniu, krótko podsumuj co zapisałeś i do których bloków.\n\nPodsumowanie ostatnich wiadomości sesji:\n{history_summary}\n\nZacznij refleksję i zapisz wnioski do pamięci."
                );
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: "🧠 /remember — refleksja i zapis do memory blocks...".to_string(),
                });
                self.start_agent_stream(reflection_prompt);
                return Ok(());
            }
            "/theme" => {
                if parts.len() > 1 {
                    if let Some(mode) = AppTheme::parse(parts[1]) {
                        self.current_theme = AppTheme::get(&mode);
                        self.config.theme = parts[1].to_lowercase();
                        self.config.save().ok();
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("🎨 Zmieniono motyw na: [{}]", self.current_theme.name),
                        });
                    } else {
                        let list = AppTheme::list_all().into_iter().map(|(id, _, _)| id).collect::<Vec<_>>().join(", ");
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("Nieznany motyw. Dostępne motywy: {}\nLub naciśnij [Ctrl+K], aby wybrać wizualnie.", list),
                        });
                    }
                } else {
                    self.show_theme_picker = true;
                }
            }
            "/sync-mode" | "/git-sync-mode" => {
                if parts.len() > 1 {
                    let mode = parts[1].to_lowercase();
                    if mode == "both" || mode == "all" || mode == "dual" {
                        self.config.git_sync_mode = "both".to_string();
                        self.config.save_project_config(&self.work_dir).ok();
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: "🔄 Ustawiono tryb synchronizacji projektu: [DUAL / BOTH] (wypycha do repozytorium publicznego i prywatnego).".to_string(),
                        });
                    } else if mode == "public" || mode == "origin" {
                        self.config.git_sync_mode = "public".to_string();
                        self.config.save_project_config(&self.work_dir).ok();
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: "🌐 Ustawiono tryb synchronizacji projektu: [PUBLIC ONLY] (wypycha tylko do publicznego repozytorium 'origin').".to_string(),
                        });
                    } else if mode == "private" || mode == "priv" {
                        self.config.git_sync_mode = "private".to_string();
                        self.config.save_project_config(&self.work_dir).ok();
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: "🔒 Ustawiono tryb synchronizacji projektu: [PRIVATE ONLY] (wypycha tylko do prywatnego repozytorium 'private').".to_string(),
                        });
                    } else {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: "Nieznany tryb. Dostępne opcje:\n• `/sync-mode both` - wypycha do obu repozytoriów (Dual-Repo)\n• `/sync-mode public` - wypycha tylko do publicznego repo\n• `/sync-mode private` - wypycha tylko do prywatnego repo".to_string(),
                        });
                    }
                } else {
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!("🔄 Aktualny tryb synchronizacji projektu: [{}]\nAby zmienić:\n• `/sync-mode both` (Publiczne + Prywatne)\n• `/sync-mode public` (Tylko Publiczne)\n• `/sync-mode private` (Tylko Prywatne)", self.config.git_sync_mode.to_uppercase()),
                    });
                }
            }
            "/push" | "/sync-git" | "/dual-push" => {
                if parts.len() > 1 && parts[1] != "all" && parts[1] != "both" {
                    let target_remote = parts[1];
                    match GitAssistant::push_to(&self.work_dir, Some(target_remote)) {
                        Ok(out) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("🚀 Git push [{target_remote}] zakończony sukcesem:\n```\n{}\n```", out),
                            });
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("❌ Błąd git push [{target_remote}]: {e}"),
                            });
                        }
                    }
                } else if parts.len() > 1 && (parts[1] == "all" || parts[1] == "both") || self.config.git_sync_mode == "both" {
                    // Wypchnij do wszystkich skonfigurowanych repozytoriów (Dual-Repo)
                    match GitAssistant::push_all_remotes(&self.work_dir) {
                        Ok(summary) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("🚀 {}\nAby zmienić domyślny tryb projektu: `/sync-mode <both|public|private>`", summary),
                            });
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("❌ Błąd git push: {e}"),
                            });
                        }
                    }
                } else if self.config.git_sync_mode == "private" {
                    match GitAssistant::push_to(&self.work_dir, Some("private")) {
                        Ok(out) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("🔒 Git push [private] zakończony sukcesem:\n```\n{}\n```", out),
                            });
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("❌ Błąd git push [private]: {e}"),
                            });
                        }
                    }
                } else {
                    // Domyślnie public
                    match GitAssistant::push_to(&self.work_dir, Some("origin")) {
                        Ok(out) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("🌐 Git push [origin / public] zakończony sukcesem:\n```\n{}\n```", out),
                            });
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("❌ Błąd git push [origin]: {e}"),
                            });
                        }
                    }
                }
            }
            "/pull" => {
                let target_remote = parts.get(1).copied();
                match GitAssistant::pull_from(&self.work_dir, target_remote) {
                    Ok(out) => {
                        let r_name = target_remote.unwrap_or("domyślny");
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("📥 Git pull [{r_name}] zakończony sukcesem:\n```\n{}\n```", out),
                        });
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("❌ Błąd git pull: {e}"),
                        });
                    }
                }
            }
            "/vault" => {
                if parts.len() > 1 && parts[1] == "push" {
                    let private_remote = if parts.len() > 2 { parts[2] } else { "private" };
                    match GitAssistant::push_full_private_vault(&self.work_dir, private_remote) {
                        Ok(msg) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: msg,
                            });
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("❌ Błąd sejfu prywatnego: {e}\nUpewnij się, że masz dodane prywatne repozytorium: `/remote add private <url>`"),
                            });
                        }
                    }
                } else {
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: "🔒 Prywatny Sejf Poświadczeń (Private Vault):\n• `/vault push [nazwa_remote]` - wykonuje pełną kopię WSZYSTKICH plików (w tym `.env`, `.env.*` i poświadczeń) na branch `vault/backup` w prywatnym repozytorium!\n• Publiczny branch roboczy i repozytorium publiczne pozostają w 100% bezpieczne.".to_string(),
                    });
                }
            }
            "/remotes" | "/remote" => {
                if parts.len() > 1 {
                    if parts[1] == "add" {
                        if parts.len() < 4 {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: "Użycie: `/remote add <nazwa> <url>`\nPrzykład: `/remote add private git@github.com:moj-user/prywatne-repo.git`".to_string(),
                            });
                        } else {
                            let name = parts[2];
                            let url = parts[3];
                            match GitAssistant::add_remote(&self.work_dir, name, url) {
                                Ok(msg) => {
                                    self.messages.push(ChatMessage {
                                        role: "system".to_string(),
                                        content: format!("✅ {msg}\nTeraz możesz użyć `/push all` lub `/push {name}`!"),
                                    });
                                }
                                Err(e) => {
                                    self.messages.push(ChatMessage {
                                        role: "system".to_string(),
                                        content: format!("❌ Błąd dodawania remote: {e}"),
                                    });
                                }
                            }
                        }
                    } else if parts[1] == "remove" || parts[1] == "rm" {
                        if parts.len() < 3 {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: "Użycie: `/remote remove <nazwa>`".to_string(),
                            });
                        } else {
                            let name = parts[2];
                            match GitAssistant::remove_remote(&self.work_dir, name) {
                                Ok(msg) => {
                                    self.messages.push(ChatMessage {
                                        role: "system".to_string(),
                                        content: format!("🗑️ {msg}"),
                                    });
                                }
                                Err(e) => {
                                    self.messages.push(ChatMessage {
                                        role: "system".to_string(),
                                        content: format!("❌ Błąd usuwania remote: {e}"),
                                    });
                                }
                            }
                        }
                    } else {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: "Opcje /remote:\n• `/remote` - lista repozytoriów\n• `/remote add <nazwa> <url>` - dodaj repozytorium prywatne/publiczne\n• `/remote remove <nazwa>` - usuń remote".to_string(),
                        });
                    }
                } else {
                    match GitAssistant::get_remotes(&self.work_dir) {
                        Ok(list) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("🌐 Zdalne repozytoria Git (Dual-Repo):\n```\n{}\n```\n📌 Przydatne komendy:\n• `/remote add private <url>` - dodaj prywatne repozytorium\n• `/push all` - wypchnij zmiany jednocześnie do obu repozytoriów!\n• `/push <nazwa>` - wypchnij tylko do wybranego", list),
                            });
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("⚠️ {e}\nAby dodać repozytorium: `/remote add origin <url>` lub `/remote add private <url>`"),
                            });
                        }
                    }
                }
            }
            "/pr" | "/mr" => {
                if let Ok((status, diff)) = GitAssistant::get_status_and_diff(&self.work_dir) {
                    let prompt = format!(
                        "Przygotuj profesjonalny, czytelny opis Pull Requesta / Merge Requesta (Tytuł, Podsumowanie zmian, Wpływ na architekturę, Plan testów) w formacie Markdown dla poniższych zmian:\n\nStatus:\n{}\n\nDiff:\n{}",
                        status, diff
                    );
                    self.start_agent_stream(prompt);
                }
            }
            "/branch" => {
                if parts.len() > 1 {
                    let branch_name = parts[1];
                    match EnvironmentManager::switch_branch(&self.work_dir, branch_name) {
                        Ok(msg) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("🌿 {msg}"),
                            });
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("❌ Błąd przełączania brancha: {e}"),
                            });
                        }
                    }
                } else {
                    let current = EnvironmentManager::get_current_branch(&self.work_dir).unwrap_or_else(|_| "brak gita".to_string());
                    let all_branches = EnvironmentManager::list_branches(&self.work_dir).unwrap_or_default();
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!("🌿 Aktywny branch: `{current}`\n\nLista branchy:\n```\n{}\n```\nAby przełączyć/utworzyć: `/branch <nazwa>`", all_branches),
                    });
                }
            }
            "/target" => {
                if parts.len() > 1 {
                    let target_name = parts[1];
                    let msg = self.env_manager.set_target(target_name, &mut self.config);
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!("🎯 {msg}"),
                    });
                } else {
                    let available = self.env_manager.list_available_targets().join(", ");
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!("🎯 Aktywne środowisko: [{}]\nDostępne/zdefiniowane środowiska: {}\nAby zmienić: `/target <dev|test|prod|własne>`", self.env_manager.active_target.to_uppercase(), available),
                    });
                }
            }
            "/env" => {
                if parts.len() > 1 {
                    match parts[1].to_lowercase().as_str() {
                        "host" | "local" => {
                            self.runtime_target = RuntimeTarget::Host;
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: "💻 Ustawiono środowisko wykonawcze na: [HOST - Lokalny system]".to_string(),
                            });
                        }
                        "wsl" => {
                            let distro = if parts.len() > 2 { Some(parts[2].to_string()) } else { None };
                            self.runtime_target = RuntimeTarget::Wsl { distro: distro.clone() };
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("🐧 Ustawiono środowisko wykonawcze na: [WSL (distro: {})]", distro.as_deref().unwrap_or("domyślne")),
                            });
                        }
                        "docker" => {
                            if parts.len() < 3 {
                                self.messages.push(ChatMessage {
                                    role: "system".to_string(),
                                    content: "Użycie: /env docker <nazwa_lub_id_kontenera>".to_string(),
                                });
                            } else {
                                let container = parts[2].to_string();
                                self.runtime_target = RuntimeTarget::Docker { container: container.clone() };
                                self.messages.push(ChatMessage {
                                    role: "system".to_string(),
                                    content: format!("🐳 Ustawiono środowisko wykonawcze na kontener Docker: [{container}]"),
                                });
                            }
                        }
                        "ssh" => {
                            if parts.len() < 3 {
                                self.messages.push(ChatMessage {
                                    role: "system".to_string(),
                                    content: "Użycie: /env ssh <user@host> [port] [klucz_id_rsa]\nPrzykład: /env ssh root@192.168.1.100".to_string(),
                                });
                            } else {
                                let target_str = parts[2];
                                let (user, host) = if let Some(idx) = target_str.find('@') {
                                    (Some(target_str[..idx].to_string()), target_str[idx + 1..].to_string())
                                } else {
                                    (None, target_str.to_string())
                                };
                                let port = parts.get(3).and_then(|p| p.parse::<u16>().ok());
                                let key_path = parts.get(4).map(|k| k.to_string());

                                self.runtime_target = RuntimeTarget::Ssh {
                                    host: host.clone(),
                                    user: user.clone(),
                                    port,
                                    key_path,
                                };
                                let u_str = user.unwrap_or_else(|| "default".to_string());
                                self.messages.push(ChatMessage {
                                    role: "system".to_string(),
                                    content: format!("🌐 Ustawiono środowisko wykonawcze na zdalny serwer SSH Linux: [{u_str}@{host}]"),
                                });
                            }
                        }
                        _ => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: "Dostępne opcje: /env host, /env wsl [distro], /env docker <kontener>, /env ssh <user@host>".to_string(),
                            });
                        }
                    }
                } else {
                    let runtime_desc = match &self.runtime_target {
                        RuntimeTarget::Host => "Host (Lokalny)".to_string(),
                        RuntimeTarget::Wsl { distro } => format!("WSL (distro: {})", distro.as_deref().unwrap_or("domyślne")),
                        RuntimeTarget::Docker { container } => format!("Docker ({container})"),
                        RuntimeTarget::Ssh { host, user, .. } => format!("SSH ({}@{})", user.as_deref().unwrap_or(""), host),
                    };
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!("⚙️ Aktywne środowisko wykonawcze: [{runtime_desc}]\n\nAby zmienić:\n• `/env host`\n• `/env wsl [distro]`\n• `/env docker <kontener>`\n• `/env ssh <user@host>`"),
                    });
                }
            }
            "/ssh" => {
                if parts.len() > 1 {
                    let target_str = parts[1];
                    let (user, host) = if let Some(idx) = target_str.find('@') {
                        (Some(target_str[..idx].to_string()), target_str[idx + 1..].to_string())
                    } else {
                        (None, target_str.to_string())
                    };
                    let port = parts.get(2).and_then(|p| p.parse::<u16>().ok());
                    let key_path = parts.get(3).map(|k| k.to_string());

                    self.runtime_target = RuntimeTarget::Ssh {
                        host: host.clone(),
                        user: user.clone(),
                        port,
                        key_path,
                    };
                    let u_str = user.unwrap_or_else(|| "default".to_string());
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!("🌐 Połączono środowisko wykonawcze ze zdalnym serwerem Linux SSH: [{u_str}@{host}]"),
                    });
                } else {
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: "Użycie: `/ssh <user@host>` lub `/ssh root@192.168.1.50 22 ~/.ssh/id_rsa`".to_string(),
                    });
                }
            }
            "/docker" => {
                match RuntimeEngine::list_docker_containers() {
                    Ok(list) => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("🐳 Aktywne kontenery Docker:\n```\n{}\n```\nAby przekierować komendy do kontenera: `/env docker <nazwa>`", list),
                        });
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("❌ Błąd Dockera: {e}"),
                        });
                    }
                }
            }
            "/wsl" => {
                match RuntimeEngine::list_wsl_distros() {
                    Ok(list) => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("🐧 Zainstalowane dystrybucje WSL:\n```\n{}\n```\nAby przełączyć: `/env wsl [nazwa_distro]`", list),
                        });
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("❌ Błąd WSL: {e}"),
                        });
                    }
                }
            }
            "/p" | "/palette" => {
                self.show_command_palette = true;
                self.palette_query.clear();
                self.palette_index = 0;
                self.refresh_palette_filter();
            }
            "/files" | "/explorer" | "/fm" | "/mc" => {
                self.file_manager.refresh_all();
                self.show_file_manager = true;
            }
            "/mcp" => {
                let mcp = McpManager::load_from_project_or_global(&self.work_dir);
                let report = mcp.get_status_report();
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: report,
                });
            }
            "/commit" => {
                if parts.len() > 1 {
                    let msg = parts[1..].join(" ");
                    match GitAssistant::commit(&self.work_dir, &msg) {
                        Ok(out) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("✅ Pomyślnie zacommitowano zmiany:\n```\n{}\n```", out),
                            });
                        }
                        Err(e) => {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("❌ Błąd git commit: {e}"),
                            });
                        }
                    }
                } else {
                    // Wygeneruj propozycję commita za pomocą LLM na podstawie diffa
                    if let Ok((status, diff)) = GitAssistant::get_status_and_diff(&self.work_dir) {
                        if status.is_empty() && diff.is_empty() {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: "Brak zmian w repozytorium do zacommitowania (working tree clean).".to_string(),
                            });
                        } else {
                            let prompt = format!(
                                "Na podstawie poniższych zmian w gicie wygeneruj profesjonalny komunikat Conventional Commits (np. feat: ..., fix: ...). Zwróć tylko komunikat commita:\n\nStatus:\n{}\n\nDiff:\n{}",
                                status, diff
                            );
                            self.start_agent_stream(prompt);
                        }
                    }
                }
            }
            "/review" => {
                if let Ok((status, diff)) = GitAssistant::get_status_and_diff(&self.work_dir) {
                    if status.is_empty() && diff.is_empty() {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: "Brak zmian do audytu w repozytorium.".to_string(),
                        });
                    } else {
                        let prompt = format!(
                            "Przeprowadź dokładny przegląd (code review) poniższych zmian w kodzie. Zwróć uwagę na błędy, bezpieczeństwo i sugestie ulepszeń:\n\nStatus:\n{}\n\nDiff:\n{}",
                            status, diff
                        );
                        self.start_agent_stream(prompt);
                    }
                }
            }
            "/undo" => {
                match GitAssistant::undo_uncommitted_changes(&self.work_dir) {
                    Ok(msg) => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("↩️ {msg}"),
                        });
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("❌ Błąd cofania: {e}"),
                        });
                    }
                }
            }
            "/local" => {
                if parts.len() < 2 || parts[1] == "list" {
                    let mut list_msg = String::from("🖥️ **Skonfigurowane Serwery Lokalne i OpenAI-Compatible:**\n");
                    list_msg.push_str(&format!("  • LM Studio: `{}` (Domyślny port 1234)\n", self.config.lmstudio_url.as_deref().unwrap_or("http://localhost:1234/v1")));
                    list_msg.push_str(&format!("  • Llama.cpp: `{}` (Domyślny port 8080)\n", self.config.llamacpp_url.as_deref().unwrap_or("http://localhost:8080/v1")));
                    list_msg.push_str(&format!("  • Ollama:    `{}` (Domyślny port 11434)\n", self.config.ollama_url.as_deref().unwrap_or("http://localhost:11434/v1")));
                    for ep in &self.config.custom_endpoints {
                        list_msg.push_str(&format!("  • `{}`: `{}`\n", ep.id, ep.base_url));
                    }
                    list_msg.push_str("\nAby dodać nowy serwer:\n`/local add <id> <url> [api_key]`\nPrzykład: `/local add vllm http://localhost:8000/v1`");
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: list_msg,
                    });
                    return Ok(());
                }

                if parts[1] == "add" && parts.len() >= 4 {
                    let id = parts[2].to_string();
                    let url = parts[3].to_string();
                    let api_key = parts.get(4).map(|s| s.to_string());

                    self.config.custom_endpoints.retain(|e| e.id != id);
                    self.config.custom_endpoints.push(crate::config::CustomEndpoint {
                        id: id.clone(),
                        name: format!("Custom Server ({})", id),
                        base_url: url.clone(),
                        api_key,
                        default_model: Some("default".to_string()),
                    });
                    let _ = self.config.save();

                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!("✅ Dodano lokalny serwer OpenAI-compatible `{id}` (`{url}`). Modele dostępne jako `{id}/<model>` lub w Selektorze `Ctrl+M`!"),
                    });
                    return Ok(());
                }

                if parts[1] == "remove" && parts.len() >= 3 {
                    let id = parts[2];
                    self.config.custom_endpoints.retain(|e| e.id != id);
                    let _ = self.config.save();
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!("🗑️ Usunięto lokalny serwer `{id}`."),
                    });
                    return Ok(());
                }

                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: "Użycie: `/local list` lub `/local add <id> <url> [api_key]` lub `/local remove <id>`".to_string(),
                });
            }
            "/spawn" | "/agent" => {
                if parts.len() < 2 {
                    self.input_text = "/spawn ".to_string();
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: "👥 Użycie: `/spawn <opis zadania dla podagenta>`\nPrzykład: `/spawn Napisz testy jednostkowe dla modułu parsera`".to_string(),
                    });
                    return Ok(());
                }
                let task_prompt = parts[1..].join(" ");
                let task_id = self.subagent_manager.spawn_task(task_prompt, self.active_model.clone(), self.event_tx.clone());
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!("🚀 Zlecono zadanie dla podagenta `{task_id}` w tle. Wyniki pojawią się w czacie."),
                });
            }
            "/voice" => {
                if let Some(ref cmd) = self.config.voice_plugin_command {
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!("🎙️ Uruchamianie zewnętrznego pluginu głosowego: `{cmd}`..."),
                    });
                } else {
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: "🎙️ **Plugin Głosowy (Voice / STT Hook):**\nAby podpiąć plugin głosowy (np. Whisper), ustaw `voice_plugin_command` w `.opencode/config.json`.\nPrzykład: `\"voice_plugin_command\": \"whisper-cli --record\"`".to_string(),
                    });
                }
            }
            "/search" => {
                if parts.len() < 2 {
                    self.input_text = "/search ".to_string();
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: "🔍 Wpisz zapytanie po `/search ` i naciśnij Enter:".to_string(),
                    });
                    return Ok(());
                }
                let query = parts[1..].join(" ");
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!("🔍 Szukanie w sieci dla: '{}'...", query),
                });
                let app_tx = self.event_tx.clone();
                tokio::spawn(async move {
                    match WebSearch::search(&query).await {
                        Ok(res) => {
                            let _ = app_tx.send(AppEvent::StatusNotification(res)).await;
                        }
                        Err(e) => {
                            let _ = app_tx.send(AppEvent::StatusNotification(format!("❌ Błąd wyszukiwania: {e}"))).await;
                        }
                    }
                });
            }
            "/mode" => {
                if parts.len() > 1 {
                    let m = parts[1].to_lowercase();
                    if ["coder","architect","ask","interactive","auto"].contains(&m.as_str()) {
                        self.agent_mode = m.clone();
                        if m == "auto" { self.config.trust_mode = true; }
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("Ustawiono tryb agenta na: [{}]", self.agent_mode.to_uppercase()),
                        });
                    } else {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: "Dostępne tryby: /mode coder | architect | ask | interactive | auto (Ctrl+T)".to_string(),
                        });
                    }
                } else {
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!("Bieżący tryb: [{}] (Użyj: /mode coder|architect|ask|interactive|auto lub Ctrl+T)", self.agent_mode.to_uppercase()),
                    });
                }
            }
            "/autocheck" => {
                if parts.len() > 1 {
                    self.auto_check = parts[1] == "on" || parts[1] == "true" || parts[1] == "1";
                } else {
                    self.auto_check = !self.auto_check;
                }
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!("Automatyczne sprawdzanie kompilacji (Auto-Check): {}", if self.auto_check { "✅ WŁĄCZONE" } else { "❌ WYŁĄCZONE" }),
                });
            }
            "/export-md" => {
                let default_file = format!("opencode_report_{}.md", Utc::now().format("%Y%m%d_%H%M%S"));
                let path_str = if parts.len() > 1 { parts[1] } else { &default_file };
                let export_path = Path::new(path_str);
                self.save_current_session();
                match TemplateManager::export_to_markdown(&self.current_session, export_path) {
                    Ok(_) => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("✅ Wyeksportowano raport Markdown do: {:?}", export_path),
                        });
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("❌ Błąd eksportu Markdown: {e}"),
                        });
                    }
                }
            }
            "/new" | "/n" => {
                self.start_new_session();
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: "Rozpoczęto nową sesję czatu.".to_string(),
                });
            }
            "/sessions" | "/history" | "/h" => {
                self.refresh_sessions_list();
                self.show_session_picker = true;
            }
            "/load" => {
                if parts.len() > 1 {
                    self.load_session_by_id(parts[1]);
                } else {
                    self.refresh_sessions_list();
                    self.show_session_picker = true;
                }
            }
            "/export" => {
                let default_filename = format!("opencode_export_{}.bundle.json", Utc::now().format("%Y%m%d_%H%M%S"));
                let path_str = if parts.len() > 1 { parts[1] } else { &default_filename };
                let export_path = Path::new(path_str);

                self.save_current_session();
                let mut all_sessions = Vec::new();
                if let Ok(meta_list) = self.session_manager.list_sessions() {
                    for meta in meta_list {
                        if let Ok(s) = self.session_manager.load_session(&meta.id) {
                            all_sessions.push(s);
                        }
                    }
                }

                match SyncEngine::export_bundle(export_path, &all_sessions, true, Some(&self.config)) {
                    Ok(_) => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("✅ Wyeksportowano {} sesji i konfigurację do: {:?}", all_sessions.len(), export_path),
                        });
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("❌ Błąd eksportu: {e}"),
                        });
                    }
                }
            }
            "/import" => {
                if parts.len() < 2 {
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: "Użycie: /import <ścieżka_do_pliku.bundle.json>".to_string(),
                    });
                    return Ok(());
                }

                let import_path = Path::new(parts[1]);
                match SyncEngine::import_bundle(import_path, &self.session_manager, Some(&mut self.config)) {
                    Ok(count) => {
                        self.refresh_sessions_list();
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("✅ Zaimportowano pomyślnie {} sesji z pliku {:?}", count, import_path),
                        });
                    }
                    Err(e) => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("❌ Błąd importu: {e}"),
                        });
                    }
                }
            }
            "/sync" => {
                if parts.len() < 2 {
                    let sync_url = self.config.sync_server_url.as_deref().unwrap_or("[nieustawiony]");
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!("☁️ Ustawienia serwera synchronizacji:\nURL: {sync_url}\nKomendy: /sync push, /sync pull, /sync set <url> [token]"),
                    });
                    return Ok(());
                }

                match parts[1] {
                    "set" => {
                        if parts.len() > 2 {
                            self.config.sync_server_url = Some(parts[2].to_string());
                            if parts.len() > 3 {
                                self.config.sync_token = Some(parts[3].to_string());
                            }
                            self.config.save().ok();
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: format!("✅ Zaktualizowano adres serwera synchronizacji: {}", parts[2]),
                            });
                        }
                    }
                    "push" => {
                        let sync_url = self.config.sync_server_url.clone();
                        let sync_token = self.config.sync_token.clone();
                        if let Some(url) = sync_url {
                            self.save_current_session();
                            let mut all_sessions = Vec::new();
                            if let Ok(meta_list) = self.session_manager.list_sessions() {
                                for meta in meta_list {
                                    if let Ok(s) = self.session_manager.load_session(&meta.id) {
                                        all_sessions.push(s);
                                    }
                                }
                            }
                            let project_name = self.work_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "default".to_string());
                            let app_tx = self.event_tx.clone();

                            tokio::spawn(async move {
                                match SyncEngine::push_to_server(&url, sync_token.as_deref(), &project_name, &all_sessions).await {
                                    Ok(msg) => {
                                        let _ = app_tx.send(AppEvent::StatusNotification(format!("☁️ {msg}"))).await;
                                    }
                                    Err(e) => {
                                        let _ = app_tx.send(AppEvent::StatusNotification(format!("❌ Błąd synchronizacji push: {e}"))).await;
                                    }
                                }
                            });
                        } else {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: "Brak skonfigurowanego serwera synchronizacji. Użyj: /sync set <url> [token]".to_string(),
                            });
                        }
                    }
                    "pull" => {
                        if let Some(ref url) = self.config.sync_server_url {
                            let project_name = self.work_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "default".to_string());
                            let sm = self.session_manager.clone();
                            let app_tx = self.event_tx.clone();
                            let url = url.clone();
                            let token = self.config.sync_token.clone();

                            tokio::spawn(async move {
                                match SyncEngine::pull_from_server(&url, token.as_deref(), &project_name, &sm).await {
                                    Ok(count) => {
                                        let _ = app_tx.send(AppEvent::StatusNotification(format!("☁️ Pomyślnie pobrano {} sesji z serwera.", count))).await;
                                    }
                                    Err(e) => {
                                        let _ = app_tx.send(AppEvent::StatusNotification(format!("❌ Błąd synchronizacji pull: {e}"))).await;
                                    }
                                }
                            });
                        } else {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: "Brak skonfigurowanego serwera synchronizacji. Użyj: /sync set <url> [token]".to_string(),
                            });
                        }
                    }
                    _ => {}
                }
            }
            "/auth" => {
                if parts.len() == 1 {
                    let report = crate::auth::AuthManager::get_auth_status_report(&self.config, &self.work_dir);
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: report,
                    });
                    return Ok(());
                }

                match parts[1] {
                    "export-env" | "export" => {
                        let path_str = if parts.len() > 2 { parts[2] } else { ".env" };
                        let export_path = Path::new(path_str);
                        match crate::auth::AuthManager::export_to_env(export_path, &self.config) {
                            Ok(_) => {
                                self.messages.push(ChatMessage {
                                    role: "system".to_string(),
                                    content: format!("✅ Wyeksportowano plik uwierzytelniania .env do: {:?}\nMożesz teraz skopiować ten 1 plik na inny komputer!", export_path),
                                });
                            }
                            Err(e) => {
                                self.messages.push(ChatMessage {
                                    role: "system".to_string(),
                                    content: format!("❌ Błąd eksportu .env: {e}"),
                                });
                            }
                        }
                    }
                    "import-env" | "import" => {
                        if parts.len() < 3 {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: "Użycie: /auth import-env <ścieżka_do_pliku.env>".to_string(),
                            });
                            return Ok(());
                        }
                        let import_path = Path::new(parts[2]);
                        match crate::auth::AuthManager::import_from_env(import_path, &mut self.config) {
                            Ok(count) => {
                                self.messages.push(ChatMessage {
                                    role: "system".to_string(),
                                    content: format!("✅ Pomyślnie zaimportowano {} kluczy z pliku {:?}", count, import_path),
                                });
                            }
                            Err(e) => {
                                self.messages.push(ChatMessage {
                                    role: "system".to_string(),
                                    content: format!("❌ Błąd importu .env: {e}"),
                                });
                            }
                        }
                    }
                    "set" => {
                        if parts.len() < 4 {
                            self.messages.push(ChatMessage {
                                role: "system".to_string(),
                                content: "Użycie: /auth set <NAZWA_ZMIENNEJ> <WARTOSC>\nPrzykłady: GEMINI_API_KEY, GROQ_API_KEY, OPENAI_API_KEY, CURSOR_AUTH_TOKEN".to_string(),
                            });
                            return Ok(());
                        }
                        let key = parts[2].to_uppercase();
                        let val = parts[3].to_string();
                        std::env::set_var(&key, &val);

                        match key.as_str() {
                            "GEMINI_API_KEY" => self.config.direct_gemini_api_key = Some(val.clone()),
                            "GROQ_API_KEY" => self.config.direct_groq_api_key = Some(val.clone()),
                            "OPENAI_API_KEY" => self.config.direct_openai_api_key = Some(val.clone()),
                            "COMMANDCODE_API_KEY" => self.config.commandcode_api_key = Some(val.clone()),
                            "OPENCODE_SYNC_URL" => self.config.sync_server_url = Some(val.clone()),
                            "OPENCODE_SYNC_TOKEN" => self.config.sync_token = Some(val.clone()),
                            _ => {}
                        }
                        self.config.save().ok();

                        // Zapisz do globalnego pliku .env
                        let global_env = crate::auth::AuthManager::global_env_path();
                        crate::auth::AuthManager::export_to_env(&global_env, &self.config).ok();

                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: format!("✅ Ustawiono i zapisano w .env zmienną: {}", key),
                        });
                    }
                    _ => {
                        self.messages.push(ChatMessage {
                            role: "system".to_string(),
                            content: "Nieznana podkomenda /auth. Użyj: /auth, /auth export-env, /auth import-env, /auth set <KEY> <VAL>".to_string(),
                        });
                    }
                }
            }
            "/opencode" => {
                let report = crate::importer::OpenCodeMigration::inspect_opencode_data(&self.work_dir);
                let imported = crate::importer::OpenCodeMigration::import_credentials(&mut self.config);
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!("{report}\n\nZaimportowano zaktualizowanych kluczy: {imported}"),
                });
            }
            "/model" | "/m" => {
                if parts.len() > 1 {
                    self.active_model = parts[1].to_string();
                    self.messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: format!("Ustawiono operatora na: {}", self.active_model),
                    });
                    self.save_current_session();
                } else {
                    self.show_model_picker = true;
                }
            }
            "/clear" => {
                self.messages.clear();
                self.save_current_session();
            }
            "/config" => {
                let cfg_path = AppConfig::config_path();
                let storage_path = self.session_manager.storage_path();
                let stack = self.agent.context().detect_project_stack();
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!(
                        "⚙️ Konfiguracja OpenCode-RS:\nPlik config: {}\nMagazyn sesji tego projektu: {}\nTryb zapisu: {}\nTryb Agenta: [{}]\nWykryty stos: {}\nAuto-Check: {}\nSerwer sync: {}",
                        cfg_path.display(),
                        storage_path.display(),
                        self.config.storage_mode,
                        self.agent_mode.to_uppercase(),
                        stack,
                        if self.auto_check { "Włączony" } else { "Wyłączony" },
                        self.config.sync_server_url.as_deref().unwrap_or("[nieustawiony]")
                    ),
                });
            }
            "/refactor" => {
                let target = if parts.len() > 1 { parts[1..].join(" ") } else { "cały projekt / kluczowe moduły".to_string() };
                let prompt = format!("Proszę dokonaj profesjonalnej refaktoryzacji kodu (cel: {target}). Popraw czytelność, modułowość, wydajność i usuń powtórzenia kodu. Zastosuj najlepsze wzorce projektowe.");
                self.start_agent_stream(prompt);
            }
            "/tests" | "/test" => {
                let target = if parts.len() > 1 { parts[1..].join(" ") } else { "cały projekt".to_string() };
                let prompt = format!("Napisz i uruchom kompleksowy zestaw testów jednostkowych i integracyjnych dla: {target}. Sprawdź przypadki brzegowe i upewnij się, że testy przechodzą.");
                self.start_agent_stream(prompt);
            }
            "/doc" | "/docs" => {
                let target = if parts.len() > 1 { parts[1..].join(" ") } else { "projekt".to_string() };
                let prompt = format!("Wygeneruj kompletną dokumentację techniczną, komentarze docstrings i przewodnik architektoniczny dla: {target}.");
                self.start_agent_stream(prompt);
            }
            "/explain" => {
                let target = if parts.len() > 1 { parts[1..].join(" ") } else { "cały projekt".to_string() };
                let prompt = format!("Wyjaśnij szczegółowo i przystępnie architekturę, strukturę modułów oraz zasady działania: {target}.");
                self.start_agent_stream(prompt);
            }
            "/fix" => {
                let target = if parts.len() > 1 { parts[1..].join(" ") } else { "bieżące błędy lub ostrzeżenia kompilacji".to_string() };
                let prompt = format!("Przeanalizuj projekt, zdiagnozuj problem ({target}) i zaproponuj gotowe, poprawne rozwiązanie.");
                self.start_agent_stream(prompt);
            }
            "/optimize" => {
                let target = if parts.len() > 1 { parts[1..].join(" ") } else { "kod i alokacje pamięci".to_string() };
                let prompt = format!("Przeprowadź optymalizację wydajnościową dla: {target}. Zoptymalizuj zużycie procesora i pamięci RAM.");
                self.start_agent_stream(prompt);
            }
            "/security" | "/audit" => {
                let prompt = "Przeprowadź dokładny audyt bezpieczeństwa projektu. Sprawdź walidację danych wejściowych, bezpieczną obsługę błędów, brak wycieków danych oraz potencjalne podatności.".to_string();
                self.start_agent_stream(prompt);
            }
            "/grep" => {
                let q = if parts.len() > 1 { parts[1..].join(" ") } else { String::new() };
                if q.is_empty() {
                    self.messages.push(ChatMessage { role: "system".to_string(), content: "Użycie: /grep <fraza> — Live Grep (rg) + preview (Ctrl+F)".to_string() });
                } else {
                    match crate::live_grep::LiveGrep::search(&self.work_dir, &q, 20) {
                        Ok(hits) if hits.is_empty() => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("🔍 Brak wyników dla `{}`", q) }),
                        Ok(hits) => {
                            let mut out = format!("🔍 Live Grep `{}` — {} trafień:\n", q, hits.len());
                            for h in &hits {
                                out.push_str(&format!("  {}:{}: {}\n", h.file.display(), h.line, h.text));
                                out.push_str(&crate::live_grep::LiveGrep::preview(&self.work_dir, h, 1));
                                out.push('\n');
                            }
                            self.messages.push(ChatMessage { role: "system".to_string(), content: out });
                        },
                        Err(e) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("❌ grep: {}", e) }),
                    }
                }
            },
            "/rename" => {
                let pat = if parts.len() > 1 { parts[1..].join(" ") } else { "*_renamed".to_string() };
                match self.file_manager.multi_rename(&pat) {
                    Ok(msg) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("✏️ {}", msg) }),
                    Err(e) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("❌ rename: {}", e) }),
                }
            },
            "/hotlist" => {
                if parts.len() > 1 && parts[1] == "add" {
                    let cur = self.file_manager.active().current_dir.clone();
                    self.file_manager.active_mut().drive_hotlist.push(cur.clone());
                    self.messages.push(ChatMessage { role: "system".to_string(), content: format!("⭐ Dodano do hotlist (Ctrl+D): {}", cur.display()) });
                } else {
                    let list = self.file_manager.active().drive_hotlist.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n");
                    self.messages.push(ChatMessage { role: "system".to_string(), content: if list.is_empty() { "Hotlist pusty — /hotlist add".to_string() } else { format!("⭐ Hotlist:\n{}", list) } });
                }
            },
            "/drive" => {
                let drives = if cfg!(target_os = "windows") {
                    std::process::Command::new("wmic").args(["logicaldisk", "get", "name"]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_else(|_| "C: D:".to_string())
                } else {
                    std::process::Command::new("ls").args(["/"]).output().map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default()
                };
                self.messages.push(ChatMessage { role: "system".to_string(), content: format!("💾 Drive bar (Alt+F1/F2):\n{}", drives) });
            },
            "/lsp" => {
                let mut diag = crate::lsp::LspDiagnostics::new();
                match diag.refresh(&self.work_dir) {
                    Ok(n) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("🔎 LSP cargo check: {} diagnostyk\n{}", n, diag.summary()) }),
                    Err(e) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("❌ LSP: {}", e) }),
                }
            },
            "/taste" => {
                let tm = crate::taste::TasteManager::new(self.work_dir.clone());
                if parts.len() == 1 {
                    self.messages.push(ChatMessage { role: "system".to_string(), content: tm.status_report() });
                } else {
                    match parts[1] {
                        "enable" => {
                            let user = parts.get(2).map(|s| *s == "--user" || *s == "-u").unwrap_or(false);
                            match tm.set_enabled(true, user) {
                                Ok(msg) => self.messages.push(ChatMessage { role: "system".to_string(), content: msg }),
                                Err(e) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("❌ /taste enable: {e}") }),
                            }
                        },
                        "disable" => {
                            let user = parts.get(2).map(|s| *s == "--user" || *s == "-u").unwrap_or(false);
                            match tm.set_enabled(false, user) {
                                Ok(msg) => self.messages.push(ChatMessage { role: "system".to_string(), content: msg }),
                                Err(e) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("❌ /taste disable: {e}") }),
                            }
                        },
                        "push" => {
                            let args: Vec<&str> = parts[1..].to_vec();
                            match tm.exec_npx_taste(&args) {
                                Ok(o) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("🧠 npx taste {}:\n```\n{}\n```", args.join(" "), o) }),
                                Err(e) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("❌ {e}\nTip: npm i -g command-code") }),
                            }
                        },
                        "pull" | "list" | "lint" | "open" => {
                            let args: Vec<&str> = parts[1..].to_vec();
                            match tm.exec_npx_taste(&args) {
                                Ok(o) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("🧠 npx taste {}:\n```\n{}\n```", args.join(" "), o) }),
                                Err(e) => self.messages.push(ChatMessage { role: "system".to_string(), content: format!("❌ {e}") }),
                            }
                        },
                        _ => self.messages.push(ChatMessage { role: "system".to_string(), content: tm.status_report() }),
                    }
                }
            },
            "/skills" => {
                let sm = crate::skills::SkillsManager::new(self.work_dir.clone());
                self.messages.push(ChatMessage { role: "system".to_string(), content: sm.preview_list() });
            },
            "/memories" => {
                let ctx = crate::agent::context::ContextManager::new(self.work_dir.clone()).load_memories_context();
                self.messages.push(ChatMessage { role: "system".to_string(), content: if ctx.is_empty() { "🧠 Brak memories.md — dodaj .windsurf/memories.md lub .trae/memories.md".to_string() } else { ctx } });
            },
            "/build" => {
                let target = if parts.len() > 1 { parts[1..].join(" ") } else { "cały projekt (Trae BUILD solver)".to_string() };
                let prompt = format!("🏗️ Trae BUILD MODE — zbuduj kompletny projekt od 0: {target}. Wygeneruj wszystkie pliki, moduły, testy i dokumentację. Pracuj autonomicznie (auto mode).");
                // przełącz na auto dla BUILD
                self.agent_mode = "auto".to_string();
                self.config.trust_mode = true;
                self.start_agent_stream(prompt);
            },
            "/preview" => {
                // Webview preview placeholder — uruchom vite/next dev jeśli istnieje
                let preview_cmd = if self.work_dir.join("vite.config.ts").exists() || self.work_dir.join("vite.config.js").exists() { "npm run dev -- --host 0.0.0.0 --port 5173" } else if self.work_dir.join("next.config.js").exists() || self.work_dir.join("next.config.ts").exists() { "npm run dev -- -p 3000 -H 0.0.0.0" } else { "python -m http.server 8000" };
                self.messages.push(ChatMessage { role: "system".to_string(), content: format!("🌐 Preview (Trae-like): uruchom `{}` w osobnym terminalu, potem otwórz http://localhost:5173|3000|8000\nTip: użyj `!{}` aby uruchomić w tle", preview_cmd, preview_cmd) });
            },
            "/interactive" => {
                self.agent_mode = "interactive".to_string();
                self.messages.push(ChatMessage { role: "system".to_string(), content: "⚡ Tryb Interactive (Cline-like) — potwierdzenia inline diff przed edit/bash".to_string() });
            },
            "/auto" => {
                self.agent_mode = "auto".to_string();
                self.config.trust_mode = true;
                self.messages.push(ChatMessage { role: "system".to_string(), content: "🤖 Tryb Auto (Unattended) — pełna autonomia, trust_mode=ON".to_string() });
            },


            "/update" => {
                let rt = tokio::runtime::Handle::try_current();
                if let Ok(h) = rt {
                    let tx = self.event_tx.clone();
                    h.spawn(async move {
                        match crate::update::Updater::check_for_update().await {
                            Ok(Some((tag, _url))) => { let _ = tx.send(AppEvent::StatusNotification(format!("⬆️ Aktualizacja dostępna: {} → {}", crate::update::Updater::current_version(), tag))).await; },
                            Ok(None) => { let _ = tx.send(AppEvent::StatusNotification("✅ Brak aktualizacji".to_string())).await; },
                            Err(e) => { let _ = tx.send(AppEvent::StatusNotification(format!("❌ update check: {}", e))).await; },
                        }
                    });
                    self.messages.push(ChatMessage { role: "system".to_string(), content: "🔄 Sprawdzam aktualizacje...".to_string() });
                } else {
                    self.messages.push(ChatMessage { role: "system".to_string(), content: format!("Aktualna wersja: v{} — /update wymaga tokio", env!("CARGO_PKG_VERSION")) });
                }
            },
            "/hermes" => {
                if parts.len() > 1 && parts[1] == "add" {
                    let prompt = if parts.len() > 3 { parts[3..].join(" ") } else { "hermes task".to_string() };
                    let schedule = if parts.len() > 2 { Some(parts[2].to_string()) } else { None };
                    let runtime = parts.iter().find(|p| p.starts_with("--runtime")).map(|p| p.trim_start_matches("--runtime=").to_string()).unwrap_or_else(|| "host".to_string());
                    let daemon = std::sync::Arc::new(crate::hermes::HermesDaemon::new(self.work_dir.clone(), self.config.clone()));
                    let rt = tokio::runtime::Handle::try_current();
                    if let Ok(h) = rt {
                        let d = daemon.clone();
                        let sch = schedule.clone();
                        h.spawn(async move {
                            let _ = d.add_job(prompt, sch, runtime, None).await;
                        });
                    }
                    self.messages.push(ChatMessage { role: "system".to_string(), content: format!("⏰ Hermes cron dodany: schedule={:?}", schedule) });
                } else {
                    self.messages.push(ChatMessage { role: "system".to_string(), content: "Hermes: /hermes add \"<cron|*>\" \"<prompt>\" [--runtime host|docker:ci|ssh:user@host|wsl:Ubuntu] | /hermes list".to_string() });
                }
            },
            "/help" => {
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: "📖 Dostępne komendy i skróty klawiszowe:\n\
• [Ctrl+P] lub /palette - Interaktywna Paleta Komend (Raycast / VS Code style)\n\
• [Ctrl+E] lub /mc - Eksplorator plików Midnight Commander (Spacja wkleja @plik do promptu)\n\
• [Ctrl+T] lub /mode <coder|architect|ask> - Zmiana trybu pracy agenta\n\
• [Ctrl+H] lub /history - Historia sesji czatu projektu\n\
• [Ctrl+N] lub /new - Nowa sesja czatu\n\
• [Ctrl+M] lub /model - Wybór operatora / modelu AI\n\
• @plik.rs / @git / @tree - Inteligentne wstrzykiwanie plików/gita do promptu\n\
• /commit [komunikat|auto] - Asystent Git commit (automatyczny komunikat przez AI)\n\
• /pr [tytuł] - Utwórz PR via gh\n\
• /grep <fraza> - Live Grep (rg) + preview (Ctrl+F)\n\
• /lsp - Diagnostyka cargo check (LSP)\n\
• /rename <wzorzec> - Multi-rename (Ctrl+M w files) wzorzec *\n\
• /hotlist [add] - Ulubione ścieżki (Ctrl+D)\n\
• /drive - Drive bar (Alt+F1/F2)\n\
• /hermes add \"<cron>\" \"<prompt>\" [--runtime ...] - Cron daemon uniwersalny\n\
• /mcp [install <pkg>] - MCP serwery\n\
• /update - Sprawdź aktualizacje GitHub\n\
• /review - Inteligentny audyt bezpieczeństwa i jakości kodu\n\
• /undo - Bezpieczne cofanie ostatnich zmian w repozytorium\n\
• /search <zapytanie> - Wyszukiwarka dokumentacji w internecie\n\
• /mcp - Podgląd zarejestrowanych serwerów Model Context Protocol\n\
• /export-md [plik.md] - Eksport rozmowy do czytelnego raportu Markdown\n\
• /refactor, /tests, /doc, /explain - Wbudowane szablony zadań\n\
• /auth (export-env | import-env | set) - Zarządzanie 1 plikiem .env\n\
• /opencode - Diagnostyka danych oryginalnego OpenCode\n\
• /autocheck [on|off] - Automatyczne sprawdzanie kompilacji projektu\n\
• [Ctrl+L] lub /clear - Wyczyść czat | [Ctrl+C] - Zapis i wyjście".to_string(),
                });
            }
            _ => {
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!("Nieznana komenda: {}. Wpisz /help lub naciśnij [Ctrl+P] aby otworzyć Paletę Komend.", parts[0]),
                });
            }
        }

        Ok(())
    }

    pub fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Token(tok) => {
                self.streaming_buffer.push_str(&tok);
            }
            AppEvent::StreamFinished(_used_model) => {
                self.is_streaming = false;
                if !self.streaming_buffer.is_empty() {
                    self.messages.push(ChatMessage {
                        role: "assistant".to_string(),
                        content: self.streaming_buffer.clone(),
                    });
                    self.streaming_buffer.clear();
                    self.save_current_session();
                    self.refresh_sessions_list();
                }
            }
            AppEvent::StreamError(err) => {
                self.is_streaming = false;
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: format!("❌ Błąd: {err}"),
                });
                self.streaming_buffer.clear();
                self.save_current_session();
            }
            AppEvent::StatusNotification(msg) => {
                self.messages.push(ChatMessage {
                    role: "system".to_string(),
                    content: msg,
                });
                self.refresh_sessions_list();
            }
            AppEvent::ModelsDiscovered(models) => {
                if !models.is_empty() {
                    self.dynamic_models = models;
                }
            }
        }
    }
}
