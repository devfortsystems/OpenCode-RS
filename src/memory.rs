//! Memory Blocks — edytowalne segmenty system promptu, które agent sam przepisuje.
//!
//! Inspirowane Letta Code (https://docs.letta.com/letta-code/memory).
//! Trzy bloki:
//! - `persona`  — globalna tożsamość agenta (jak się zachowuje). `~/.opencode/memory/persona.md`
//! - `human`    — globalne preferencje usera (jak user lubi pracować). `~/.opencode/memory/human.md`
//! - `project`  — per-projekt (konwencje, wzorce, decyzje). `.opencode/memory/project.md`
//!
//! Bloki są zawsze wstrzykiwane do system promptu (always-in-context, jak Letta).
//! Agent edytuje je przez tools `core_memory_append` / `core_memory_replace`.
//! Komenda `/remember` każe agentowi przemyśleć sesję i zapisać wnioski.

use std::fs;
use std::path::PathBuf;

/// Etykiety bloków pamięci (label = unikalny identyfikator, jak w Letta).
pub const BLOCK_LABELS: &[&str] = &["persona", "human", "project"];

/// Maksymalny rozmiar bloku w znakach (limit, jak w Letta — blok to cenny real estate).
pub const BLOCK_CHAR_LIMIT: usize = 4000;

/// Scope bloku — globalny (współdzielony między projektami) czy per-projekt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockScope {
    Global,
    Project,
}

impl BlockScope {
    pub fn for_label(label: &str) -> Self {
        match label {
            "persona" | "human" => BlockScope::Global,
            "project" => BlockScope::Project,
            _ => BlockScope::Project,
        }
    }
}

/// Opis bloku — Letta używa `description` jako głównej wskazówki dla agenta,
/// by wiedział do czego służy blok i jak go używać.
pub fn block_description(label: &str) -> &'static str {
    match label {
        "persona" => "Twoja tożsamość jako agenta — jak się zachowujesz, Twój styl pracy, zasady. Przepisuj ten blok, gdy odkryjesz, że Twoje domyślne zachowanie trzeba dopracować (np. 'zawsze najpierw czytaj testy przed zmianą kodu'). Generalizuj, nie notuj pojedynczych zdarzeń.",
        "human" => "Preferencje usera — jak user lubi pracować z Tobą. Zapisuj tu wzorce: 'user woli unwrap() zamiast match dla Option', 'user używa polskich komentarzy', 'user nie chce długich wyjaśnień'. Generalizuj preferencje, nie loguj pojedynczych rozmów.",
        "project" => "Wiedza o tym konkretnym projekcie — konwencje, wzorce architektoniczne, decyzje, pułapki. Zapisuj to, co przyszłe sesje powinny wiedzieć od startu, żeby nie musiały odkrywać od zera (np. 'ten projekt używa tokio::spawn dla IO', 'migracje DB idą przez sqlx::migrate!').",
        _ => "Własny blok pamięci agenta.",
    }
}

pub struct MemoryBlocks {
    work_dir: PathBuf,
}

impl MemoryBlocks {
    pub fn new(work_dir: PathBuf) -> Self {
        Self { work_dir }
    }

    fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(PathBuf::from)
            .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
            .or_else(|| directories::UserDirs::new().map(|d| d.home_dir().to_path_buf()))
    }

    /// Katalog pamięci globalnej: `~/.opencode/memory`
    fn global_memory_dir() -> Option<PathBuf> {
        Self::home_dir().map(|h| h.join(".opencode").join("memory"))
    }

    /// Katalog pamięci per-projekt: `.opencode/memory`
    fn project_memory_dir(&self) -> PathBuf {
        self.work_dir.join(".opencode").join("memory")
    }

    /// Zwraca ścieżkę pliku dla danego bloku (globalnego lub per-projekt).
    fn block_path(&self, label: &str) -> Option<PathBuf> {
        match BlockScope::for_label(label) {
            BlockScope::Global => Self::global_memory_dir().map(|d| d.join(format!("{label}.md"))),
            BlockScope::Project => Some(self.project_memory_dir().join(format!("{label}.md"))),
        }
    }

    /// Wczytuje zawartość bloku (pusty string jeśli nie istnieje).
    pub fn load_block(&self, label: &str) -> String {
        self.block_path(label)
            .and_then(|p| fs::read_to_string(&p).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_default()
    }

    /// Wczytuje wszystkie bloki jako (label, content).
    pub fn load_all(&self) -> Vec<(&'static str, String)> {
        BLOCK_LABELS.iter().map(|l| (*l, self.load_block(l))).collect()
    }

    /// Zapisuje zawartość bloku (tworzy katalogi jeśli trzeba, przycina do limitu).
    pub fn save_block(&self, label: &str, content: &str) -> anyhow::Result<()> {
        if !BLOCK_LABELS.contains(&label) {
            anyhow::bail!("Nieznany blok pamięci: '{label}'. Dostępne: {}", BLOCK_LABELS.join(", "));
        }
        let trimmed = content.trim();
        if trimmed.len() > BLOCK_CHAR_LIMIT {
            anyhow::bail!(
                "Blok '{label}' przekracza limit {BLOCK_CHAR_LIMIT} znaków (jest {len}). Skróć lub podziel.",
                len = trimmed.len()
            );
        }
        let path = self.block_path(label)
            .ok_or_else(|| anyhow::anyhow!("Nie można ustalić ścieżki pamięci dla bloku '{label}' (brak HOME)?"))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, trimmed)?;
        Ok(())
    }

    /// Dopisuje tekst na koniec bloku (z pustą linią separującą jeśli blok niepusty).
    pub fn append_block(&self, label: &str, content: &str) -> anyhow::Result<String> {
        let existing = self.load_block(label);
        let new_content = if existing.is_empty() {
            content.trim().to_string()
        } else {
            format!("{existing}\n\n{}", content.trim())
        };
        if new_content.len() > BLOCK_CHAR_LIMIT {
            anyhow::bail!(
                "Blok '{label}' po dopisaniu przekroczy limit {BLOCK_CHAR_LIMIT} znaków (byłby {len}). Nie dopisano.",
                len = new_content.len()
            );
        }
        self.save_block(label, &new_content)?;
        Ok(new_content)
    }

    /// Podmienia fragment bloku (old_str -> new_str, pierwsze wystąpienie).
    pub fn replace_in_block(&self, label: &str, old_str: &str, new_str: &str) -> anyhow::Result<String> {
        if old_str.is_empty() {
            anyhow::bail!("old_str nie może być pusty w core_memory_replace");
        }
        let existing = self.load_block(label);
        if existing.is_empty() {
            anyhow::bail!("Blok '{label}' jest pusty — nie ma czego zastąpić. Użyj core_memory_append.");
        }
        if !existing.contains(old_str) {
            anyhow::bail!("Nie znaleziono fragmentu w bloku '{label}'.");
        }
        let new_content = existing.replacen(old_str, new_str, 1);
        self.save_block(label, &new_content)?;
        Ok(new_content)
    }

    /// Formatuje wszystkie niepuste bloki do wstrzyknięcia w system prompt.
    /// Format inspirowany Letta — XML-like, blok = label + description + value.
    pub fn inject_into_prompt(&self) -> String {
        let blocks = self.load_all();
        let non_empty: Vec<_> = blocks.iter().filter(|(_, c)| !c.is_empty()).collect();
        if non_empty.is_empty() {
            return String::new();
        }
        let mut out = String::from("\nMEMORY BLOCKS (edytowalne segmenty Twojego system promptu — przepisuj je, by się uczyć):\n");
        for (label, content) in non_empty {
            out.push_str(&format!(
                "<memory_block label=\"{label}\">\n  description: {desc}\n  value:\n{content}\n</memory_block>\n",
                desc = block_description(label),
                content = content
            ));
        }
        out.push_str("\nNarzędzia do edycji pamięci: `core_memory_append(label, content)` oraz `core_memory_replace(label, old_str, new_str)`. Używaj ich, gdy odkryjesz wzorzec, preferencję lub wiedzę o projekcie, którą Twoja przyszła sesja powinna znać od startu. Generalizuj — nie loguj pojedynczych zdarzeń.\n");
        out
    }

    /// Status report dla komendy /memory (podgląd stanu bloków).
    pub fn status_report(&self) -> String {
        let mut out = String::from("🧠 Memory Blocks (Letta-style, always-in-context):\n\n");
        for label in BLOCK_LABELS {
            let content = self.load_block(label);
            let scope = match BlockScope::for_label(label) {
                BlockScope::Global => "global",
                BlockScope::Project => "project",
            };
            let path = self.block_path(label).map(|p| p.display().to_string()).unwrap_or_else(|| "(brak HOME)".to_string());
            if content.is_empty() {
                out.push_str(&format!("  • {label} [{scope}] — pusty\n    ścieżka: {path}\n    opis: {}\n\n", block_description(label)));
            } else {
                let preview: String = content.chars().take(120).collect();
                let suffix = if content.len() > 120 { "..." } else { "" };
                out.push_str(&format!(
                    "  • {label} [{scope}] — {} znaków\n    ścieżka: {path}\n    podgląd: {preview}{suffix}\n\n",
                    content.len()
                ));
            }
        }
        out.push_str("Narzędzia agenta: core_memory_append / core_memory_replace / create_skill\n");
        out.push_str("Komendy: /remember (refleksja), /memory (ten podgląd), /memory show <label>, /memory clear <label>, /memory push|pull|status (git sync), /doctor (audyt)\n");
        out
    }

    // ─── /doctor: audyt jakości pamięci ───────────────────────────────────

    /// Audytuje jakość memory blocks — wykrywa duplikaty, puste bloki, nadmierny rozmiar,
    /// brak newline'ów, potencjalne sekrety. Zwraca raport z sugestiami (jak Letta /doctor).
    pub fn doctor_report(&self) -> String {
        let mut findings: Vec<String> = Vec::new();
        let mut ok_count = 0usize;

        for label in BLOCK_LABELS {
            let content = self.load_block(label);
            if content.is_empty() {
                findings.push(format!("⚠️  Blok '{label}' jest pusty — agent nie uczy się w tym obszarze. Użyj /remember by zacząć."));
                continue;
            }

            let len = content.len();
            let scope = match BlockScope::for_label(label) {
                BlockScope::Global => "global",
                BlockScope::Project => "project",
            };

            // 1. Rozmiar — ostrzeżenie przy >80% limitu
            if len > BLOCK_CHAR_LIMIT * 4 / 5 {
                findings.push(format!(
                    "⚠️  Blok '{label}' [{}] zajmuje {}/{} znaków ({}%) — zbliża się do limitu. Skonsoliduj lub przenieś do learned skill.",
                    scope, len, BLOCK_CHAR_LIMIT, len * 100 / BLOCK_CHAR_LIMIT
                ));
            }

            // 2. Duplikaty linii — wykryj powtarzające się zdania
            let lines: Vec<&str> = content.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
            let mut seen = std::collections::HashMap::new();
            let mut dupes = Vec::new();
            for line in &lines {
                let key = line.to_lowercase();
                let count = seen.entry(key).or_insert(0u32);
                *count += 1;
                if *count == 2 {
                    dupes.push(line.chars().take(80).collect::<String>());
                }
            }
            if !dupes.is_empty() {
                findings.push(format!(
                    "⚠️  Blok '{label}' ma {} zduplikowanych linii — skonsoliduj (np. core_memory_replace). Przykłady: {}",
                    dupes.len(),
                    dupes.iter().take(3).map(|d| format!("'{}...'", d)).collect::<Vec<_>>().join(", ")
                ));
            }

            // 3. Potencjalne sekrety — proste heurystyki (nigdy nie loguj w treści bloku)
            let lower = content.to_lowercase();
            let secret_patterns = ["api_key", "apikey", "sk-ant", "sk-", "ghp_", "gho_", "password", "secret", "token=", "bearer "];
            let found_secrets: Vec<&str> = secret_patterns.iter()
                .filter(|p| lower.contains(*p))
                .copied()
                .collect();
            if !found_secrets.is_empty() {
                findings.push(format!(
                    "🚨 KRYTYCZNE: Blok '{label}' zawiera potencjalne sekrety (wzorce: {}). Natychmiast wyczyść: /memory clear {label}",
                    found_secrets.join(", ")
                ));
            }

            // 4. Brak struktury — bardzo długi blok bez nagłówków/list
            if len > 500 && !content.contains('#') && !content.contains('-') && !content.contains('*') {
                findings.push(format!(
                    "💡 Blok '{label}' jest długi ({} znaków) bez struktury (brak #/list). Rozważ dodanie nagłówków markdown dla czytelności agenta.",
                    len
                ));
            }

            if findings.iter().all(|f| !f.contains(&format!("'{label}'"))) {
                ok_count += 1;
                findings.push(format!("✅ Blok '{label}' [{}] — {} znaków, OK", scope, len));
            }
        }

        // 5. Learned skills — czy agent uczy się skilli?
        let learned = crate::skills::SkillsManager::new(self.work_dir.clone()).list_learned_skills();
        if learned.is_empty() {
            findings.push("💡 Brak learned skilli — po skończeniu złożonego zadania użyj /skill-learn by agent zapisał procedurę do .opencode/skills/learned/.".to_string());
        } else {
            findings.push(format!("✅ Learned skilli: {} (w .opencode/skills/learned/)", learned.len()));
        }

        let header = if ok_count == BLOCK_LABELS.len() && !learned.is_empty() {
            "🩺 /doctor — pamięć w dobrej kondycji:\n\n"
        } else {
            "🩺 /doctor — znaleziono problemy do poprawy:\n\n"
        };
        format!("{header}{}\n\nPodsumowanie: {ok_count}/{} bloków OK, {} learned skilli.", findings.join("\n"), BLOCK_LABELS.len(), learned.len())
    }

    // ─── MemFS git sync ───────────────────────────────────────────────────

    /// Katalog pamięci globalnej (gwarantuje istnienie).
    fn ensure_global_memory_dir() -> anyhow::Result<PathBuf> {
        let dir = Self::global_memory_dir()
            .ok_or_else(|| anyhow::anyhow!("Nie można ustalić katalogu pamięci globalnej (brak HOME)?"))?;
        fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Inicjalizuje repo git w ~/.opencode/memory jeśli jeszcze nie ma.
    /// Zwraca true jeśli nowo utworzono, false jeśli już istniało.
    pub fn git_init_memory() -> anyhow::Result<bool> {
        let dir = Self::ensure_global_memory_dir()?;
        let git_dir = dir.join(".git");
        if git_dir.exists() {
            return Ok(false);
        }
        let out = std::process::Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()?;
        if !out.status.success() {
            anyhow::bail!("git init failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        // .gitignore — nie śledź potencjalnych sekretów
        fs::write(dir.join(".gitignore"), "*.secret\n*.key\n.env\n")?;
        Ok(true)
    }

    /// Status repo pamięci (czy są niecommitowane zmiany).
    pub fn git_status_memory() -> anyhow::Result<String> {
        Self::git_init_memory()?;
        let dir = Self::global_memory_dir().unwrap();
        let out = std::process::Command::new("git")
            .args(["status", "--short", "--porcelain"])
            .current_dir(&dir)
            .output()?;
        let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if s.is_empty() {
            Ok("czysty (brak zmian)".to_string())
        } else {
            Ok(s)
        }
    }

    /// Commit + push pamięci do zdalnego repo ( wymaga ustawionego remote przez /memory remote).
    pub fn git_push_memory(message: &str) -> anyhow::Result<String> {
        Self::git_init_memory()?;
        let dir = Self::global_memory_dir().unwrap();

        // add --all
        let add = std::process::Command::new("git")
            .args(["add", "--all"])
            .current_dir(&dir)
            .output()?;
        if !add.status.success() {
            anyhow::bail!("git add: {}", String::from_utf8_lossy(&add.stderr));
        }

        // commit (allow-empty=false — jeśli nic się nie zmieniło, poinformuj)
        let msg = if message.is_empty() { "opencode-rs memory sync" } else { message };
        let commit = std::process::Command::new("git")
            .args(["commit", "-m", msg])
            .current_dir(&dir)
            .output()?;
        let commit_out = String::from_utf8_lossy(&commit.stdout).to_string();
        let commit_err = String::from_utf8_lossy(&commit.stderr).to_string();
        if !commit.status.success() {
            // "nothing to commit" — OK, nie błąd
            if commit_err.contains("nothing to commit") || commit_out.contains("nothing to commit") {
                return Ok("nic do commitowania (brak zmian od ostatniego sync)".to_string());
            }
            anyhow::bail!("git commit: {commit_err}");
        }

        // push (jeśli jest remote)
        let remote = std::process::Command::new("git")
            .args(["remote"])
            .current_dir(&dir)
            .output()?;
        let remotes = String::from_utf8_lossy(&remote.stdout).trim().to_string();
        if remotes.is_empty() {
            return Ok(format!("✅ Commit lokalnie (brak remote — ustaw przez /memory remote <url>). Commit:\n{commit_out}"));
        }

        let push = std::process::Command::new("git")
            .args(["push", "-u", "origin", "HEAD"])
            .current_dir(&dir)
            .output()?;
        if !push.status.success() {
            anyhow::bail!("git push: {}", String::from_utf8_lossy(&push.stderr));
        }
        Ok(format!("✅ Commit + push OK.\n{commit_out}"))
    }

    /// Pull pamięci z remote (sync między maszynami).
    pub fn git_pull_memory() -> anyhow::Result<String> {
        Self::git_init_memory()?;
        let dir = Self::global_memory_dir().unwrap();
        let remote = std::process::Command::new("git")
            .args(["remote"])
            .current_dir(&dir)
            .output()?;
        let remotes = String::from_utf8_lossy(&remote.stdout).trim().to_string();
        if remotes.is_empty() {
            anyhow::bail!("Brak remote — ustaw przez /memory remote <url> przed pull.");
        }
        let pull = std::process::Command::new("git")
            .args(["pull", "origin"])
            .current_dir(&dir)
            .output()?;
        let out = String::from_utf8_lossy(&pull.stdout).to_string();
        let err = String::from_utf8_lossy(&pull.stderr).to_string();
        if !pull.status.success() {
            anyhow::bail!("git pull: {err}");
        }
        Ok(out)
    }

    /// Ustawia remote URL dla repo pamięci.
    pub fn git_set_remote(url: &str) -> anyhow::Result<String> {
        Self::git_init_memory()?;
        let dir = Self::global_memory_dir().unwrap();
        // sprawdź czy origin istnieje
        let check = std::process::Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(&dir)
            .output()?;
        let action = if check.status.success() { "set-url" } else { "add" };
        let out = std::process::Command::new("git")
            .args(["remote", action, "origin", url])
            .current_dir(&dir)
            .output()?;
        if !out.status.success() {
            anyhow::bail!("git remote {action}: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(format!("✅ Remote origin → {url}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_work_dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("opencode_mem_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn test_memory_blocks_empty_when_no_files() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        // project jest per-projekt — świeży katalog, zawsze pusty
        assert!(mb.load_block("project").is_empty());
        // persona/human są globalne (~/.opencode/memory) — mogą istnieć z poprzednich sesji,
        // więc nie testujemy ich pustości tutaj (to zależne od stanu HOME).
        // Sprawdzamy tylko że load_block nie panikuje.
        let _ = mb.load_block("persona");
        let _ = mb.load_block("human");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_save_and_load_project_block() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        mb.save_block("project", "Ten projekt używa tokio::spawn dla IO").unwrap();
        let loaded = mb.load_block("project");
        assert_eq!(loaded, "Ten projekt używa tokio::spawn dla IO");
        // Plik powinien być w .opencode/memory/project.md
        let p = dir.join(".opencode").join("memory").join("project.md");
        assert!(p.exists(), "plik bloku project powinien istnieć");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_append_block_with_separator() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        mb.save_block("project", "linia 1").unwrap();
        let after = mb.append_block("project", "linia 2").unwrap();
        assert_eq!(after, "linia 1\n\nlinia 2");
        // Append do pustego bloku nie daje separatora
        let after2 = mb.append_block("project", "linia 3").unwrap();
        assert!(after2.contains("linia 3"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_replace_in_block() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        mb.save_block("project", "używamy unwrap wszędzie").unwrap();
        let after = mb.replace_in_block("project", "unwrap", "expect z msg").unwrap();
        assert_eq!(after, "używamy expect z msg wszędzie");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_replace_not_found_errors() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        mb.save_block("project", "treść").unwrap();
        let res = mb.replace_in_block("project", "nieistnieje", "coś");
        assert!(res.is_err());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_unknown_label_rejected() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        let res = mb.save_block("nieznany", "treść");
        assert!(res.is_err());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_char_limit_enforced() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        let big = "x".repeat(BLOCK_CHAR_LIMIT + 1);
        let res = mb.save_block("project", &big);
        assert!(res.is_err(), "zapis nad limit powinien zwrócić błąd");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_inject_into_prompt_format() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        mb.save_block("project", "wzorzec X").unwrap();
        let injected = mb.inject_into_prompt();
        assert!(injected.contains("MEMORY BLOCKS"));
        assert!(injected.contains("label=\"project\""));
        assert!(injected.contains("wzorzec X"));
        assert!(injected.contains("core_memory_append"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_status_report_lists_all_blocks() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        mb.save_block("human", "user woli polskie komentarze").unwrap();
        let report = mb.status_report();
        assert!(report.contains("persona"));
        assert!(report.contains("human"));
        assert!(report.contains("project"));
        assert!(report.contains("polskie komentarze"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_block_scope_routing() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        // project → per-projekt
        mb.save_block("project", "p").unwrap();
        assert!(dir.join(".opencode").join("memory").join("project.md").exists());
        // persona/human → global (~/.opencode/memory) — nie testujemy zapisu do HOME w unit teście,
        // ale sprawdzamy że scope jest poprawny
        assert_eq!(BlockScope::for_label("persona"), BlockScope::Global);
        assert_eq!(BlockScope::for_label("human"), BlockScope::Global);
        assert_eq!(BlockScope::for_label("project"), BlockScope::Project);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_doctor_detects_empty_blocks() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        let report = mb.doctor_report();
        // Wszystkie bloki puste → powinno zgłosić
        assert!(report.contains("pusty"));
        assert!(report.contains("persona"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_doctor_detects_secrets() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        mb.save_block("project", "Mój API key to sk-ant-12345").unwrap();
        let report = mb.doctor_report();
        assert!(report.contains("KRYTYCZNE") || report.contains("sekret"), "doctor powinien wykryć potencjalny sekret: {report}");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_doctor_detects_duplicates() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        mb.save_block("project", "user woli unwrap\nuser woli unwrap\nuser woli unwrap").unwrap();
        let report = mb.doctor_report();
        assert!(report.contains("zduplikowanych"), "doctor powinien wykryć duplikaty: {report}");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_doctor_ok_for_clean_block() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        mb.save_block("human", "User woli polskie komentarze i unwrap() zamiast match").unwrap();
        let report = mb.doctor_report();
        assert!(report.contains("OK"), "czysty blok powinien być OK: {report}");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_doctor_warns_near_limit() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        let big = "x".repeat(BLOCK_CHAR_LIMIT * 9 / 10); // 90% limitu
        mb.save_block("project", &big).unwrap();
        let report = mb.doctor_report();
        assert!(report.contains("zbliża się do limitu") || report.contains("limitu"), "doctor powinien ostrzec o rozmiarze: {report}");
        fs::remove_dir_all(&dir).ok();
    }
}
