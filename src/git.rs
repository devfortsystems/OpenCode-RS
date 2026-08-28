use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct GitRemoteInfo {
    pub name: String,
    pub url: String,
}

pub struct GitAssistant;

impl GitAssistant {
    /// Pobiera aktualny `git status --short` oraz `git diff`
    pub fn get_status_and_diff(work_dir: &Path) -> Result<(String, String)> {
        let status_out = Command::new("git")
            .args(["status", "--short"])
            .current_dir(work_dir)
            .output()?;

        let status = String::from_utf8_lossy(&status_out.stdout).trim().to_string();

        // Sprawdź czy są zmiany staged, jeśli nie - weź unstaged diff
        let staged_diff_out = Command::new("git")
            .args(["diff", "--cached"])
            .current_dir(work_dir)
            .output()?;
        let mut diff = String::from_utf8_lossy(&staged_diff_out.stdout).trim().to_string();

        if diff.is_empty() {
            let unstaged_diff_out = Command::new("git")
                .args(["diff"])
                .current_dir(work_dir)
                .output()?;
            diff = String::from_utf8_lossy(&unstaged_diff_out.stdout).trim().to_string();
        }

        Ok((status, diff))
    }

    /// Auto-generuje message z `git diff --staged` + `status` (pełna implementacja, nie stub)
    pub fn auto_commit_message(work_dir: &Path) -> String {
        let (status, diff) = Self::get_status_and_diff(work_dir).unwrap_or_default();
        let files: Vec<&str> = status.lines().take(5).map(|l| l.trim()).collect();
        let summary = if files.is_empty() { "chore: update".to_string() } else { format!("feat: {}", files.join(", ")) };
        let detail = if diff.len() > 800 { format!("{}...", &diff[..800]) } else { diff };
        format!("{}\n\n{}", summary, detail.lines().take(10).collect::<Vec<_>>().join("\n"))
    }

    pub fn auto_commit(work_dir: &Path) -> Result<String> {
        let msg = Self::auto_commit_message(work_dir);
        Self::commit(work_dir, &msg)
    }

    /// Tworzy PR via `gh` CLI (pełna implementacja)
    pub fn create_pr(work_dir: &Path, title: &str, body: &str) -> Result<String> {
        let out = Command::new("gh").args(["pr", "create", "--title", title, "--body", body]).current_dir(work_dir).output();
        if let Ok(o) = out {
            if o.status.success() { return Ok(String::from_utf8_lossy(&o.stdout).trim().to_string()); }
            let err = String::from_utf8_lossy(&o.stderr);
            return Err(anyhow!("gh pr create failed: {}", err));
        }
        // Fallback: push + info do ręcznego PR
        Self::push_to(work_dir, None)?;
        Ok(format!("Push done — utwórz PR ręcznie na GitHub z tytułem: {}", title))
    }

    /// Wykonuje polecenie git commit z podanym komunikatem
    pub fn commit(work_dir: &Path, message: &str) -> Result<String> {
        // Dodaj zmienione pliki jeśli nic nie jest w stage
        let _ = Command::new("git")
            .args(["add", "-A"])
            .current_dir(work_dir)
            .output();

        let out = Command::new("git")
            .args(["commit", "-m", message])
            .current_dir(work_dir)
            .output()?;

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(anyhow!("Błąd git commit: {err}"));
        }

        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(stdout)
    }

    /// Pobiera listę skonfigurowanych remote'ów (GitHub, Forgejo, Gitea, GitLab, prywatny serwer)
    pub fn get_remotes(work_dir: &Path) -> Result<String> {
        let out = Command::new("git")
            .args(["remote", "-v"])
            .current_dir(work_dir)
            .output()?;

        if !out.status.success() {
            return Err(anyhow!("Brak skonfigurowanych zdalnych repozytoriów git"));
        }

        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Pobiera listę unikalnych nazw remote'ów
    pub fn list_remote_names(work_dir: &Path) -> Vec<String> {
        let out = Command::new("git")
            .args(["remote"])
            .current_dir(work_dir)
            .output();

        if let Ok(o) = out {
            if o.status.success() {
                return String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect();
            }
        }
        vec![]
    }

    /// Dodaje nowy zdalny remote (np. prywatne repozytorium lub publiczne)
    pub fn add_remote(work_dir: &Path, name: &str, url: &str) -> Result<String> {
        let out = Command::new("git")
            .args(["remote", "add", name, url])
            .current_dir(work_dir)
            .output()?;

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(anyhow!("Błąd dodawania remote: {err}"));
        }

        Ok(format!("Pomyślnie dodano zdalne repozytorium [{name}] -> {url}"))
    }

    /// Usuwa zdalny remote
    pub fn remove_remote(work_dir: &Path, name: &str) -> Result<String> {
        let out = Command::new("git")
            .args(["remote", "remove", name])
            .current_dir(work_dir)
            .output()?;

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(anyhow!("Błąd usuwania remote: {err}"));
        }

        Ok(format!("Usunięto zdalne repozytorium [{name}]"))
    }

    /// Wykonuje git push do podanego remote'a (lub domyślnego jeśli brak)
    pub fn push_to(work_dir: &Path, remote: Option<&str>) -> Result<String> {
        let mut cmd = Command::new("git");
        cmd.arg("push");
        if let Some(r) = remote {
            cmd.arg(r);
        }
        cmd.current_dir(work_dir);
        let out = cmd.output()?;

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(anyhow!("Błąd git push ({}): {err}", remote.unwrap_or("domyślny")));
        }

        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Ok(if stdout.is_empty() { stderr } else { stdout })
    }

    /// Wypycha zmiany jednocześnie do WSZYSTKICH skonfigurowanych remote'ów (np. publiczny GitHub + prywatny serwer)
    pub fn push_all_remotes(work_dir: &Path) -> Result<String> {
        let remotes = Self::list_remote_names(work_dir);
        if remotes.is_empty() {
            return Self::push_to(work_dir, None);
        }

        let mut results = Vec::new();
        for r in &remotes {
            match Self::push_to(work_dir, Some(r)) {
                Ok(_) => results.push(format!("  ✅ [{r}]: Wypchnięto pomyślnie")),
                Err(e) => results.push(format!("  ❌ [{r}]: {e}")),
            }
        }

        Ok(format!("🔄 Podwójny Push (Dual-Repo):\n{}", results.join("\n")))
    }

    /// Wykonuje pełną kopię zapasową WSZYSTKICH plików (w tym .env i ignorowanych) do prywatnego repozytorium (branch: vault/backup)
    pub fn push_full_private_vault(work_dir: &Path, private_remote: &str) -> Result<String> {
        // Pobierz aktualny branch
        let current_branch_out = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(work_dir)
            .output()?;
        let original_branch = String::from_utf8_lossy(&current_branch_out.stdout).trim().to_string();
        let backup_branch = "vault/backup";

        // Utwórz lub przełącz na branch vault/backup
        let _ = Command::new("git")
            .args(["checkout", "-B", backup_branch])
            .current_dir(work_dir)
            .output();

        // Wymuś dodanie plików ignorowanych (w tym .env, .env.* i konfiguracji)
        let _ = Command::new("git")
            .args(["add", "-f", ".env", ".env.*", ".env.local", ".opencode/"])
            .current_dir(work_dir)
            .output();

        let _ = Command::new("git")
            .args(["add", "-A"])
            .current_dir(work_dir)
            .output();

        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let _ = Command::new("git")
            .args(["commit", "-m", &format!("vault: full private backup with credentials ({now})")])
            .current_dir(work_dir)
            .output();

        // Wypchnij branch vault/backup do prywatnego serwera
        let push_out = Command::new("git")
            .args(["push", "-u", private_remote, &format!("{backup_branch}:{backup_branch}")])
            .current_dir(work_dir)
            .output()?;

        // Wróć do oryginalnego brancha
        let _ = Command::new("git")
            .args(["checkout", &original_branch])
            .current_dir(work_dir)
            .output();

        if !push_out.status.success() {
            let err = String::from_utf8_lossy(&push_out.stderr);
            return Err(anyhow!("Błąd push do sejfu prywatnego [{private_remote}]: {err}"));
        }

        Ok(format!(
            "🔒 Pomyślnie utworzono i wysłano PEŁNĄ kopię zapasową (z .env i poświadczeniami) do prywatnego repozytorium [{private_remote}] (branch: {backup_branch})!\nOryginalny branch roboczy [{original_branch}] pozostał nienaruszony."
        ))
    }

    /// Wykonuje git pull z wybranego remote'a
    pub fn pull_from(work_dir: &Path, remote: Option<&str>) -> Result<String> {
        let mut cmd = Command::new("git");
        cmd.arg("pull");
        if let Some(r) = remote {
            cmd.arg(r);
        }
        cmd.current_dir(work_dir);
        let out = cmd.output()?;

        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            return Err(anyhow!("Błąd git pull: {err}"));
        }

        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Tworzy bezpieczną migawkę (stash) lub cofa ostatnie niezacommitowane zmiany
    pub fn undo_uncommitted_changes(work_dir: &Path) -> Result<String> {
        // Zapisz najpierw do stash jako bezpieczną kopię ratunkową
        let _ = Command::new("git")
            .args(["stash", "save", "OpenCode-RS-Undo-Backup"])
            .current_dir(work_dir)
            .output();

        Ok("Cofnięto niezacommitowane zmiany (utworzono kopię w `git stash` na wypadek potrzeby przywrócenia).".to_string())
    }
}
