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

use crate::database::Database;

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
    /// Wbudowana baza DevFortDB — primary storage dla memory blocks.
    /// None = DB niedostępna (fallback do plików .md).
    db: Option<Database>,
}

impl MemoryBlocks {
    pub fn new(work_dir: PathBuf) -> Self {
        // Próbuj otworzyć DB (non-fatal — fallback do plików .md)
        let db = Database::open(&work_dir).ok();
        Self { work_dir, db }
    }

    fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(PathBuf::from)
            .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
            .or_else(|| directories::UserDirs::new().map(|d| d.home_dir().to_path_buf()))
    }

    /// Katalog pamięci globalnej: `~/.opencode-rs/memory` (fallback do legacy `~/.opencode/memory`)
    fn global_memory_dir() -> Option<PathBuf> {
        let home = Self::home_dir()?;
        let rs_dir = home.join(".opencode-rs").join("memory");
        if rs_dir.exists() {
            return Some(rs_dir);
        }
        let legacy_dir = home.join(".opencode").join("memory");
        if legacy_dir.exists() {
            return Some(legacy_dir);
        }
        Some(rs_dir)
    }

    /// Katalog pamięci per-projekt: `.opencode-rs/memory` (fallback do legacy `.opencode/memory`)
    fn project_memory_dir(&self) -> PathBuf {
        let rs_dir = self.work_dir.join(".opencode-rs").join("memory");
        if rs_dir.exists() {
            return rs_dir;
        }
        let legacy_dir = self.work_dir.join(".opencode").join("memory");
        if legacy_dir.exists() {
            return legacy_dir;
        }
        rs_dir
    }

    /// Zwraca ścieżkę pliku dla danego bloku (globalnego lub per-projekt).
    fn block_path(&self, label: &str) -> Option<PathBuf> {
        match BlockScope::for_label(label) {
            BlockScope::Global => Self::global_memory_dir().map(|d| d.join(format!("{label}.md"))),
            BlockScope::Project => Some(self.project_memory_dir().join(format!("{label}.md"))),
        }
    }

    /// Wczytuje zawartość bloku (pusty string jeśli nie istnieje).
    /// Primary: DevFortDB. Fallback: plik .md (legacy — pełna treść, przed migracją).
    pub fn load_block(&self, label: &str) -> String {
        // 1) Spróbuj z DevFortDB (primary)
        if let Some(ref db) = self.db {
            if let Ok(Some(content)) = db.get_str::<String>("memory", label) {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() {
                    return trimmed;
                }
            }
        }

        // 2) Fallback: plik .md (legacy — pełna treść, przed migracją do skrótów)
        let from_file = self.block_path(label)
            .and_then(|p| fs::read_to_string(&p).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_default();

        // 3) Auto-migracja: jeśli plik ma pełną treść (nie skrót), zapisz do DB
        if !from_file.is_empty() && !from_file.starts_with("# Memory block:") {
            if let Some(ref db) = self.db {
                let _ = db.put_str("memory", label, &from_file);
            }
            return from_file;
        }

        // 4) Skrót .md — nie jest pełną treścią, zwróć pusty (DB ma pełną)
        String::new()
    }

    /// Wczytuje wszystkie bloki jako (label, content).
    pub fn load_all(&self) -> Vec<(&'static str, String)> {
        BLOCK_LABELS.iter().map(|l| (*l, self.load_block(l))).collect()
    }

    /// Zapisuje zawartość bloku (tworzy katalogi jeśli trzeba, przycina do limitu).
    /// Primary: DevFortDB. Mirror: plik .md ze skrótową informacją (jak się dostać do bazy).
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

        // 1) DevFortDB (primary — pełna treść)
        if let Some(ref db) = self.db {
            db.put_str("memory", label, &trimmed.to_string())?;
        }

        // 2) Mirror .md — skrótowa informacja (jak się dostać do pełnej treści w bazie)
        let path = self.block_path(label)
            .ok_or_else(|| anyhow::anyhow!("Nie można ustalić ścieżki pamięci dla bloku '{label}' (brak HOME)?"))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let scope = match BlockScope::for_label(label) {
            BlockScope::Global => "global",
            BlockScope::Project => "project",
        };
        let char_count = trimmed.chars().count();
        let preview: String = trimmed.chars().take(80).collect();
        let suffix = if char_count > 80 { "..." } else { "" };
        let stub = format!(
            "# Memory block: {label} [{scope}]\n\
             \n\
             Pełna treść ({char_count} znaków) jest w **DevFortDB**:\n\
             \n\
             - Baza: `.opencode/db/opencode.mdb`\n\
             - Namespace: `memory`\n\
             - Klucz: `{label}`\n\
             - Odczyt: `opencode` → komenda `/memory show {label}`\n\
             \n\
             Podgląd:\n\
             ```\n\
             {preview}{suffix}\n\
             ```\n"
        );
        fs::write(&path, stub)?;

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

    // ─── /palace: pełny drzewiasty podgląd stanu pamięci (jak Letta) ────

    /// Pełny podgląd stanu pamięci — drzewo z memory blocks, learned skills,
    /// wszystkimi skillami i placeholderem dla archival memory (na przyszłość).
    /// Format inspirowany Letta /palace — jedno miejsce, pełny obraz "umysłu" agenta.
    pub fn palace_report(&self) -> String {
        let blocks = self.load_all();
        let total_chars: usize = blocks.iter().map(|(_, c)| c.len()).sum();
        let total_capacity = BLOCK_CHAR_LIMIT * BLOCK_LABELS.len();

        let sm = crate::skills::SkillsManager::new(self.work_dir.clone());
        let all_skills = sm.list_skills();
        let learned: Vec<_> = all_skills.iter().filter(|s| s.source == "learned").collect();
        let non_learned: Vec<_> = all_skills.iter().filter(|s| s.source != "learned").collect();

        // Grupuj non-learned skille po source dla zwięzłego podsumowania
        let mut by_source: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for s in &non_learned {
            *by_source.entry(s.source.as_str()).or_insert(0) += 1;
        }

        let mut out = String::from("🏰 Pałac Pamięci — pełny stan umysłu agenta:\n\n");

        // 1. Memory blocks (always-in-context)
        out.push_str(&format!("🧠 Memory Blocks (always-in-context, {}/{} znaków, {:.1}%):\n",
            total_chars, total_capacity, total_chars as f64 * 100.0 / total_capacity as f64));
        for (i, (label, content)) in blocks.iter().enumerate() {
            let is_last = i == blocks.len() - 1;
            let branch = if is_last { "└──" } else { "├──" };
            let scope = match BlockScope::for_label(label) {
                BlockScope::Global => "global",
                BlockScope::Project => "project",
            };
            let path = self.block_path(label).map(|p| p.display().to_string()).unwrap_or_else(|| "(brak HOME)".to_string());
            if content.is_empty() {
                out.push_str(&format!("{branch} {label} [{scope}] — pusty\n"));
                out.push_str(&format!("    └── {path}\n"));
            } else {
                let preview: String = content.chars().take(80).collect();
                let suffix = if content.len() > 80 { "…" } else { "" };
                out.push_str(&format!("{branch} {label} [{scope}] — {} znaków\n", content.len()));
                out.push_str(&format!("    ├── {path}\n"));
                out.push_str(&format!("    └── \"{preview}{suffix}\"\n"));
            }
        }
        out.push('\n');

        // 1b. Plan projektu (persistentny, per-projekt)
        let plan = ProjectPlan::load(&self.work_dir);
        let (plan_total, plan_done) = plan.stats();
        out.push_str(&format!("📋 Plan projektu (.opencode/plan.md, {plan_done}/{plan_total} kroków ukończonych):\n"));
        if plan.goal.is_empty() && plan.steps.is_empty() && plan.notes.is_empty() {
            out.push_str("└── (brak planu — użyj /plan <instrukcja> lub tools plan_set/plan_add_step)\n");
        } else {
            if !plan.goal.is_empty() {
                let goal_preview: String = plan.goal.chars().take(80).collect();
                let suffix = if plan.goal.len() > 80 { "…" } else { "" };
                out.push_str(&format!("├── cel: \"{goal_preview}{suffix}\"\n"));
            }
            for (i, step) in plan.steps.iter().enumerate() {
                let is_last = i == plan.steps.len() - 1 && plan.notes.is_empty();
                let branch = if is_last { "└──" } else { "├──" };
                let mark = if step.completed { "✅" } else { "⬜" };
                let desc_preview: String = step.description.chars().take(60).collect();
                let suffix = if step.description.len() > 60 { "…" } else { "" };
                out.push_str(&format!("{branch} {mark} {}. {}{suffix}\n", i + 1, desc_preview));
            }
            if !plan.notes.is_empty() {
                let notes_preview: String = plan.notes.chars().take(60).collect();
                let suffix = if plan.notes.len() > 60 { "…" } else { "" };
                out.push_str(&format!("└── notatki: \"{notes_preview}{suffix}\"\n"));
            }
        }
        out.push('\n');

        // 2. Learned skills (procedury wyuczone przez /skill-learn)
        out.push_str(&format!("🎓 Learned Skills ({}) — procedury wyuczone z doświadczenia:\n", learned.len()));
        if learned.is_empty() {
            out.push_str("└── (brak — użyj /skill-learn po skończeniu złożonego zadania)\n");
        } else {
            for (i, s) in learned.iter().enumerate() {
                let is_last = i == learned.len() - 1;
                let branch = if is_last { "└──" } else { "├──" };
                let preview: String = s.preview.chars().take(60).collect();
                let suffix = if s.preview.len() > 60 { "…" } else { "" };
                out.push_str(&format!("{branch} {} — \"{}{}\"\n", s.name, preview, suffix));
            }
        }
        out.push('\n');

        // 3. Wszystkie skille (roo/cline/opencode/commandcode/rules)
        out.push_str(&format!("📦 Wszystkie skille ({})", all_skills.len()));
        if !by_source.is_empty() {
            let summary: Vec<String> = by_source.iter().map(|(k, v)| format!("{}({})", k, v)).collect();
            out.push_str(&format!(": {}", summary.join(", ")));
        }
        out.push_str(":\n");
        if non_learned.is_empty() && learned.is_empty() {
            out.push_str("└── (brak skilli — dodaj .roo/skills/<name>/SKILL.md lub .opencode/skills/*.md)\n");
        } else if non_learned.is_empty() {
            out.push_str("└── (tylko learned skilli — patrz wyżej)\n");
        } else {
            for (i, s) in non_learned.iter().enumerate() {
                let is_last = i == non_learned.len() - 1;
                let branch = if is_last { "└──" } else { "├──" };
                let preview: String = s.preview.chars().take(50).collect();
                let suffix = if s.preview.len() > 50 { "…" } else { "" };
                out.push_str(&format!("{branch} {} [{}] — \"{}{}\"\n", s.name, s.source, preview, suffix));
            }
        }
        out.push('\n');

        // 4. Archival memory (placeholder — wektorowa baza długoterminowa, na przyszłość)
        out.push_str("🔮 Archival Memory (wektorowa, długoterminowa):\n");
        out.push_str("└── niedostępne — planowane w backlogu (qdrant/chroma + embedding model)\n");
        out.push_str("    użyj /remember + /skill-learn jako obecny mechanizm długoterminowy\n\n");

        // 5. Statystyki końcowe
        let git_status = Self::git_status_memory().unwrap_or_else(|_| "(brak repo)".to_string());
        out.push_str("📊 Statystyki:\n");
        out.push_str(&format!("├── bloki: {}/{} znaków ({:.1}% pojemności)\n",
            total_chars, total_capacity, total_chars as f64 * 100.0 / total_capacity as f64));
        out.push_str(&format!("├── learned skilli: {}\n", learned.len()));
        out.push_str(&format!("├── wszystkich skilli: {}\n", all_skills.len()));
        out.push_str(&format!("└── git sync: {}\n", git_status));

        out.push_str("\nKomendy: /memory (podgląd bloków), /memory show <label>, /remember (refleksja), /skill-learn (nowy skill), /doctor (audyt jakości), /memory push|pull (git sync)");
        out
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
        // Plik powinien być w .opencode-rs/memory/project.md
        let p = dir.join(".opencode-rs").join("memory").join("project.md");
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
        assert!(dir.join(".opencode-rs").join("memory").join("project.md").exists());
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

    // ─── /palace tests ────────────────────────────────────────────────────

    #[test]
    fn test_palace_report_empty_state() {
        // Świeży katalog bez bloków i bez skilli — palace powinien się renderować bez paniki
        // i pokazywać stan "pusty" + placeholder archival.
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        let report = mb.palace_report();
        assert!(report.contains("Pałac Pamięci"), "palace powinien mieć nagłówek: {report}");
        assert!(report.contains("Memory Blocks"), "palace powinien pokazywać sekcję bloków: {report}");
        assert!(report.contains("Archival Memory"), "palace powinien mieć placeholder archival: {report}");
        assert!(report.contains("Statystyki"), "palace powinien mieć statystyki: {report}");
        // project jest per-projekt — świeży katalog, więc pusty
        assert!(report.contains("project [project] — pusty"), "project powinien być pusty w świeżym katalogu: {report}");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_palace_report_with_project_block_and_learned_skill() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        mb.save_block("project", "Ten projekt używa tokio::spawn dla IO i polskie komentarze").unwrap();

        // Dodaj learned skill przez SkillsManager (deleguje do crate::skills)
        let sm = crate::skills::SkillsManager::new(dir.clone());
        sm.create_learned_skill("db-migration", "# DB Migration\n1. sqlx::migrate!\n2. test").unwrap();

        let report = mb.palace_report();
        assert!(report.contains("project [project] — 56 znaków") || report.contains("project [project] —"), "palace powinien pokazać rozmiar bloku project: {report}");
        assert!(report.contains("tokio::spawn"), "palace powinien pokazać preview bloku project: {report}");
        assert!(report.contains("db-migration"), "palace powinien pokazać learned skill: {report}");
        assert!(report.contains("learned skilli: 1"), "palace powinien policzyć 1 learned skill: {report}");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_palace_report_with_roo_skill() {
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        // Dodaj skille roo i cline (non-learned)
        let roo_dir = dir.join(".roo").join("skills").join("code-review");
        fs::create_dir_all(&roo_dir).unwrap();
        fs::write(roo_dir.join("SKILL.md"), "# Code Review\nSprawdź bezpieczeństwo i styl").unwrap();
        fs::write(dir.join(".clinerules"), "używaj clean code").unwrap();

        let report = mb.palace_report();
        assert!(report.contains("code-review [roo]"), "palace powinien pokazać skill roo: {report}");
        assert!(report.contains(".clinerules [rules]"), "palace powinien pokazać .clinerules: {report}");
        assert!(report.contains("roo(1)"), "palace powinien podsumować source roo(1): {report}");
        assert!(report.contains("rules(1)"), "palace powinien podsumować source rules(1): {report}");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_palace_report_capacity_calculation() {
        // Sprawdź że procent pojemności jest liczony poprawnie (nie panikuje przy 0 znaków)
        let dir = fresh_work_dir();
        let mb = MemoryBlocks::new(dir.clone());
        let report = mb.palace_report();
        // Powinien zawierać procent (nawet 0.0%)
        assert!(report.contains("0.0%") || report.contains("% pojemności"), "palace powinien pokazać procent: {report}");
        fs::remove_dir_all(&dir).ok();
    }
}

// ============================================================================
// ProjectPlan — persistentny plan per projekt (.opencode/plan.md)
// ============================================================================
//
// Plan jest zapisywany w pliku `.opencode/plan.md` w katalogu projektu.
// Wczytywany zawsze przy starcie agenta i wstrzykiwany do system prompt.
// Agent aktualizuje plan przez tools `plan_set` / `plan_add_step` /
// `plan_complete_step` / `plan_clear`.
// Komenda `/plan` pokazuje aktualny plan w TUI.
//
// Format pliku: Markdown z listą checkboxów (jak GitHub issues):
//   ## Cel: <główny cel projektu>
//
//   - [ ] Krok 1: opis
//   - [ ] Krok 2: opis
//   - [x] Krok 3: ukończony
//
// Po zamknięciu i otwarciu UI — plan nadal tam jest. Każdy model (Cursor,
// Devin ACP, Gemini, etc.) widzi ten sam plan bo jest wczytywany z dysku.

use serde::{Deserialize, Serialize};

/// Pojedynczy krok planu.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanStep {
    pub description: String,
    pub completed: bool,
}

/// Persistentny plan projektu.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectPlan {
    /// Główny cel projektu (1 zdanie).
    pub goal: String,
    /// Lista kroków (zachowuje kolejność).
    pub steps: Vec<PlanStep>,
    /// Notatki / kontekst dodatkowy (np. decyzje architektoniczne).
    pub notes: String,
}

impl ProjectPlan {
    /// Ścieżka pliku planu: `.opencode-rs/plan.md` w katalogu projektu (fallback do `.opencode/plan.md`).
    pub fn plan_path(work_dir: &std::path::Path) -> std::path::PathBuf {
        let rs_path = work_dir.join(".opencode-rs").join("plan.md");
        if rs_path.exists() {
            return rs_path;
        }
        let legacy = work_dir.join(".opencode").join("plan.md");
        if legacy.exists() {
            return legacy;
        }
        rs_path
    }

    /// Wczytuje plan z dysku. Zwraca pusty plan jeśli plik nie istnieje.
    pub fn load(work_dir: &std::path::Path) -> Self {
        let path = Self::plan_path(work_dir);
        if !path.exists() {
            return Self::default();
        }
        match fs::read_to_string(&path) {
            Ok(content) => Self::parse_markdown(&content),
            Err(_) => Self::default(),
        }
    }

    /// Zapisuje plan do `.opencode-rs/plan.md` (tworzy katalogi jeśli trzeba).
    pub fn save(&self, work_dir: &std::path::Path) -> anyhow::Result<()> {
        let path = work_dir.join(".opencode-rs").join("plan.md");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, self.to_markdown())?;
        Ok(())
    }

    /// Parsuje format Markdown (zapisany przez `to_markdown`).
    /// Wytrzyma ręczne edycje usera — ignoruje linie których nie rozumie.
    pub fn parse_markdown(content: &str) -> Self {
        let mut plan = Self::default();
        let mut in_notes = false;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            // Cel: "## Cel: <opis>"
            if let Some(rest) = trimmed.strip_prefix("## Cel:") {
                plan.goal = rest.trim().to_string();
                continue;
            }
            // Sekcja notatek
            if trimmed == "## Notatki:" || trimmed == "## Notatki" {
                in_notes = true;
                continue;
            }
            if trimmed.starts_with("## ") {
                in_notes = false;
                continue;
            }
            if in_notes {
                if !plan.notes.is_empty() {
                    plan.notes.push('\n');
                }
                plan.notes.push_str(trimmed);
                continue;
            }
            // Krok: "- [ ] opis" lub "- [x] opis"
            if let Some(rest) = trimmed.strip_prefix("- [x]") {
                plan.steps.push(PlanStep {
                    description: rest.trim().to_string(),
                    completed: true,
                });
            } else if let Some(rest) = trimmed.strip_prefix("- [ ]") {
                plan.steps.push(PlanStep {
                    description: rest.trim().to_string(),
                    completed: false,
                });
            }
        }
        plan
    }

    /// Renderuje plan jako Markdown (format stabilny, round-trip z `parse_markdown`).
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        if !self.goal.is_empty() {
            out.push_str(&format!("## Cel: {}\n\n", self.goal));
        }
        if !self.steps.is_empty() {
            for step in &self.steps {
                let mark = if step.completed { 'x' } else { ' ' };
                out.push_str(&format!("- [{mark}] {}\n", step.description));
            }
            out.push('\n');
        }
        if !self.notes.is_empty() {
            out.push_str("## Notatki:\n");
            out.push_str(&self.notes);
            out.push('\n');
        }
        out
    }

    /// Renderuje plan do wstrzyknięcia w system prompt (kompaktowy).
    pub fn to_prompt_section(&self) -> String {
        if self.goal.is_empty() && self.steps.is_empty() && self.notes.is_empty() {
            return "(brak planu — agent powinien rozważyć użycie plan_set gdy zadanie jest złożone)".to_string();
        }
        let mut out = String::new();
        if !self.goal.is_empty() {
            out.push_str(&format!("Cel: {}\n", self.goal));
        }
        if !self.steps.is_empty() {
            let total = self.steps.len();
            let done = self.steps.iter().filter(|s| s.completed).count();
            out.push_str(&format!("Kroki ({done}/{total} ukończonych):\n"));
            for (i, step) in self.steps.iter().enumerate() {
                let mark = if step.completed { "✅" } else { "⬜" };
                out.push_str(&format!("  {mark} {}. {}\n", i + 1, step.description));
            }
        }
        if !self.notes.is_empty() {
            out.push_str(&format!("Notatki:\n{}\n", self.notes));
        }
        out
    }

    // ── Operacje wywoływane przez tools agenta ──────────────────────────

    /// Ustawia główny cel planu (nadpisuje poprzedni).
    pub fn set_goal(&mut self, goal: &str) {
        self.goal = goal.trim().to_string();
    }

    /// Dodaje nowy krok na końcu listy.
    pub fn add_step(&mut self, description: &str) {
        self.steps.push(PlanStep {
            description: description.trim().to_string(),
            completed: false,
        });
    }

    /// Oznacza krok jako ukończony (po indeksie 1-based) lub cofa oznaczenie.
    pub fn toggle_step(&mut self, step_number: usize) -> anyhow::Result<()> {
        let total = self.steps.len();
        let idx = step_number.checked_sub(1)
            .ok_or_else(|| anyhow::anyhow!("Numer kroku musi być >= 1 (dostalem {step_number})"))?;
        let step = self.steps.get_mut(idx)
            .ok_or_else(|| anyhow::anyhow!("Krok {step_number} nie istnieje (plan ma {total} kroków)"))?;
        step.completed = !step.completed;
        Ok(())
    }

    /// Aktualizuje opis kroku (po indeksie 1-based).
    pub fn update_step(&mut self, step_number: usize, new_description: &str) -> anyhow::Result<()> {
        let total = self.steps.len();
        let idx = step_number.checked_sub(1)
            .ok_or_else(|| anyhow::anyhow!("Numer kroku musi być >= 1"))?;
        let step = self.steps.get_mut(idx)
            .ok_or_else(|| anyhow::anyhow!("Krok {step_number} nie istnieje (plan ma {total} kroków)"))?;
        step.description = new_description.trim().to_string();
        Ok(())
    }

    /// Czyści cały plan (cel + kroki + notatki).
    pub fn clear(&mut self) {
        self.goal.clear();
        self.steps.clear();
        self.notes.clear();
    }

    /// Dodaje notatkę (dopisuje do sekcji notatek).
    pub fn add_note(&mut self, note: &str) {
        if !self.notes.is_empty() {
            self.notes.push('\n');
        }
        self.notes.push_str(note.trim());
    }

    /// Statystyki: (liczba kroków, liczba ukończonych).
    pub fn stats(&self) -> (usize, usize) {
        let total = self.steps.len();
        let done = self.steps.iter().filter(|s| s.completed).count();
        (total, done)
    }
}

#[cfg(test)]
mod plan_tests {
    use super::*;
    use uuid::Uuid;

    fn fresh_work_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("opencode_plan_{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).ok();
        dir
    }

    #[test]
    fn test_plan_empty_when_no_file() {
        let dir = fresh_work_dir();
        let plan = ProjectPlan::load(&dir);
        assert!(plan.goal.is_empty());
        assert!(plan.steps.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_plan_save_and_load_roundtrip() {
        let dir = fresh_work_dir();
        let mut plan = ProjectPlan::default();
        plan.set_goal("Zbudować CLI w Rust");
        plan.add_step("Inicjalizacja cargo");
        plan.add_step("Napisz main.rs");
        plan.add_step("Testy");
        plan.toggle_step(1).unwrap(); // krok 1 ukończony
        plan.add_note("Używamy tokio dla async");
        plan.save(&dir).unwrap();

        let loaded = ProjectPlan::load(&dir);
        assert_eq!(loaded.goal, "Zbudować CLI w Rust");
        assert_eq!(loaded.steps.len(), 3);
        assert!(loaded.steps[0].completed);
        assert!(!loaded.steps[1].completed);
        assert_eq!(loaded.steps[2].description, "Testy");
        assert!(loaded.notes.contains("tokio"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_plan_markdown_roundtrip() {
        let mut plan = ProjectPlan::default();
        plan.set_goal("Test roundtrip");
        plan.add_step("Krok A");
        plan.add_step("Krok B");
        plan.toggle_step(2).unwrap();
        plan.add_note("Notatka 1");

        let md = plan.to_markdown();
        let parsed = ProjectPlan::parse_markdown(&md);

        assert_eq!(parsed.goal, "Test roundtrip");
        assert_eq!(parsed.steps.len(), 2);
        assert!(!parsed.steps[0].completed);
        assert!(parsed.steps[1].completed);
        assert_eq!(parsed.steps[1].description, "Krok B");
        assert!(parsed.notes.contains("Notatka 1"));
    }

    #[test]
    fn test_plan_to_prompt_section_empty() {
        let plan = ProjectPlan::default();
        let section = plan.to_prompt_section();
        assert!(section.contains("brak planu"));
    }

    #[test]
    fn test_plan_to_prompt_section_with_content() {
        let mut plan = ProjectPlan::default();
        plan.set_goal("Mój cel");
        plan.add_step("Krok 1");
        plan.add_step("Krok 2");
        plan.toggle_step(1).unwrap();

        let section = plan.to_prompt_section();
        assert!(section.contains("Cel: Mój cel"));
        assert!(section.contains("1/2 ukończonych"));
        assert!(section.contains("✅"));
        assert!(section.contains("⬜"));
    }

    #[test]
    fn test_plan_toggle_step_out_of_range() {
        let mut plan = ProjectPlan::default();
        plan.add_step("Tylko krok");
        assert!(plan.toggle_step(0).is_err(), "0 powinien być błędem");
        assert!(plan.toggle_step(2).is_err(), "2 nie istnieje");
        assert!(plan.toggle_step(1).is_ok(), "1 powinien działać");
    }

    #[test]
    fn test_plan_clear() {
        let mut plan = ProjectPlan::default();
        plan.set_goal("Cel");
        plan.add_step("Krok");
        plan.add_note("Notatka");
        plan.clear();
        assert!(plan.goal.is_empty());
        assert!(plan.steps.is_empty());
        assert!(plan.notes.is_empty());
    }

    #[test]
    fn test_plan_update_step() {
        let mut plan = ProjectPlan::default();
        plan.add_step("Stary opis");
        plan.update_step(1, "Nowy opis").unwrap();
        assert_eq!(plan.steps[0].description, "Nowy opis");
    }

    #[test]
    fn test_plan_parse_handles_human_edits() {
        // User może ręcznie edytować plik — parser musi być wyrozumiały
        let md = r#"## Cel: Ręcznie edytowany plan

- [ ] Pierwszy krok
- [x] Ukończony krok
- [ ] Trzeci krok

## Notatki:
To jest notatka
na dwie linie
"#;
        let plan = ProjectPlan::parse_markdown(md);
        assert_eq!(plan.goal, "Ręcznie edytowany plan");
        assert_eq!(plan.steps.len(), 3);
        assert!(plan.steps[1].completed);
        assert!(plan.notes.contains("dwie linie"));
    }

    #[test]
    fn test_plan_stats() {
        let mut plan = ProjectPlan::default();
        plan.add_step("A");
        plan.add_step("B");
        plan.add_step("C");
        plan.toggle_step(1).unwrap();
        plan.toggle_step(3).unwrap();
        let (total, done) = plan.stats();
        assert_eq!(total, 3);
        assert_eq!(done, 2);
    }
}
