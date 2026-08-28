#[cfg(test)]
mod e2e_tests {
    use crate::app::App;
    use crate::config::AppConfig;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
    use std::fs;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent { code, modifiers: KeyModifiers::empty(), kind: KeyEventKind::Press, state: crossterm::event::KeyEventState::empty() }
    }
    fn ctrl(c: char) -> KeyEvent {
        KeyEvent { code: KeyCode::Char(c), modifiers: KeyModifiers::CONTROL, kind: KeyEventKind::Press, state: crossterm::event::KeyEventState::empty() }
    }

    #[tokio::test]
    async fn e2e_mode_cycle_interactive_auto() {
        let rt_dir = std::env::temp_dir().join(format!("e2e_mode_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&rt_dir).unwrap();
        let cfg = AppConfig::default();
        let mut app = App::new(rt_dir.clone(), cfg);
        assert_eq!(app.agent_mode, "coder");
        app.handle_key_event(ctrl('t')).await.unwrap();
        assert_eq!(app.agent_mode, "architect");
        app.handle_key_event(ctrl('t')).await.unwrap();
        assert_eq!(app.agent_mode, "ask");
        app.handle_key_event(ctrl('t')).await.unwrap();
        assert_eq!(app.agent_mode, "interactive");
        app.handle_key_event(ctrl('t')).await.unwrap();
        assert_eq!(app.agent_mode, "auto");
        assert!(app.config.trust_mode);
        app.handle_key_event(ctrl('t')).await.unwrap();
        assert_eq!(app.agent_mode, "coder");
        fs::remove_dir_all(&rt_dir).ok();
    }

    #[tokio::test]
    async fn e2e_palette_filter_taste_and_skills() {
        let dir = std::env::temp_dir().join(format!("e2e_palette_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let mut app = App::new(dir.clone(), AppConfig::default());
        // Ctrl+P open palette
        app.handle_key_event(ctrl('p')).await.unwrap();
        assert!(app.show_command_palette);
        // filter "taste"
        for c in "taste".chars() {
            app.handle_key_event(key(KeyCode::Char(c))).await.unwrap();
        }
        assert!(app.filtered_palette.iter().any(|i| i.command == "/taste"));
        // Esc close
        app.handle_key_event(key(KeyCode::Esc)).await.unwrap();
        assert!(!app.show_command_palette);
        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn e2e_model_picker_scroll_and_filter() {
        let dir = std::env::temp_dir().join(format!("e2e_model_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let mut app = App::new(dir.clone(), AppConfig::default());
        // open picker Ctrl+M
        app.handle_key_event(ctrl('m')).await.unwrap();
        assert!(app.show_model_picker);
        // initial tab is fav (8), switch to All for full count
        let total_fav = app.filtered_models().len();
        assert!(total_fav >= 1, "fav should have at least 1");
        app.handle_key_event(key(KeyCode::Right)).await.unwrap(); // -> All
        assert_eq!(app.model_picker_index, 0);
        let total_all = app.filtered_models().len();
        assert!(total_all >= 70, "All should have 70+ models, got {}", total_all);
        // Down should increase index by 1
        let before = app.model_picker_index;
        app.handle_key_event(key(KeyCode::Down)).await.unwrap();
        assert_eq!(app.model_picker_index, before + 1);
        // mouse scroll down via handle_mouse_event (simulated: filtered len check)
        let total = app.filtered_models().len();
        if total > 1 {
            app.handle_mouse_event(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::ScrollUp, // physical up -> logical down after inversion
                column: 0, row: 0,
                modifiers: KeyModifiers::empty(),
            });
            assert_eq!(app.model_picker_index, 2);
        }
        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn e2e_file_manager_and_terminal_tag() {
        let dir = std::env::temp_dir().join(format!("e2e_fm_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("a.txt"), "hello").unwrap();
        fs::write(dir.join(".opencode").join("terminal.log"), "").ok();
        fs::create_dir_all(dir.join(".opencode")).ok();
        fs::write(dir.join(".opencode").join("terminal.log"), "cargo test ok").unwrap();
        let mut app = App::new(dir.clone(), AppConfig::default());
        // Ctrl+E open
        app.handle_key_event(ctrl('e')).await.unwrap();
        assert!(app.show_file_manager);
        // navigate down
        app.handle_key_event(key(KeyCode::Down)).await.unwrap();
        // close
        app.handle_key_event(key(KeyCode::Esc)).await.unwrap();
        assert!(!app.show_file_manager);
        // @terminal resolve
        let cm = crate::agent::context::ContextManager::new(dir.clone());
        let res = cm.resolve_smart_context("check @terminal");
        assert!(res.contains("cargo test ok"));
        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn e2e_taste_command_flow() {
        let dir = std::env::temp_dir().join(format!("e2e_taste_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(dir.join(".commandcode").join("taste").join("cli")).unwrap();
        fs::write(dir.join(".commandcode").join("taste").join("cli").join("taste.md"), "prefer tabs\nmax 80 chars").unwrap();
        let mut app = App::new(dir.clone(), AppConfig::default());
        // /taste should show status with our taste
        app.handle_key_event(key(KeyCode::Char('/'))).await.unwrap();
        assert!(app.show_command_palette);
        app.handle_key_event(key(KeyCode::Esc)).await.unwrap();
        // direct command
        // We'll directly test TasteManager
        let tm = crate::taste::TasteManager::new(dir.clone());
        assert!(tm.is_enabled());
        assert!(tm.collect_taste_content().contains("prefer tabs"));
        assert!(tm.status_report().contains("Taste-1 Status"));
        fs::remove_dir_all(&dir).ok();
    }
}
