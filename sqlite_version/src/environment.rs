use anyhow::{anyhow, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::AppConfig;

#[derive(Debug, Clone)]
pub struct EnvironmentManager {
    pub active_target: String, // np. "dev", "test", "production", "staging"
    pub work_dir: PathBuf,
}

impl EnvironmentManager {
    pub fn new(work_dir: PathBuf) -> Self {
        Self {
            active_target: "dev".to_string(),
            work_dir,
        }
    }

    /// Pobiera aktualny branch git
    pub fn get_current_branch(work_dir: &Path) -> Result<String> {
        let out = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(work_dir)
            .output()?;

        if !out.status.success() {
            return Err(anyhow!("Projekt nie jest repozytorium git lub brak commitów"));
        }

        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Pobiera listę wszystkich branchy lokalnych
    pub fn list_branches(work_dir: &Path) -> Result<String> {
        let out = Command::new("git")
            .args(["branch", "-a"])
            .current_dir(work_dir)
            .output()?;

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(anyhow!("Błąd pobierania branchy: {err}"));
        }

        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }

    /// Przełącza lub tworzy nowy branch git
    pub fn switch_branch(work_dir: &Path, branch_name: &str) -> Result<String> {
        // Spróbuj przełączyć na istniejący branch
        let checkout_out = Command::new("git")
            .args(["checkout", branch_name])
            .current_dir(work_dir)
            .output()?;

        if checkout_out.status.success() {
            return Ok(format!("Przełączono na istniejący branch: `{branch_name}`"));
        }

        // Jeśli branch nie istnieje, stwórz nowy (checkout -b)
        let create_out = Command::new("git")
            .args(["checkout", "-b", branch_name])
            .current_dir(work_dir)
            .output()?;

        if !create_out.status.success() {
            let err = String::from_utf8_lossy(&create_out.stderr);
            return Err(anyhow!("Błąd tworzenia brancha `{branch_name}`: {err}"));
        }

        Ok(format!("Utworzono i przełączono na nowy branch: `{branch_name}`"))
    }

    /// Ustawia aktywne środowisko (np. dev, test, prod, custom) i wczytuje powiązany plik .env
    pub fn set_target(&mut self, target: &str, config: &mut AppConfig) -> String {
        self.active_target = target.to_lowercase();

        // Sprawdź czy istnieje dedykowany plik .env dla tego targetu
        let env_target_file = self.work_dir.join(format!(".env.{}", self.active_target));
        let mut loaded_msg = String::new();

        if env_target_file.exists() {
            if let Ok(count) = crate::auth::AuthManager::import_from_env(&env_target_file, config) {
                loaded_msg = format!(" (Załadowano {} zmiennych z pliku .env.{})", count, self.active_target);
            }
        }

        format!("Aktywne środowisko: [{target}]{loaded_msg}")
    }

    /// Zwraca listę zdefiniowanych plików środowiskowych w projekcie
    pub fn list_available_targets(&self) -> Vec<String> {
        let mut targets = vec!["dev".to_string(), "test".to_string(), "production".to_string(), "staging".to_string()];

        if let Ok(entries) = fs::read_dir(&self.work_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(".env.") {
                    let target = name.trim_start_matches(".env.").to_string();
                    if !targets.contains(&target) {
                        targets.push(target);
                    }
                }
            }
        }

        targets
    }
}
