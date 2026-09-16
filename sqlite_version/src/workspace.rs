use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Reprezentacja pojedynczej karty roboczej (Session Tab).
/// Każda karta posiada niezależny katalog roboczy (inny projekt),
/// niezależny model AI, własną sesję oraz zapamiętany draft promptu.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceTab {
    pub id: String,
    pub title: String,
    pub project_path: PathBuf,
    pub model: String,
    pub session_id: String,
    pub active_file: Option<PathBuf>,
    pub scroll_line: usize,
    /// Niedokończony prompt wpisany przez użytkownika (WezTerm-style resurrect).
    /// Nie ginie przy restarcie komputera / ubiciu procesu.
    pub draft_prompt: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Globalny stan przestrzeni roboczej z wieloma kartami.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceState {
    pub active_tab_id: String,
    pub tabs: Vec<WorkspaceTab>,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            active_tab_id: String::new(),
            tabs: Vec::new(),
        }
    }
}

/// Menedżer przestrzeni roboczej (Workspace Resurrect Engine).
/// Odpowiada za zarządzanie kartami, izolację katalogów projektów
/// oraz natychmiastowe utrwalanie stanu do bazy i dysku.
pub struct WorkspaceManager {
    state_file: PathBuf,
    pub state: WorkspaceState,
}

impl WorkspaceManager {
    /// Tworzy instancję menedżera, odczytując stan z ~/.opencode-rs/workspace_state.json
    /// lub tworząc nową początkową kartę dla podanego katalogu i modelu.
    pub fn load_or_create(default_work_dir: &Path, default_model: &str) -> Self {
        let state_file = Self::resolve_state_file();

        if let Ok(content) = fs::read_to_string(&state_file) {
            if let Ok(mut loaded) = serde_json::from_str::<WorkspaceState>(&content) {
                // Jeśli wczytano stan z kartami, upewnij się że active_tab_id wskazuje na istniejącą kartę
                if !loaded.tabs.is_empty() {
                    if !loaded.tabs.iter().any(|t| t.id == loaded.active_tab_id) {
                        loaded.active_tab_id = loaded.tabs[0].id.clone();
                    }
                    return Self {
                        state_file,
                        state: loaded,
                    };
                }
            }
        }

        // Brak zapisanego stanu lub puste karty — zainicjalizuj domyślną kartę
        let now = Utc::now().to_rfc3339();
        let tab_id = Uuid::new_v4().to_string();
        let session_id = Uuid::new_v4().to_string();

        let initial_tab = WorkspaceTab {
            id: tab_id.clone(),
            title: default_work_dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Główny".to_string()),
            project_path: default_work_dir.to_path_buf(),
            model: default_model.to_string(),
            session_id,
            active_file: None,
            scroll_line: 0,
            draft_prompt: String::new(),
            created_at: now.clone(),
            updated_at: now,
        };

        let state = WorkspaceState {
            active_tab_id: tab_id,
            tabs: vec![initial_tab],
        };

        let manager = Self { state_file, state };
        manager.save().ok();
        manager
    }

    /// Zwraca ścieżkę do pliku stanu (~/.opencode-rs/workspace_state.json)
    pub fn resolve_state_file() -> PathBuf {
        if let Some(user_dirs) = directories::UserDirs::new() {
            let dir = user_dirs.home_dir().join(".opencode-rs");
            let _ = fs::create_dir_all(&dir);
            return dir.join("workspace_state.json");
        }
        PathBuf::from(".opencode-rs").join("workspace_state.json")
    }

    /// Zapisuje stan do pliku na dysku (oraz opcjonalnie bazy danych)
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.state_file.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(&self.state)?;
        fs::write(&self.state_file, json)?;
        Ok(())
    }

    /// Pobiera referencję do aktualnie aktywnej karty
    pub fn get_active_tab(&self) -> Option<&WorkspaceTab> {
        self.state.tabs.iter().find(|t| t.id == self.state.active_tab_id)
    }

    /// Pobiera mutowalną referencję do aktualnie aktywnej karty
    pub fn get_active_tab_mut(&mut self) -> Option<&mut WorkspaceTab> {
        let active_id = self.state.active_tab_id.clone();
        self.state.tabs.iter_mut().find(|t| t.id == active_id)
    }

    /// Pobiera kartę o podanym ID
    pub fn get_tab(&self, tab_id: &str) -> Option<&WorkspaceTab> {
        self.state.tabs.iter().find(|t| t.id == tab_id)
    }

    /// Pobiera mutowalną referencję do karty o podanym ID
    pub fn get_tab_mut(&mut self, tab_id: &str) -> Option<&mut WorkspaceTab> {
        self.state.tabs.iter_mut().find(|t| t.id == tab_id)
    }

    /// Tworzy nową kartę z podanym projektem i modelem oraz ustawia ją jako aktywną
    pub fn new_tab(
        &mut self,
        title: Option<&str>,
        project_path: PathBuf,
        model: &str,
    ) -> String {
        let now = Utc::now().to_rfc3339();
        let tab_id = Uuid::new_v4().to_string();
        let session_id = Uuid::new_v4().to_string();

        let tab_title = title
            .map(|t| t.to_string())
            .unwrap_or_else(|| {
                project_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| format!("Karta {}", self.state.tabs.len() + 1))
            });

        let tab = WorkspaceTab {
            id: tab_id.clone(),
            title: tab_title,
            project_path,
            model: model.to_string(),
            session_id,
            active_file: None,
            scroll_line: 0,
            draft_prompt: String::new(),
            created_at: now.clone(),
            updated_at: now,
        };

        self.state.tabs.push(tab);
        self.state.active_tab_id = tab_id.clone();
        let _ = self.save();
        tab_id
    }

    /// Przełącza aktywną kartę
    pub fn switch_tab(&mut self, tab_id: &str) -> bool {
        if self.state.tabs.iter().any(|t| t.id == tab_id) {
            self.state.active_tab_id = tab_id.to_string();
            let _ = self.save();
            true
        } else {
            false
        }
    }

    /// Zamyka kartę o podanym ID. Jeśli zamykana karta była aktywna,
    /// aktywuje kartę sąsiadującą. Jeśli była to jedyna karta, resetuje ją.
    pub fn close_tab(&mut self, tab_id: &str) -> bool {
        let idx = self.state.tabs.iter().position(|t| t.id == tab_id);
        if let Some(pos) = idx {
            if self.state.tabs.len() == 1 {
                // Nie usuwaj ostatniej karty — wyczyść ją i zresetuj sesję
                let now = Utc::now().to_rfc3339();
                let tab = &mut self.state.tabs[0];
                tab.session_id = Uuid::new_v4().to_string();
                tab.draft_prompt.clear();
                tab.active_file = None;
                tab.scroll_line = 0;
                tab.title = "Główny".to_string();
                tab.updated_at = now;
                let _ = self.save();
                return true;
            }

            self.state.tabs.remove(pos);

            // Jeśli usunięto aktywną kartę, przełącz na najbliższą
            if self.state.active_tab_id == tab_id {
                let new_pos = if pos >= self.state.tabs.len() {
                    self.state.tabs.len() - 1
                } else {
                    pos
                };
                self.state.active_tab_id = self.state.tabs[new_pos].id.clone();
            }

            let _ = self.save();
            true
        } else {
            false
        }
    }

    /// Aktualizuje wpisywany draft promptu (np. co kilka sekund lub po zmianie w polu tekstowym)
    pub fn update_draft(&mut self, tab_id: &str, draft: &str) {
        if let Some(tab) = self.get_tab_mut(tab_id) {
            if tab.draft_prompt != draft {
                tab.draft_prompt = draft.to_string();
                tab.updated_at = Utc::now().to_rfc3339();
                let _ = self.save();
            }
        }
    }

    /// Zmienia model przypisany do danej karty
    pub fn update_model(&mut self, tab_id: &str, model: &str) {
        if let Some(tab) = self.get_tab_mut(tab_id) {
            tab.model = model.to_string();
            tab.updated_at = Utc::now().to_rfc3339();
            let _ = self.save();
        }
    }

    /// Zmienia katalog roboczy (projekt) przypisany do danej karty
    pub fn update_project_path(&mut self, tab_id: &str, path: PathBuf) {
        if let Some(tab) = self.get_tab_mut(tab_id) {
            tab.title = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Projekt".to_string());
            tab.project_path = path;
            tab.updated_at = Utc::now().to_rfc3339();
            let _ = self.save();
        }
    }

    /// Aktualizuje stan otwartego pliku w edytorze kodu danej karty
    pub fn update_editor(&mut self, tab_id: &str, file: Option<PathBuf>, scroll_line: usize) {
        if let Some(tab) = self.get_tab_mut(tab_id) {
            tab.active_file = file;
            tab.scroll_line = scroll_line;
            tab.updated_at = Utc::now().to_rfc3339();
            let _ = self.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspace_manager_lifecycle() {
        let temp_dir = std::env::temp_dir().join(format!("opencode_ws_{}", Uuid::new_v4()));
        let _ = fs::create_dir_all(&temp_dir);

        let mut mgr = WorkspaceManager {
            state_file: temp_dir.join("workspace_state.json"),
            state: WorkspaceState::default(),
        };

        // 1. Dodaj pierwszą kartę na projekcie A z modelem claude
        let proj_a = temp_dir.join("projekt_a");
        let tab1_id = mgr.new_tab(Some("Projekt A"), proj_a.clone(), "claude-3.7-sonnet");

        assert_eq!(mgr.state.tabs.len(), 1);
        assert_eq!(mgr.state.active_tab_id, tab1_id);
        assert_eq!(mgr.get_active_tab().unwrap().title, "Projekt A");
        assert_eq!(mgr.get_active_tab().unwrap().project_path, proj_a);
        assert_eq!(mgr.get_active_tab().unwrap().model, "claude-3.7-sonnet");

        // 2. Dodaj drugą kartę na projekcie B z modelem devin
        let proj_b = temp_dir.join("projekt_b");
        let tab2_id = mgr.new_tab(Some("Projekt B"), proj_b.clone(), "devin-acp");

        assert_eq!(mgr.state.tabs.len(), 2);
        assert_eq!(mgr.state.active_tab_id, tab2_id);
        assert_eq!(mgr.get_active_tab().unwrap().title, "Projekt B");
        assert_eq!(mgr.get_active_tab().unwrap().model, "devin-acp");

        // 3. Sprawdź draft promptu (WezTerm-style resurrect)
        mgr.update_draft(&tab1_id, "Napisz testy dla bazy danych...");
        assert_eq!(
            mgr.get_tab(&tab1_id).unwrap().draft_prompt,
            "Napisz testy dla bazy danych..."
        );

        // 4. Przełącz kartę z powrotem na tab 1
        assert!(mgr.switch_tab(&tab1_id));
        assert_eq!(mgr.state.active_tab_id, tab1_id);

        // 5. Zamknij tab 2
        assert!(mgr.close_tab(&tab2_id));
        assert_eq!(mgr.state.tabs.len(), 1);
        assert_eq!(mgr.state.active_tab_id, tab1_id);

        // 6. Zamknij tab 1 (ostatnia karta nie może zniknąć, resetuje się)
        assert!(mgr.close_tab(&tab1_id));
        assert_eq!(mgr.state.tabs.len(), 1);
        assert_eq!(mgr.state.tabs[0].draft_prompt, "");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_workspace_persistence_and_restore() {
        let temp_dir = std::env::temp_dir().join(format!("opencode_ws_persist_{}", Uuid::new_v4()));
        let _ = fs::create_dir_all(&temp_dir);
        let state_file = temp_dir.join("workspace_state.json");

        let proj_dir = temp_dir.join("mój_projekt");
        let _ = fs::create_dir_all(&proj_dir);

        {
            let mut mgr = WorkspaceManager {
                state_file: state_file.clone(),
                state: WorkspaceState::default(),
            };

            let tab_id = mgr.new_tab(Some("Mój Projekt"), proj_dir.clone(), "antigravity-flash");
            mgr.update_draft(&tab_id, "Wykryj i napraw wyciek pamięci w module X");
            mgr.update_editor(&tab_id, Some(proj_dir.join("src/main.rs")), 42);
        }

        // Symulacja restartu komputera — ponowne wczytanie z pliku stanu
        let content = fs::read_to_string(&state_file).expect("Plik stanu powinien istnieć");
        let restored_state: WorkspaceState = serde_json::from_str(&content).expect("Prawidłowy JSON");

        assert_eq!(restored_state.tabs.len(), 1);
        let restored_tab = &restored_state.tabs[0];
        assert_eq!(restored_tab.title, "Mój Projekt");
        assert_eq!(restored_tab.model, "antigravity-flash");
        assert_eq!(
            restored_tab.draft_prompt,
            "Wykryj i napraw wyciek pamięci w module X"
        );
        assert_eq!(restored_tab.scroll_line, 42);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
