use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::app::App;
use crate::providers::ProviderRouter;
use crate::syntax::SyntaxHighlighter;
use crate::theme::AppTheme;

pub fn render(f: &mut Frame, app: &mut App) {
    let size = f.area();

    // Główny podział pionowy: Główny obszar roboczy + Footer z pigułkami skrótów
    let root_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),   // Obszar roboczy
            Constraint::Length(1), // Pasek skrótów (Pill badges)
        ])
        .split(size);

    // Podział poziomy: Tryb IDE (3 kolumny) vs Tryb Klasyczny (Czat + ew. Sidebar)
    if app.ide_mode {
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(22), // Eksplorator plików
                Constraint::Percentage(48), // Edytor kodu (VS Code Dark+)
                Constraint::Percentage(30), // Czat AI i input
            ])
            .split(root_chunks[0]);

        render_ide_file_explorer(f, h_chunks[0], app);
        render_ide_editor(f, h_chunks[1], app);
        render_chat_column(f, h_chunks[2], app);
    } else if app.show_sidebar {
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(72), // Czat i Input
                Constraint::Percentage(28), // Sidebar
            ])
            .split(root_chunks[0]);

        render_chat_column(f, h_chunks[0], app);
        render_sidebar(f, h_chunks[1], app);
    } else {
        render_chat_column(f, root_chunks[0], app);
    }

    render_footer(f, root_chunks[1], app);

    // Modale
    if app.show_command_palette {
        render_command_palette(f, size, app);
    }
    if app.show_theme_picker {
        render_theme_picker(f, size, app);
    }
    if app.show_model_picker {
        render_model_picker(f, size, app);
    }
    if app.show_session_picker {
        render_session_picker(f, size, app);
    }
    if app.show_file_manager {
        render_file_manager(f, size, app);
    }
    if app.permission_dialog.is_some() {
        render_permission_dialog(f, size, app);
    }
}

#[allow(dead_code)]
fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.current_theme;
    let proj_name = app.work_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "project".to_string());
    let current_branch = crate::environment::EnvironmentManager::get_current_branch(&app.work_dir).unwrap_or_else(|_| "main".to_string());

    // Lewa strona — brand + projekt + branch + tryb + model
    let left_line = Line::from(vec![
        Span::styled(" ● ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled("opencode", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("-rs ", Style::default().fg(theme.secondary)),
        Span::styled(format!(" 📁 {} ", proj_name), Style::default().fg(theme.text_muted)),
        Span::styled(format!(" 🌿 {} ", current_branch), Style::default().fg(theme.assistant)),
        Span::styled(format!(" 󰘐 [{}] ", app.agent_mode.to_uppercase()), Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" ◈ {} ", app.active_model), Style::default().fg(theme.secondary)),
    ]);

    // Prawa strona — pełna ścieżka katalogu (żeby nie zgubić się w projektach)
    let full_path = app.work_dir.display().to_string();
    // Skróć jeśli za długa — pokaż ostatnie 3 segmenty
    let path_display = if full_path.chars().count() > 60 {
        let segments: Vec<&str> = full_path.split(['\\', '/']).filter(|s| !s.is_empty()).collect();
        if segments.len() > 3 {
            format!("...\\{}", segments[segments.len()-3..].join("\\"))
        } else {
            full_path
        }
    } else {
        full_path
    };
    let right_line = Line::from(vec![
        Span::styled(" 📂 ", Style::default().fg(theme.text_muted)),
        Span::styled(path_display, Style::default().fg(theme.text_muted).add_modifier(Modifier::DIM)),
        Span::styled(" ", Style::default()),
    ]);

    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(20), Constraint::Length(70)])
        .split(area);

    let header_widget = Paragraph::new(left_line).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme.border)),
    );
    let right_widget = Paragraph::new(right_line)
        .alignment(Alignment::Right)
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme.border)),
        );

    f.render_widget(header_widget, header_chunks[0]);
    f.render_widget(right_widget, header_chunks[1]);
}

fn render_chat_column(f: &mut Frame, area: Rect, app: &App) {
    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),    // Czat
            Constraint::Length(3), // Czysty, nowoczesny input dock
        ])
        .split(area);

    render_chat_stream(f, v_chunks[0], app);
    render_input_box(f, v_chunks[1], app);
}

fn render_sidebar(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.current_theme;
    let proj_name = app.work_dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "projekt".to_string());
    let current_branch = crate::environment::EnvironmentManager::get_current_branch(&app.work_dir).unwrap_or_else(|_| "main".to_string());
    let (est_tokens, max_tokens, percent, cost_str) = app.estimate_tokens_and_cost();
    let stack = app.agent.context().detect_project_stack();
    let runtime_label = match &app.runtime_target {
        crate::runtime::RuntimeTarget::Host => "Host (Local)".to_string(),
        crate::runtime::RuntimeTarget::Wsl { distro: _ } => "WSL (Linux)".to_string(),
        crate::runtime::RuntimeTarget::Docker { container: _ } => "Docker".to_string(),
        crate::runtime::RuntimeTarget::Ssh { host, user, .. } => {
            if let Some(u) = user {
                format!("SSH ({u}@{host})")
            } else {
                format!("SSH ({host})")
            }
        }
    };

    let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let spinner_icon = spinner_frames[app.spinner_frame % spinner_frames.len()];

    let status_color = if app.is_streaming { theme.assistant } else { theme.success };
    let status_text = if app.is_streaming {
        format!("{} {}", spinner_icon, app.current_lang.status_generating())
    } else {
        app.current_lang.status_ready().to_string()
    };

    let mut lines = Vec::new();

    // ── KARTA 1: AGENT & STATUS ──
    lines.push(Line::from(vec![
        Span::styled("╭─ ◈ AGENT & RUNTIME ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled("──────────────╮", Style::default().fg(theme.border)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(theme.border)),
        Span::styled("Model:   ", Style::default().fg(theme.text_muted)),
        Span::styled(&app.active_model, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(theme.border)),
        Span::styled("Status:  ", Style::default().fg(theme.text_muted)),
        Span::styled(status_text, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(theme.border)),
        Span::styled("Runtime: ", Style::default().fg(theme.text_muted)),
        Span::styled(runtime_label, Style::default().fg(theme.accent)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("╰──────────────────────────────────────╯", Style::default().fg(theme.border)),
    ]));
    lines.push(Line::from(""));

    // ── KARTA 2: KONTEKST & ZUŻYCIE ──
    let bar_len: usize = 16;
    let filled_len = ((percent as f32 / 100.0) * bar_len as f32).round() as usize;
    let empty_len = bar_len.saturating_sub(filled_len);
    let bar_filled: String = "█".repeat(filled_len);
    let bar_empty: String = "░".repeat(empty_len);

    let bar_color = if percent < 50 {
        theme.success
    } else if percent < 80 {
        theme.warning
    } else {
        theme.error
    };

    lines.push(Line::from(vec![
        Span::styled("╭─ 📊 CONTEXT & USAGE ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled("─────────────╮", Style::default().fg(theme.border)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(theme.border)),
        Span::styled("Tokens:  ", Style::default().fg(theme.text_muted)),
        Span::styled(format!("{est_tokens}/{max_tokens} "), Style::default().fg(Color::White)),
        Span::styled(format!("({percent}%)"), Style::default().fg(bar_color).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(theme.border)),
        Span::styled("Usage:   ", Style::default().fg(theme.text_muted)),
        Span::styled("[", Style::default().fg(theme.border)),
        Span::styled(bar_filled, Style::default().fg(bar_color)),
        Span::styled(bar_empty, Style::default().fg(theme.border)),
        Span::styled("]", Style::default().fg(theme.border)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(theme.border)),
        Span::styled("Cost:    ", Style::default().fg(theme.text_muted)),
        Span::styled(cost_str, Style::default().fg(theme.system).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("╰──────────────────────────────────────╯", Style::default().fg(theme.border)),
    ]));
    lines.push(Line::from(""));

    // ── KARTA 3: REPOZYTORIUM & DUAL SYNC ──
    let sync_mode_badge = match app.config.git_sync_mode.as_str() {
        "both" => "[DUAL SYNC (BOTH)]",
        "public" => "[PUBLIC ONLY]",
        "private" => "[PRIVATE ONLY]",
        _ => "[DUAL SYNC]",
    };

    lines.push(Line::from(vec![
        Span::styled("╭─ 🌐 WORKSPACE & GIT ", Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD)),
        Span::styled("─────────────╮", Style::default().fg(theme.border)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(theme.border)),
        Span::styled("Folder:  ", Style::default().fg(theme.text_muted)),
        Span::styled(proj_name, Style::default().fg(Color::White)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(theme.border)),
        Span::styled("Branch:  ", Style::default().fg(theme.text_muted)),
        Span::styled(current_branch, Style::default().fg(theme.assistant)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(theme.border)),
        Span::styled("Git:     ", Style::default().fg(theme.text_muted)),
        Span::styled(sync_mode_badge, Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(theme.border)),
        Span::styled("Stack:   ", Style::default().fg(theme.text_muted)),
        Span::styled(stack, Style::default().fg(theme.system)),
    ]));
    lines.push(Line::from(vec![
        Span::styled("╰──────────────────────────────────────╯", Style::default().fg(theme.border)),
    ]));

    let sidebar_widget = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(theme.border)),
    );

    f.render_widget(sidebar_widget, area);
}

fn render_chat_stream(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.current_theme;
    let mut lines = Vec::new();

    if app.messages.is_empty() && (!app.is_streaming || app.streaming_buffer.is_empty()) {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  ╭─ ⚡ OPENCODE-RS ────────────────────────────────────────────────╮", Style::default().fg(theme.border)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  │  ", Style::default().fg(theme.border)),
            Span::styled(app.current_lang.welcome_msg(), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  │  ", Style::default().fg(theme.border)),
            Span::styled(app.current_lang.welcome_hint(), Style::default().fg(theme.text_muted)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  ╰─────────────────────────────────────────────────────────────────╯", Style::default().fg(theme.border)),
        ]));
        lines.push(Line::from(""));
    }

    for msg in &app.messages {
        if msg.role == "user" {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  ╭─ ● You ", Style::default().fg(theme.user).add_modifier(Modifier::BOLD)),
                Span::styled("────────────────────────────────────────────────────────╮", Style::default().fg(theme.border)),
            ]));
            for l in msg.content.lines() {
                lines.push(Line::from(vec![
                    Span::styled("  │ ", Style::default().fg(theme.border)),
                    Span::styled(l, Style::default().fg(Color::White)),
                ]));
            }
            lines.push(Line::from(vec![
                Span::styled("  ╰─────────────────────────────────────────────────────────────────╯", Style::default().fg(theme.border)),
            ]));
        } else if msg.role == "assistant" {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  ╭─ ◈ OpenCode ", Style::default().fg(theme.assistant).add_modifier(Modifier::BOLD)),
                Span::styled(format!("── {} ", app.active_model), Style::default().fg(theme.text_muted)),
                Span::styled("──────────────────────────────────╮", Style::default().fg(theme.border)),
            ]));

            let mut in_code_block = false;
            let mut code_lang = String::new();

            for l in msg.content.lines() {
                if l.starts_with("```") {
                    in_code_block = !in_code_block;
                    if in_code_block {
                        code_lang = l.trim_start_matches('`').trim().to_string();
                        let lang_tag = if code_lang.is_empty() { "code".to_string() } else { code_lang.clone() };
                        lines.push(Line::from(vec![
                            Span::styled("  │ ", Style::default().fg(theme.border)),
                            Span::styled(format!("╭─ 󰘐 {} ", lang_tag), Style::default().fg(theme.accent)),
                            Span::styled("────────────────────────╮", Style::default().fg(theme.border)),
                        ]));
                    } else {
                        lines.push(Line::from(vec![
                            Span::styled("  │ ", Style::default().fg(theme.border)),
                            Span::styled("╰────────────────────────────────────╯", Style::default().fg(theme.border)),
                        ]));
                    }
                } else if in_code_block {
                    let hl = SyntaxHighlighter::highlight_code_line(l, &code_lang);
                    let mut prefixed_spans = vec![
                        Span::styled("  │ ", Style::default().fg(theme.border)),
                        Span::styled("│ ", Style::default().fg(theme.border)),
                    ];
                    prefixed_spans.extend(hl.spans);
                    lines.push(Line::from(prefixed_spans));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled("  │ ", Style::default().fg(theme.border)),
                        Span::styled(l, Style::default().fg(Color::Rgb(220, 225, 235))),
                    ]));
                }
            }
            lines.push(Line::from(vec![
                Span::styled("  ╰─────────────────────────────────────────────────────────────────╯", Style::default().fg(theme.border)),
            ]));
        } else if msg.role == "system" {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  ⚙ ", Style::default().fg(theme.system)),
                Span::styled(&msg.content, Style::default().fg(theme.system)),
            ]));
        }
    }

    if app.is_streaming && !app.streaming_buffer.is_empty() {
        let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let spinner_icon = spinner_frames[app.spinner_frame % spinner_frames.len()];

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(format!("  ╭─ {} Generating response... ", spinner_icon), Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
            Span::styled("────────────────────────────────────────╮", Style::default().fg(theme.border)),
        ]));
        for l in app.streaming_buffer.lines() {
            lines.push(Line::from(vec![
                Span::styled("  │ ", Style::default().fg(theme.border)),
                Span::styled(l, Style::default().fg(Color::Rgb(220, 225, 235))),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled("  ╰─────────────────────────────────────────────────────────────────╯", Style::default().fg(theme.border)),
        ]));
    }

    let chat_widget = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset, 0));

    f.render_widget(chat_widget, area);
}

fn render_input_box(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.current_theme;
    let input_placeholder = if app.input_text.is_empty() {
        app.current_lang.input_placeholder()
    } else {
        &app.input_text
    };

    let input_style = if app.input_text.is_empty() {
        Style::default().fg(theme.text_muted)
    } else {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    };

    let mode_label = format!(" [{}] ", app.agent_mode.to_uppercase());
    let input_lines = vec![
        Line::from(vec![
            Span::styled(" ❯ ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
            Span::styled(input_placeholder, input_style),
            Span::styled("▌", Style::default().fg(theme.primary)),
        ]),
    ];

    let input_widget = Paragraph::new(input_lines).block(
        Block::default()
            .title(Span::styled(mode_label, Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(if app.is_streaming {
                Style::default().fg(theme.border)
            } else {
                Style::default().fg(theme.border_active)
            }),
    );

    f.render_widget(input_widget, area);
}

fn render_footer(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.current_theme;
    let version = env!("CARGO_PKG_VERSION");
    
    // Zunifikowany pasek skrótów (jeden pasek na dole ekranu)
    let shortcuts = Line::from(vec![
        Span::styled("[^T] ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled("Tryb  ", Style::default().fg(Color::Rgb(156, 163, 175))),
        Span::styled("[^P] ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled("Komendy  ", Style::default().fg(Color::Rgb(156, 163, 175))),
        Span::styled("[^M] ", Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD)),
        Span::styled("Modele  ", Style::default().fg(Color::Rgb(156, 163, 175))),
        Span::styled("[^E] ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled("Pliki  ", Style::default().fg(Color::Rgb(156, 163, 175))),
        Span::styled("[^H] ", Style::default().fg(theme.system).add_modifier(Modifier::BOLD)),
        Span::styled("Sesje  ", Style::default().fg(Color::Rgb(156, 163, 175))),
        Span::styled("[^K] ", Style::default().fg(Color::Rgb(168, 85, 247)).add_modifier(Modifier::BOLD)),
        Span::styled("Motyw  ", Style::default().fg(Color::Rgb(156, 163, 175))),
        Span::styled("[^B] ", Style::default().fg(Color::Rgb(52, 211, 153)).add_modifier(Modifier::BOLD)),
        Span::styled("Sidebar  ", Style::default().fg(Color::Rgb(156, 163, 175))),
        Span::styled("[F3] ", Style::default().fg(Color::Rgb(250, 204, 21)).add_modifier(Modifier::BOLD)),
        Span::styled(if app.ide_mode { "Classic  " } else { "IDE  " }, Style::default().fg(Color::Rgb(156, 163, 175))),
        Span::styled("[^C] ", Style::default().fg(Color::Rgb(248, 113, 113)).add_modifier(Modifier::BOLD)),
        Span::styled("Wyjście", Style::default().fg(Color::Rgb(156, 163, 175))),
    ]);

    // Prawy dół — podpis DevFort.pl + wersja OpenCode kompatybilności (auto z Cargo.toml)
    let branding = Line::from(vec![
        Span::styled(" DevFort.pl ", Style::default().fg(Color::Rgb(56, 189, 248)).add_modifier(Modifier::BOLD)),
        Span::styled("· OpenCode-RS ", Style::default().fg(Color::Rgb(156, 163, 175))),
        Span::styled(format!("v{} ", version), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(format!("(OpenCode {}) ", version), Style::default().fg(Color::Rgb(100, 116, 139))),
    ]);

    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([ratatui::layout::Constraint::Min(0), ratatui::layout::Constraint::Length(38)])
        .split(area);
    f.render_widget(Paragraph::new(shortcuts).alignment(ratatui::layout::Alignment::Left), chunks[0]);
    f.render_widget(Paragraph::new(branding).alignment(ratatui::layout::Alignment::Right), chunks[1]);
}

fn render_theme_picker(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.current_theme;
    let popup_area = centered_rect(65, 55, area);
    f.render_widget(Clear, popup_area);

    let themes = AppTheme::list_all();
    let visible = (popup_area.height.saturating_sub(2)) as usize;
    let len = themes.len();
    let effective_idx = app.theme_picker_index.min(len.saturating_sub(1));
    let offset = if len > visible && visible > 0 && effective_idx >= visible {
        let mut off = effective_idx - visible + 1;
        if off + visible > len {
            off = len.saturating_sub(visible);
        }
        off
    } else {
        0
    };
    let items: Vec<ListItem> = themes
        .iter()
        .skip(offset)
        .take(visible.max(1))
        .enumerate()
        .map(|(i, (id, name, desc))| {
            let original_idx = offset + i;
            let is_selected = original_idx == effective_idx;
            let is_current = id == &app.config.theme;

            let style = if is_selected {
                Style::default().fg(Color::Black).bg(theme.primary).add_modifier(Modifier::BOLD)
            } else if is_current {
                Style::default().fg(theme.assistant).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let prefix = if is_selected {
                "▶ "
            } else if is_current {
                "● "
            } else {
                "  "
            };

            let line = Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("{:<14} ", id), style),
                Span::styled(format!("{:<30} ", name), if is_selected { style } else { Style::default().fg(theme.accent) }),
                Span::styled(format!("- {}", desc), if is_selected { style } else { Style::default().fg(theme.text_muted) }),
            ]);
            ListItem::new(line)
        })
        .collect();

    let title = if len > visible && visible > 0 {
        format!(" 🎨 Wybierz Motyw (42) [{}-{}/{}] ", offset + 1, (offset + visible).min(len), len)
    } else {
        " 🎨 Wybierz Motyw / Schemat Kolorów (OLED Pure Black / Graphite / OpenCode) ".to_string()
    };
    let list = List::new(items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.primary)),
    );

    f.render_widget(list, popup_area);
}

fn render_command_palette(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.current_theme;
    let popup_area = centered_rect(76, 62, area);
    f.render_widget(Clear, popup_area);

    let popup_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Wyszukiwarka Spotlight
            Constraint::Min(4),    // Lista komend
            Constraint::Length(1), // Pasek skrótów nawigacji
        ])
        .split(popup_area);

    // ── GÓRNY PASEK WYSZUKIWANIA SPOTLIGHT ──
    let placeholder = if app.palette_query.is_empty() {
        "Wpisz nazwę komendy (np. branch, commit, refactor, wsl, model, theme)..."
    } else {
        &app.palette_query
    };

    let search_line = Line::from(vec![
        Span::styled(" ❯ ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled(
            placeholder,
            if app.palette_query.is_empty() {
                Style::default().fg(theme.text_muted)
            } else {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            },
        ),
        Span::styled("▌", Style::default().fg(theme.primary)),
    ]);

    let search_widget = Paragraph::new(search_line).block(
        Block::default()
            .title(" ⚡ Menu Komend & Narzędzi (Wpisz / lub szukaj) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_active)),
    );
    f.render_widget(search_widget, popup_chunks[0]);

    // ── LISTA KOMEND Z ELEGANCKIMI IKONAMI I TAGAMI ──
    let items: Vec<ListItem> = if app.filtered_palette.is_empty() {
        vec![ListItem::new(Line::from(vec![
            Span::styled("   Brak pasujących komend dla tego zapytania", Style::default().fg(theme.text_muted)),
        ]))]
    } else {
        app.filtered_palette
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let is_selected = i == app.palette_index;

                let icon = match item.command {
                    "/commit" => "💾 ",
                    "/review" => "🔍 ",
                    "/undo" => "↩️ ",
                    "/refactor" => "⚡ ",
                    "/tests" => "🧪 ",
                    "/doc" => "📖 ",
                    "/explain" => "💡 ",
                    "/search" => "🌐 ",
                    "/wsl" => "🐧 ",
                    "/docker" => "🐳 ",
                    "/local" => "🖥️ ",
                    "/branch" => "🌿 ",
                    "/target" => "🎯 ",
                    "/env" => "⚙️ ",
                    "/files" => "📁 ",
                    "/theme" => "🎨 ",
                    "/model" => "◈ ",
                    "/sessions" | "/history" => "📂 ",
                    "/vault" => "🔒 ",
                    "/new" => "✨ ",
                    "/mode" => "🔄 ",
                    "/autocheck" => "🛡️ ",
                    "/ssh" => "🔑 ",
                    "/export-md" => "📑 ",
                    _ => "🔹 ",
                };

                let tag_str = if !item.shortcut.is_empty() {
                    item.shortcut.to_string()
                } else {
                    item.category.replace("🛠️ ", "").replace("⚙️ ", "").replace("⚡ ", "").replace("📁 ", "").replace("🌐 ", "")
                };

                let line = if is_selected {
                    Line::from(vec![
                        Span::styled(" ▶ ", Style::default().fg(Color::Black).bg(theme.primary).add_modifier(Modifier::BOLD)),
                        Span::styled(icon, Style::default().fg(Color::Black).bg(theme.primary)),
                        Span::styled(format!("{:<14}", item.command), Style::default().fg(Color::Black).bg(theme.primary).add_modifier(Modifier::BOLD)),
                        Span::styled(format!("{:<42}", item.description), Style::default().fg(Color::Black).bg(theme.primary)),
                        Span::styled(format!(" [{:>8}] ", tag_str), Style::default().fg(Color::Black).bg(theme.primary).add_modifier(Modifier::BOLD)),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled("   ", Style::default()),
                        Span::styled(icon, Style::default().fg(theme.primary)),
                        Span::styled(format!("{:<14}", item.command), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
                        Span::styled(format!("{:<42}", item.description), Style::default().fg(Color::Rgb(215, 220, 230))),
                        Span::styled(format!(" [{:>8}] ", tag_str), Style::default().fg(theme.accent)),
                    ])
                };

                ListItem::new(line)
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border)),
    );
    f.render_widget(list, popup_chunks[1]);

    // ── DOLNY PASEK PODPOWIEDZI ──
    let hint_line = Line::from(vec![
        Span::styled("  [↑/↓] ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled("Nawigacja  ", Style::default().fg(theme.text_muted)),
        Span::styled("[Enter] ", Style::default().fg(theme.assistant).add_modifier(Modifier::BOLD)),
        Span::styled("Wykonaj Komendę Natychmiast  ", Style::default().fg(theme.text_muted)),
        Span::styled("[Esc] ", Style::default().fg(theme.error).add_modifier(Modifier::BOLD)),
        Span::styled("Zamknij", Style::default().fg(theme.text_muted)),
    ]);
    let hint_widget = Paragraph::new(hint_line);
    f.render_widget(hint_widget, popup_chunks[2]);
}

fn render_file_manager(f: &mut Frame, area: Rect, app: &App) {
    use crate::file_manager::ActivePane;

    let theme = &app.current_theme;
    let popup_area = centered_rect(92, 82, area);
    f.render_widget(Clear, popup_area);

    let popup_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),    // 2 Panele
            Constraint::Length(1), // Pasek funkcyjny
        ])
        .split(popup_area);

    let pane_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Lewy Panel
            Constraint::Percentage(50), // Prawy Panel
        ])
        .split(popup_chunks[0]);

    // ── LEWY PANEL ──
    let is_left_active = app.file_manager.active_pane == ActivePane::Left;
    render_pane(f, pane_chunks[0], &app.file_manager.left, is_left_active, "Left Pane", theme);

    // ── PRAWY PANEL ──
    let is_right_active = app.file_manager.active_pane == ActivePane::Right;
    render_pane(f, pane_chunks[1], &app.file_manager.right, is_right_active, "Right Pane", theme);

    // Pasek funkcyjny na dole
    let footer_line = Line::from(vec![
        Span::styled(" [Tab/o/]/[] ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled("Switch Pane  ", Style::default().fg(theme.text_muted)),
        Span::styled("[Space] ", Style::default().fg(theme.assistant).add_modifier(Modifier::BOLD)),
        Span::styled("Paste @file  ", Style::default().fg(theme.text_muted)),
        Span::styled("[t] ", Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD)),
        Span::styled("Tree/List  ", Style::default().fg(theme.text_muted)),
        Span::styled("[F5/c] ", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled("Copy  ", Style::default().fg(theme.text_muted)),
        Span::styled("[F6/m] ", Style::default().fg(theme.system).add_modifier(Modifier::BOLD)),
        Span::styled("Move  ", Style::default().fg(theme.text_muted)),
        Span::styled("[F8/d] ", Style::default().fg(theme.error).add_modifier(Modifier::BOLD)),
        Span::styled("Delete  ", Style::default().fg(theme.text_muted)),
        Span::styled("[Esc] ", Style::default().fg(theme.text_muted).add_modifier(Modifier::BOLD)),
        Span::styled("Close", Style::default().fg(theme.text_muted)),
    ]);
    let footer_widget = Paragraph::new(footer_line).alignment(Alignment::Center);
    f.render_widget(footer_widget, popup_chunks[1]);
}

fn render_pane(
    f: &mut Frame,
    area: Rect,
    pane: &crate::file_manager::PaneState,
    is_active: bool,
    pane_label: &str,
    theme: &AppTheme,
) {
    let dir_display = pane.current_dir.display().to_string();
    let mode_tag = match pane.view_mode {
        crate::file_manager::FileViewMode::List => "[List]",
        crate::file_manager::FileViewMode::Tree => "[Tree]",
    };

    let title = if is_active {
        format!(" ● [{}] {} 📁 {} ", pane_label, mode_tag, dir_display)
    } else {
        format!(" ○ [{}] {} 📁 {} ", pane_label, mode_tag, dir_display)
    };

    let border_color = if is_active { theme.primary } else { theme.border };

    let items: Vec<ListItem> = match pane.view_mode {
        crate::file_manager::FileViewMode::Tree => {
            if pane.tree_entries.is_empty() {
                vec![ListItem::new(Line::from(Span::styled("  (brak elementów w drzewie)", Style::default().fg(theme.text_muted))))]
            } else {
                pane.tree_entries
                    .iter()
                    .enumerate()
                    .map(|(i, entry)| {
                        let is_selected = is_active && (i == pane.selected_index);
                        let indent = "  ".repeat(entry.depth);
                        let branch_prefix = if entry.depth > 0 { "├── " } else { "" };

                        let (icon, icon_color) = if entry.is_dir {
                            ("📁 ", theme.primary)
                        } else {
                            ("📄 ", Color::White)
                        };

                        let style = if is_selected {
                            Style::default().fg(Color::Black).bg(theme.primary).add_modifier(Modifier::BOLD)
                        } else if entry.is_dir {
                            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::White)
                        };

                        let prefix = if is_selected { "▶ " } else { "  " };
                        let line = if is_selected {
                            Line::from(vec![
                                Span::styled(prefix, style),
                                Span::styled(format!("{}{}{}{}", indent, branch_prefix, icon, entry.name), style),
                            ])
                        } else {
                            Line::from(vec![
                                Span::styled(prefix, Style::default().fg(theme.text_muted)),
                                Span::styled(indent, Style::default().fg(theme.text_muted)),
                                Span::styled(branch_prefix, Style::default().fg(theme.text_muted)),
                                Span::styled(icon, Style::default().fg(icon_color)),
                                Span::styled(&entry.name, style),
                            ])
                        };

                        ListItem::new(line)
                    })
                    .collect()
            }
        }
        crate::file_manager::FileViewMode::List => {
            if pane.items.is_empty() {
                vec![ListItem::new(Line::from(Span::styled("  (pusty katalog)", Style::default().fg(theme.text_muted))))]
            } else {
                pane.items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let is_selected = is_active && (i == pane.selected_index);
                        let (icon, icon_color) = get_file_icon(item);

                        let style = if is_selected {
                            Style::default().fg(Color::Black).bg(theme.primary).add_modifier(Modifier::BOLD)
                        } else if item.is_dir {
                            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(Color::White)
                        };

                        let prefix = if is_selected { "▶ " } else { "  " };
                        let size_str = if item.is_dir {
                            "<DIR>".to_string()
                        } else if item.size_bytes > 1024 * 1024 {
                            format!("{:.1}M", item.size_bytes as f64 / (1024.0 * 1024.0))
                        } else if item.size_bytes > 1024 {
                            format!("{}K", item.size_bytes / 1024)
                        } else {
                            format!("{}B", item.size_bytes)
                        };

                        let name_truncated = if item.name.len() > 22 {
                            format!("{}...", &item.name[..19])
                        } else {
                            item.name.clone()
                        };

                        let line = if is_selected {
                            Line::from(vec![
                                Span::styled(prefix, style),
                                Span::styled(icon, style),
                                Span::styled(format!("{:<23}", name_truncated), style),
                                Span::styled(format!("{:>6}  {}", size_str, item.modified_str), style),
                            ])
                        } else {
                            Line::from(vec![
                                Span::styled(prefix, Style::default().fg(theme.text_muted)),
                                Span::styled(icon, Style::default().fg(icon_color)),
                                Span::styled(format!("{:<23}", name_truncated), style),
                                Span::styled(format!("{:>6}  {}", size_str, item.modified_str), Style::default().fg(theme.text_muted)),
                            ])
                        };

                        ListItem::new(line)
                    })
                    .collect()
            }
        }
    };

    let list = List::new(items).block(
        Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color)),
    );

    f.render_widget(list, area);
}

fn get_file_icon(item: &crate::file_manager::FileItem) -> (&'static str, Color) {
    if item.is_parent {
        return ("⤴ ", Color::Rgb(251, 191, 36));
    }
    if item.is_dir {
        return ("📁 ", Color::Rgb(99, 102, 241));
    }

    let name = item.name.to_lowercase();
    if name.ends_with(".rs") {
        ("󱘗 ", Color::Rgb(249, 115, 22))
    } else if name.ends_with(".toml") || name.ends_with(".yaml") || name.ends_with(".yml") || name.ends_with(".json") {
        (" ", Color::Rgb(234, 179, 8))
    } else if name.ends_with(".md") || name.ends_with(".txt") {
        ("󰍔 ", Color::Rgb(56, 189, 248))
    } else if name.ends_with(".ts") || name.ends_with(".js") || name.ends_with(".jsx") || name.ends_with(".tsx") {
        (" ", Color::Rgb(250, 204, 21))
    } else if name.ends_with(".sh") || name.ends_with(".bat") || name.ends_with(".ps1") {
        (" ", Color::Rgb(52, 211, 153))
    } else if name.ends_with(".lock") {
        ("🔒 ", Color::Rgb(156, 163, 175))
    } else {
        ("📄 ", Color::White)
    }
}

fn render_model_picker(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.current_theme;
    let popup_area = centered_rect(88, 75, area);
    f.render_widget(Clear, popup_area);

    let popup_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Zakładki Providerów (Tabs)
            Constraint::Min(8),    // 2 Kolumny: Lista + Karta Parametrów
            Constraint::Length(1), // Pasek podpowiedzi
        ])
        .split(popup_area);

    // ── GÓRNY PASEK ZAKŁADEK PROVIDERÓW ──
    let tabs = App::model_provider_tabs();
    let current_tab_idx = app.model_filter_index % tabs.len();

    let tab_spans: Vec<Span> = tabs
        .iter()
        .enumerate()
        .flat_map(|(i, (label, _))| {
            let is_active = i == current_tab_idx;
            let (bg_color, fg_color) = if is_active {
                (theme.primary, Color::Black)
            } else {
                (Color::Reset, theme.text_muted)
            };

            vec![
                Span::styled(
                    format!(" {} ", label),
                    Style::default().bg(bg_color).fg(fg_color).add_modifier(if is_active { Modifier::BOLD } else { Modifier::empty() }),
                ),
                Span::styled(" ", Style::default()),
            ]
        })
        .collect();

    let tabs_widget = Paragraph::new(Line::from(tab_spans)).block(
        Block::default()
            .title(" ◈ Dostawcy & Ulubione (Użyj [←/→]/[Tab]/[ ]/[[] aby zmienić) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.primary)),
    );
    f.render_widget(tabs_widget, popup_chunks[0]);

    // ── 2 KOLUMNY: LEWA (LISTA MODELI), PRAWA (KARTA SPECYFIKACJI I TOKENÓW) ──
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(52), // Lista modeli
            Constraint::Percentage(48), // Szczegóły tokenów i limitów
        ])
        .split(popup_chunks[1]);

    let filtered = app.filtered_models();
    let visible_height = (body_chunks[0].height.saturating_sub(2)) as usize;
    let filtered_len = filtered.len();
    let effective_idx = if filtered_len == 0 { 0 } else { app.model_picker_index.min(filtered_len - 1) };
    // Oblicz offset viewportu aby zaznaczony element był zawsze widoczny
    let offset = if filtered_len > visible_height && visible_height > 0 && effective_idx >= visible_height {
        let mut off = effective_idx - visible_height + 1;
        if off + visible_height > filtered_len {
            off = filtered_len.saturating_sub(visible_height);
        }
        off
    } else {
        0
    };

    // LEWA KOLUMNA: LISTA MODELI (z viewport scrolling)
    let items: Vec<ListItem> = if filtered.is_empty() {
        vec![ListItem::new(Line::from(vec![
            Span::styled("   Brak modeli w tej kategorii", Style::default().fg(theme.text_muted)),
        ]))]
    } else {
        filtered
            .iter()
            .skip(offset)
            .take(visible_height.max(1))
            .enumerate()
            .map(|(i, (id, name, prov))| {
                let original_idx = offset + i;
                let is_selected = original_idx == effective_idx;
                let is_current = id == &app.active_model;
                let is_fav = app.config.favorite_models.contains(&id.to_string());

                let fav_icon = if is_fav { "★ " } else { "  " };

                let style = if is_selected {
                    Style::default().fg(Color::Black).bg(theme.primary).add_modifier(Modifier::BOLD)
                } else if is_current {
                    Style::default().fg(theme.assistant).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let prefix = if is_selected {
                    "▶ "
                } else if is_current {
                    "● "
                } else {
                    "  "
                };

                let name_truncated = if name.len() > 28 {
                    format!("{}...", &name[..25])
                } else {
                    name.to_string()
                };

                let line = Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(fav_icon, if is_fav { Style::default().fg(Color::Rgb(250, 204, 21)).add_modifier(Modifier::BOLD) } else { style }),
                    Span::styled(format!("{:<28}", name_truncated), style),
                    Span::styled(format!(" [{:<10}]", prov), if is_selected { style } else { Style::default().fg(theme.accent) }),
                ]);
                ListItem::new(line)
            })
            .collect()
    };

    let list_title = if filtered_len > visible_height && visible_height > 0 {
        format!(" Dostępne Modele ({}/{}) [{}-{}] ", filtered_len, filtered_len, offset + 1, (offset + visible_height).min(filtered_len))
    } else {
        format!(" Dostępne Modele ({}) ", filtered_len)
    };
    let list = List::new(items).block(
        Block::default()
            .title(list_title)
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_active)),
    );
    f.render_widget(list, body_chunks[0]);

    // PRAWA KOLUMNA: KARTA PARAMETRÓW, TOKENÓW I KREDYTÓW DANEGO PROVIDERA
    let selected_model = filtered.get(effective_idx);

    let details_lines = if let Some((id, name, prov)) = selected_model {
        let is_fav = app.config.favorite_models.contains(&id.to_string());
        let is_active = id == &app.active_model;

        let (quota_str, context_window, max_out, speed_str, reasoning_str) = match prov.as_str() {
            "commandcode" => (
                "💳 Quota / Kredyt: $0.00 / $10.00 (100% dostępne)",
                "200,000 tokenów (200k)",
                "64,000 tokenów",
                "⚡ Szybki (~60 tok/s)",
                "✅ Hybrydowe wnioskowanie i agent CLI",
            ),
            "antigravity" => (
                "🪐 Google Antigravity: Multi-Agent SDK / Pro Tier",
                "200,000 - 1,000,000 tokenów",
                "64,000 tokenów",
                "🚀 Ultra Szybki (~110 tok/s)",
                "✅ Zaawansowane planowanie i podagenci",
            ),
            "opencode" => (
                "⚡ OpenCode Engine: Natywny Runtime (Nielimitowany)",
                "200,000 tokenów",
                "32,000 tokenów",
                "⚡ Błyskawiczny (~85 tok/s)",
                "✅ Pełna integracja z terminalem",
            ),
            "trae" => (
                "🎯 Trae AI Engine: Unlimited Free Developer Tier",
                "200,000 tokenów",
                "32,000 tokenów",
                "⚡ Bardzo szybki",
                "✅ Claude 3.7 & GPT-4o",
            ),
            "cursor" => (
                "🔮 Cursor Pro Bridge: Nielimitowane szybkie zapytania",
                "200,000 tokenów",
                "64,000 tokenów",
                "⚡ Natychmiastowy",
                "✅ Claude 3.7 Thinking & o3-mini",
            ),
            "windsurf" => (
                "🌊 Windsurf Cascade: Premium Flow Tier",
                "200,000 tokenów",
                "32,000 tokenów",
                "⚡ Płynny",
                "✅ Cascade Multi-file Context",
            ),
            "gemini" => (
                "🌐 Google Gemini Direct: 15 RPM / 1M TPM Free",
                "1,000,000 tokenów (1M)",
                "64,000 tokenów",
                "🚀 Ekstremalnie szybki",
                "✅ Gemini 3.7 Flash / Pro / Flash Thinking",
            ),
            "groq" => (
                "⚡ Groq LPU Speed: 1,000 tokenów/s (Free Tier)",
                "128,000 tokenów",
                "16,000 tokenów",
                "⚡⚡ 1000 tok/s (Real-time)",
                "✅ Qwen 2.5 Coder & DeepSeek R1",
            ),
            "lmstudio" => (
                "🖥️ LM Studio: 100% Darmowy Serwer Lokalny (Port 1234)",
                "32,000 - 128,000 tokenów",
                "16,000 tokenów",
                "💻 Zależna od lokalnego GPU / CUDA / ROCm / Metal",
                "✅ Wsparcie dla GGUF, DeepSeek R1, Qwen Coder, Llama 3",
            ),
            "llamacpp" => (
                "🦙 Llama.cpp Server: Wysokowydajny serwer C++ (Port 8080)",
                "32,000 - 128,000 tokenów",
                "16,000 tokenów",
                "🚀 Zoptymalizowany pod AVX-512 / CUDA / Metal",
                "✅ Obsługa dowolnego pliku .gguf z dysku",
            ),
            "ollama" => (
                "🦙 Ollama Local: 100% Darmowy & Prywatny (Offline)",
                "32,000 - 128,000 tokenów",
                "8,000 tokenów",
                "💻 Zależna od lokalnego GPU/NPU",
                "✅ Lokalne bezpieczeństwo danych",
            ),
            _ => (
                "🔑 OpenAI-Compatible / Custom Local Endpoint",
                "128,000 tokenów",
                "16,000 tokenów",
                "Zależna od serwera",
                "✅ Standard OpenAI API / Chat Completions",
            ),
        };

        vec![
            Line::from(vec![
                Span::styled("Model: ", Style::default().fg(theme.text_muted)),
                Span::styled(name.to_string(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("ID:    ", Style::default().fg(theme.text_muted)),
                Span::styled(id.to_string(), Style::default().fg(theme.accent)),
            ]),
            Line::from(vec![
                Span::styled("Dostawca: ", Style::default().fg(theme.text_muted)),
                Span::styled(prov.to_uppercase().to_string(), Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(Span::raw("")),
            Line::from(vec![
                Span::styled("Dostępny Budżet / Quota:", Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled(format!("  {}", quota_str), Style::default().fg(theme.assistant).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(Span::raw("")),
            Line::from(vec![
                Span::styled("Parametry & Limity Tokenów:", Style::default().fg(theme.secondary).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  • Max Kontekst: ", Style::default().fg(theme.text_muted)),
                Span::styled(context_window, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("  • Max Wyjście:  ", Style::default().fg(theme.text_muted)),
                Span::styled(max_out, Style::default().fg(Color::White)),
            ]),
            Line::from(vec![
                Span::styled("  • Prędkość:     ", Style::default().fg(theme.text_muted)),
                Span::styled(speed_str, Style::default().fg(theme.assistant)),
            ]),
            Line::from(vec![
                Span::styled("  • Wnioskowanie: ", Style::default().fg(theme.text_muted)),
                Span::styled(reasoning_str, Style::default().fg(Color::Rgb(210, 215, 225))),
            ]),
            Line::from(Span::raw("")),
            Line::from(vec![
                Span::styled("Status: ", Style::default().fg(theme.text_muted)),
                if is_active {
                    Span::styled("● AKTYWNY OPERATOR  ", Style::default().fg(theme.assistant).add_modifier(Modifier::BOLD))
                } else {
                    Span::styled("○ Gotowy do wyboru  ", Style::default().fg(theme.text_muted))
                },
                if is_fav {
                    Span::styled("★ Przypięty do Ulubionych", Style::default().fg(Color::Rgb(250, 204, 21)).add_modifier(Modifier::BOLD))
                } else {
                    Span::styled("[f] Dodaj do Ulubionych", Style::default().fg(theme.text_muted))
                },
            ]),
            Line::from(vec![
                Span::styled("Wymaga: ", Style::default().fg(theme.text_muted)),
                Span::styled(requirement_str(prov), Style::default().fg(theme.accent)),
            ]),
        ]
    } else {
        vec![
            Line::from(Span::styled("Wybierz model z listy po lewej", Style::default().fg(theme.text_muted))),
        ]
    };

    let details_widget = Paragraph::new(details_lines).block(
        Block::default()
            .title(" 📊 Karta Parametrów & Budżet Tokenów ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border)),
    );
    f.render_widget(details_widget, body_chunks[1]);

    // ── DOLNY PASEK PODPOWIEDZI ──
    let hint_line = Line::from(vec![
        Span::styled("  [←/→/Tab/]/[] ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled("Dostawcy  ", Style::default().fg(theme.text_muted)),
        Span::styled("[↑/↓] ", Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)),
        Span::styled("Wybór  ", Style::default().fg(theme.text_muted)),
        Span::styled("[Space/f] ", Style::default().fg(Color::Rgb(250, 204, 21)).add_modifier(Modifier::BOLD)),
        Span::styled("★ Ulubione  ", Style::default().fg(theme.text_muted)),
        Span::styled("[Enter] ", Style::default().fg(theme.assistant).add_modifier(Modifier::BOLD)),
        Span::styled("Aktywuj Model  ", Style::default().fg(theme.text_muted)),
        Span::styled("[Esc] ", Style::default().fg(theme.error).add_modifier(Modifier::BOLD)),
        Span::styled("Zamknij", Style::default().fg(theme.text_muted)),
    ]);
    let hint_widget = Paragraph::new(hint_line);
    f.render_widget(hint_widget, popup_chunks[2]);
}

fn render_session_picker(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.current_theme;
    let popup_area = centered_rect(75, 60, area);
    f.render_widget(Clear, popup_area);

    let items: Vec<ListItem> = if app.available_sessions.is_empty() {
        vec![ListItem::new(Line::from(Span::styled("  Brak zapisanych sesji dla tego projektu", Style::default().fg(theme.text_muted))))]
    } else {
        app.available_sessions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let is_selected = i == app.session_picker_index;
                let is_current = s.id == app.current_session.id;

                let style = if is_selected {
                    Style::default().fg(Color::Black).bg(theme.primary).add_modifier(Modifier::BOLD)
                } else if is_current {
                    Style::default().fg(theme.assistant).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };

                let prefix = if is_selected {
                    "▶ "
                } else if is_current {
                    "● "
                } else {
                    "  "
                };

                let title_trimmed = if s.title.len() > 28 {
                    format!("{}...", &s.title[..25])
                } else {
                    s.title.clone()
                };

                let date_str = if s.updated_at.len() >= 16 {
                    &s.updated_at[..16]
                } else {
                    &s.updated_at
                };

                let line = Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(format!("{:<30}", title_trimmed), style),
                    Span::styled(format!(" ({} msgs)", s.message_count), if is_selected { style } else { Style::default().fg(theme.accent) }),
                    Span::styled(format!(" | {} ", s.model), if is_selected { style } else { Style::default().fg(theme.secondary) }),
                    Span::styled(format!(" | {}", date_str), if is_selected { style } else { Style::default().fg(theme.text_muted) }),
                ]);
                ListItem::new(line)
            })
            .collect()
    };

    let list = List::new(items).block(
        Block::default()
            .title(" 📂 Session History (Enter: Load, 'd'/Del: Delete, Esc: Close) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.primary)),
    );

    f.render_widget(list, popup_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Zwraca opis wymagań dla danego providera — pokazywany w model pickerze.
/// Kategoryzuje na: wtyczka (bridge), CLI (binarka), klucz API, lokalny serwer, darmowe.
fn requirement_str(prov: &str) -> String {
    // 1. CLI — binarka na PATH (auto-detekcja)
    if let Some(binary) = ProviderRouter::cli_binary_for_provider(prov) {
        let available = ProviderRouter::is_cli_available(binary);
        if available {
            match binary {
                "opencode" => return "✅ CLI `opencode` zainstalowany (darmowe ling/mimo/nemotron) — uruchom `opencode auth`".to_string(),
                "devin" => return "✅ CLI `devin` zainstalowany — wymaga login devin.ai".to_string(),
                "gemini" => return "✅ CLI `gemini` zainstalowany — darmowy tier 60 req/min (Google login)".to_string(),
                "kilo" => return "✅ CLI `kilo` zainstalowany — 17 darmowych modeli (uruchom `kilo auth`)".to_string(),
                "cline" => return "✅ CLI `cline` zainstalowany — wymaga `cline auth` (API key)".to_string(),
                "claude-code-acp" => return "✅ CLI `claude-code-acp` zainstalowany — wymaga ANTHROPIC_API_KEY lub Claude Pro/Max".to_string(),
                "codex-acp" => return "✅ CLI `codex-acp` zainstalowany — wymaga OPENAI_API_KEY".to_string(),
                "claude" => return "✅ CLI `claude` zainstalowany — wymaga ANTHROPIC_API_KEY lub Claude Pro/Max".to_string(),
                "codex" => return "✅ CLI `codex` zainstalowany — wymaga OPENAI_API_KEY".to_string(),
                "aider" => return "✅ CLI `aider` zainstalowany — wymaga klucza API modelu (OpenAI/Anthropic/etc.)".to_string(),
                _ => return format!("✅ CLI `{binary}` zainstalowany"),
            }
        } else {
            match binary {
                "opencode" => return "❌ CLI `opencode` NIE zainstalowany — uruchom: npm i -g opencode-ai@latest && opencode auth".to_string(),
                "devin" => return "❌ CLI `devin` NIE zainstalowany — uruchom: npm i -g @devin/cli".to_string(),
                "gemini" => return "❌ CLI `gemini` NIE zainstalowany — uruchom: npm i -g @google/gemini-cli && gemini login".to_string(),
                "kilo" => return "❌ CLI `kilo` NIE zainstalowany — uruchom: npm i -g kilo-ai@latest && kilo auth".to_string(),
                "cline" => return "❌ CLI `cline` NIE zainstalowany — uruchom: npm i -g @cline/cli && cline auth".to_string(),
                "claude-code-acp" => return "❌ CLI `claude-code-acp` NIE zainstalowany — uruchom: npm i -g @anthropic/claude-code-acp".to_string(),
                "codex-acp" => return "❌ CLI `codex-acp` NIE zainstalowany — uruchom: npm i -g @openai/codex-acp".to_string(),
                "claude" => return "❌ CLI `claude` NIE zainstalowany — uruchom: npm i -g @anthropic/claude-code".to_string(),
                "codex" => return "❌ CLI `codex` NIE zainstalowany — uruchom: npm i -g @openai/codex".to_string(),
                "aider" => return "❌ CLI `aider` NIE zainstalowany — uruchom: pip install aider-chat".to_string(),
                _ => return format!("❌ CLI `{binary}` NIE zainstalowany"),
            }
        }
    }

    // 2. Bridge — wtyczka VS Code (Cursor/Windsurf/Trae/VS Code)
    let bridge_providers = [
        "opencode", "cursor", "windsurf", "trae", "copilot", "amazon-q", "augment", "commandcode",
    ];
    if bridge_providers.contains(&prov) {
        return "🔌 Wtyczka OpenCode-RS Bridge — zainstaluj VSIX w edytorze (Cursor/Windsurf/Trae/VS Code)".to_string();
    }

    // 3. Direct API — klucz w .env/auth.json
    match prov {
        "gemini" => "🔑 GEMINI_API_KEY w .env lub ~/.opencode/auth.json".to_string(),
        "openai" => "🔑 OPENAI_API_KEY w .env lub ~/.opencode/auth.json".to_string(),
        "anthropic" => "🔑 ANTHROPIC_API_KEY w .env lub ~/.opencode/auth.json".to_string(),
        "deepseek" => "🔑 DEEPSEEK_API_KEY w .env lub ~/.opencode/auth.json".to_string(),
        "groq" => "🔑 GROQ_API_KEY w .env lub ~/.opencode/auth.json".to_string(),
        "mistral" => "🔑 MISTRAL_API_KEY w .env lub ~/.opencode/auth.json".to_string(),
        "openrouter" => "🔑 OPENROUTER_API_KEY w .env lub ~/.opencode/auth.json".to_string(),
        "devin-cloud" => "🔑 DEVIN_API_KEY + DEVIN_ORG_ID w .env (api.devin.ai v3)".to_string(),
        "ollama" => "🖥️ Ollama running na localhost:11434 (darmowe, lokalne)".to_string(),
        "lmstudio" => "🖥️ LM Studio running na localhost:1234 (darmowe, lokalne)".to_string(),
        "llamacpp" => "🖥️ Llama.cpp server na localhost:8080 (darmowe, lokalne)".to_string(),
        "antigravity" => "🪐 Antigravity IDE uruchomione (gRPC-Web direct, darmowe)".to_string(),
        _ => "❓ Nieznany provider — sprawdź dokumentację".to_string(),
    }
}

/// Renderuje dialog uprawnień — gdy agent chce wykonać tool z permission "ask".
/// Użytkownik zatwierdza Enter (allow) lub odmawia Esc (deny).
fn render_permission_dialog(f: &mut Frame, size: Rect, app: &App) {
    if app.permission_dialog.is_none() { return; }
    let (tool, args) = app.permission_dialog.as_ref().unwrap();

    let theme = &app.current_theme;
    let popup_area = centered_rect(60, 25, size);

    // Clear background
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title("🔐 Żądanie uprawnienia")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.warning))
        .style(Style::default().bg(theme.bg_card));

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("Narzędzie: {}", tool),
            Style::default().fg(theme.primary).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            format!("Argumenty: {}", &args[..args.len().min(80)]),
            Style::default().fg(theme.text_muted),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Enter] ", Style::default().fg(theme.success).add_modifier(Modifier::BOLD)),
            Span::styled("Zezwól  ", Style::default().fg(theme.primary)),
            Span::styled("[Esc] ", Style::default().fg(theme.error).add_modifier(Modifier::BOLD)),
            Span::styled("Odmów", Style::default().fg(theme.primary)),
        ]),
    ];

    let paragraph = Paragraph::new(lines).block(block).alignment(Alignment::Center);
    f.render_widget(paragraph, popup_area);
}

/// Renderuje lewy panel w trybie IDE — drzewo / listę plików roboczych
fn render_ide_file_explorer(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.current_theme;
    let pane = &app.file_manager.left;
    let dir_name = app
        .work_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "PROJEKT".to_string());

    let block = Block::default()
        .title(format!(" 📁 {} ", dir_name.to_uppercase()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.primary));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let visible_rows = inner.height as usize;
    if visible_rows == 0 {
        return;
    }

    let items = &pane.items;
    if items.is_empty() {
        let empty_msg = Paragraph::new(Line::from(Span::styled(
            " (brak plików) ",
            Style::default().fg(theme.text_muted),
        )));
        f.render_widget(empty_msg, inner);
        return;
    }

    let selected = pane.selected_index;
    let start_idx = if selected >= visible_rows {
        selected - visible_rows + 1
    } else {
        0
    };

    let list_items: Vec<ListItem> = items
        .iter()
        .skip(start_idx)
        .take(visible_rows)
        .enumerate()
        .map(|(i, item)| {
            let real_idx = start_idx + i;
            let is_selected = real_idx == selected;
            let icon = if item.is_dir { "📁 " } else { "📄 " };
            let mut style = if item.is_dir {
                Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            if is_selected {
                style = Style::default()
                    .bg(theme.bg_card)
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD);
            }

            let line = Line::from(vec![
                Span::styled(if is_selected { "▶ " } else { "  " }, style),
                Span::styled(icon, style),
                Span::styled(item.name.clone(), style),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(list_items);
    f.render_widget(list, inner);
}

/// Renderuje środkowy panel w trybie IDE — edytor kodu z numerami linii i VS Code Dark+
fn render_ide_editor(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.current_theme;
    let file_name = app
        .editor_file_path
        .as_ref()
        .map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())
        .unwrap_or_else(|| "Brak otwartego pliku".to_string());

    let line_count = app.editor_lines.len();
    let title = if app.editor_file_path.is_some() {
        format!(" 📝 {} ({} linii) ", file_name, line_count)
    } else {
        " 📝 Edytor kodu (Wpisz /ide <plik> lub wybierz z drzewa) ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.secondary));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.editor_lines.is_empty() {
        let help_lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "   Tryb IDE aktywny (3 kolumny: Drzewo | Edytor VS Code Dark+ | Czat AI)",
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "   • Wpisz /ide <plik> np. `/ide src/main.rs`, aby wyświetlić plik",
                Style::default().fg(theme.text_muted),
            )),
            Line::from(Span::styled(
                "   • Naciśnij F3, aby przełączyć się z powrotem do klasycznego trybu terminala",
                Style::default().fg(theme.text_muted),
            )),
            Line::from(Span::styled(
                "   • Składnia w 100% z palety VS Code Dark+ (Rust, TypeScript, Python, etc.)",
                Style::default().fg(theme.secondary),
            )),
        ];
        let p = Paragraph::new(help_lines);
        f.render_widget(p, inner);
        return;
    }

    let visible_height = inner.height as usize;
    let ext = app
        .editor_file_path
        .as_ref()
        .and_then(|p| p.extension())
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();

    let scroll = app.editor_scroll.min(line_count.saturating_sub(1));
    let display_lines: Vec<Line> = app
        .editor_lines
        .iter()
        .skip(scroll)
        .take(visible_height)
        .enumerate()
        .map(|(idx, line)| {
            SyntaxHighlighter::highlight_editor_line(scroll + idx + 1, line, &ext)
        })
        .collect();

    let p = Paragraph::new(display_lines);
    f.render_widget(p, inner);
}

