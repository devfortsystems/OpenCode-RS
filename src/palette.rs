#[derive(Debug, Clone)]
pub struct PaletteItem {
    pub command: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub shortcut: &'static str,
}

pub struct CommandPalette;

impl CommandPalette {
    pub fn get_all() -> Vec<PaletteItem> {
        vec![
            // 🛠️ Git & Środowiska
            PaletteItem {
                command: "/branch",
                category: "🛠️ Git",
                description: "Przełączanie lub tworzenie branchy git",
                shortcut: "",
            },
            PaletteItem {
                command: "/target",
                category: "⚙️ Środowiska",
                description: "Wybór środowiska docelowego (dev/test/prod) i .env",
                shortcut: "",
            },
            PaletteItem {
                command: "/env",
                category: "⚙️ Runtime",
                description: "Środowisko wykonawcze: Host / WSL / Docker kontener",
                shortcut: "",
            },
            PaletteItem {
                command: "/docker",
                category: "⚙️ Runtime",
                description: "Zarządzanie i inspekcja kontenerów Docker",
                shortcut: "",
            },
            PaletteItem {
                command: "/wsl",
                category: "⚙️ Runtime",
                description: "Lista zainstalowanych dystrybucji WSL",
                shortcut: "",
            },
            PaletteItem {
                command: "/scp",
                category: "📤 Transfer",
                description: "Transfer plików SSH: upload/download/ls (wymaga /ssh)",
                shortcut: "",
            },
            PaletteItem {
                command: "/dropzone",
                category: "📤 Transfer",
                description: "Skanuj dropzone (~/.opencode-rs/dropzone/) i dołącz pliki do czatu",
                shortcut: "",
            },
            PaletteItem {
                command: "/commit",
                category: "🛠️ Git",
                description: "Automatyczny commit z opisem wygenerowanym przez AI",
                shortcut: "",
            },
            PaletteItem {
                command: "/review",
                category: "🛠️ Git",
                description: "Audyt bezpieczeństwa i jakości niezatwierdzonego kodu",
                shortcut: "",
            },
            PaletteItem {
                command: "/undo",
                category: "🛠️ Git",
                description: "Bezpieczne cofnięcie ostatnich zmian (git stash backup)",
                shortcut: "",
            },
            PaletteItem {
                command: "/diff",
                category: "🛠️ Git",
                description: "Podgląd zmian roboczych w projekcie (git diff HEAD)",
                shortcut: "",
            },

            // 🖥️ Tryb IDE i Edycja
            PaletteItem {
                command: "/ide",
                category: "🖥️ IDE",
                description: "Przełącz tryb IDE: Drzewo | Edytor kodu (VS Code Dark+) | Czat",
                shortcut: "F3",
            },
            PaletteItem {
                command: "/repomap",
                category: "🗺️ Kod",
                description: "Generuj mapę kodu AST (RepoMap) mieszczącą się w <1500 tokenów",
                shortcut: "",
            },
            PaletteItem {
                command: "/copy",
                category: "📋 Schowek",
                description: "Kopiuj ostatni wygenerowany kod AI do schowka systemowego",
                shortcut: "",
            },
            PaletteItem {
                command: "/compact",
                category: "🧹 Kontekst",
                description: "Skompresuj historię rozmowy i zwolnij tokeny context window",
                shortcut: "",
            },
            PaletteItem {
                command: "/vsix",
                category: "📦 Wtyczki",
                description: "Zarządzanie wtyczkami Visual Studio Code (/vsix install, /vsix list)",
                shortcut: "",
            },

            // 📁 Pliki i Eksploracja
            PaletteItem {
                command: "/files",
                category: "📁 Pliki",
                description: "Dwupanelowy Eksplorator Plików (Tab: zmiana panelu, Spacja: wklej)",
                shortcut: "Ctrl+E",
            },
            PaletteItem {
                command: "/export-md",
                category: "📁 Pliki",
                description: "Eksport całej rozmowy do czytelnego raportu Markdown",
                shortcut: "",
            },

            // ⚡ Szablony Zadań
            PaletteItem {
                command: "/refactor",
                category: "⚡ Szablony",
                description: "Refaktoryzacja wskazanego pliku lub fragmentu",
                shortcut: "",
            },
            PaletteItem {
                command: "/tests",
                category: "⚡ Szablony",
                description: "Napisanie kompleksowego zestawu testów jednostkowych",
                shortcut: "",
            },
            PaletteItem {
                command: "/doc",
                category: "⚡ Szablony",
                description: "Generowanie dokumentacji i komentarzy do kodu",
                shortcut: "",
            },
            PaletteItem {
                command: "/explain",
                category: "⚡ Szablony",
                description: "Szczegółowe wyjaśnienie działania wybranego kodu",
                shortcut: "",
            },

            // 🌐 Narzędzia i Wyszukiwanie
            PaletteItem {
                command: "/search",
                category: "🌐 Narzędzia",
                description: "Szybkie przeszukiwanie internetu w terminalu",
                shortcut: "",
            },
            PaletteItem {
                command: "/mcp",
                category: "🌐 Narzędzia",
                description: "Podgląd i stan podłączonych serwerów MCP",
                shortcut: "",
            },

            // ⚙️ Konfiguracja i Tryby
            PaletteItem {
                command: "/mode",
                category: "⚙️ Ustawienia",
                description: "Przełączenie trybu agenta: Coder / Architect / Ask",
                shortcut: "Ctrl+T",
            },
            PaletteItem {
                command: "/autocheck",
                category: "⚙️ Ustawienia",
                description: "Włącz/wyłącz automatyczne sprawdzanie kompilacji",
                shortcut: "",
            },
            PaletteItem {
                command: "/model",
                category: "⚙️ Ustawienia",
                description: "Wybór aktywnego operatora / modelu AI",
                shortcut: "Ctrl+M",
            },
            PaletteItem {
                command: "/new",
                category: "⚙️ Ustawienia",
                description: "Rozpocznij nową sesję czatu",
                shortcut: "Ctrl+N",
            },
            PaletteItem {
                command: "/sessions",
                category: "⚙️ Ustawienia",
                description: "Przeglądaj i wczytuj historię sesji tego projektu",
                shortcut: "Ctrl+H",
            },
            PaletteItem {
                command: "/auth",
                category: "🔑 Autoryzacja",
                description: "Zarządzanie autoryzacją i 1 plikiem .env",
                shortcut: "",
            },
            PaletteItem {
                command: "/opencode",
                category: "🔑 Autoryzacja",
                description: "Synchronizacja i odczyt z oryginalnego OpenCode",
                shortcut: "",
            },
            PaletteItem {
                command: "/agents",
                category: "🤖 OpenCode/CommandCode",
                description: "Lista agentów (.opencode/agents + .commandcode/agents)",
                shortcut: "",
            },
            PaletteItem {
                command: "/commands",
                category: "🤖 OpenCode/CommandCode",
                description: "Lista komend (.opencode/commands + .commandcode/commands)",
                shortcut: "",
            },
            PaletteItem {
                command: "/mods",
                category: "🤖 OpenCode/CommandCode",
                description: "Lista modów CommandCode (.commandcode/mods/*.ts)",
                shortcut: "",
            },
            PaletteItem {
                command: "/plugins",
                category: "🤖 OpenCode/CommandCode",
                description: "Lista pluginów opencode (.opencode-rs/plugins/*.js|ts)",
                shortcut: "",
            },
            PaletteItem {
                command: "/compat",
                category: "🤖 OpenCode/CommandCode",
                description: "Raport kompatybilności opencode + commandcode",
                shortcut: "",
            },
            PaletteItem {
                command: "/keybinds",
                category: "🤖 OpenCode/CommandCode",
                description: "Konfigurowalne skróty klawiszowe (tui.json)",
                shortcut: "",
            },
            PaletteItem {
                command: "/format",
                category: "🤖 OpenCode/CommandCode",
                description: "Sformatuj plik (rustfmt/gofmt/prettier/black/clang-format)",
                shortcut: "",
            },
            PaletteItem {
                command: "/sync",
                category: "☁️ Chmura",
                description: "Synchronizacja sesji ze zdalnym serwerem (push/pull)",
                shortcut: "",
            },
            PaletteItem {
                command: "/clear",
                category: "⚙️ Ustawienia",
                description: "Wyczyść bieżący widok czatu",
                shortcut: "Ctrl+L",
            },
            PaletteItem {
                command: "/config",
                category: "⚙️ Ustawienia",
                description: "Pokaż stan konfiguracji i ścieżki magazynu",
                shortcut: "",
            },
            PaletteItem {
                command: "/help",
                category: "⚙️ Ustawienia",
                description: "Pomoc i pełna lista skrótów klawiszowych",
                shortcut: "",
            },
            PaletteItem {
                command: "/taste",
                category: "🧠 Taste-1",
                description: "CommandCode Taste — enable/disable/push/pull/list (npx taste)",
                shortcut: "",
            },
            PaletteItem {
                command: "/skills",
                category: "📦 Skills",
                description: "Roo/Cline/CommandCode skills — lista i podgląd .roo/skills",
                shortcut: "",
            },
            PaletteItem {
                command: "/memories",
                category: "🧠 Memories",
                description: "Windsurf/Trae memories — podgląd .windsurf/memories.md",
                shortcut: "",
            },
            PaletteItem {
                command: "/build",
                category: "🏗️ Trae BUILD",
                description: "Trae BUILD mode — zbuduj cały projekt od 0 (solver)",
                shortcut: "",
            },
            PaletteItem {
                command: "/preview",
                category: "🌐 Preview",
                description: "Webview preview Vite/Next.js (Trae-like)",
                shortcut: "",
            },
            PaletteItem {
                command: "/interactive",
                category: "⚡ Tryby",
                description: "Tryb Interactive (Cline-like) — potwierdzenia inline diff",
                shortcut: "Ctrl+T",
            },
            PaletteItem {
                command: "/auto",
                category: "⚡ Tryby",
                description: "Tryb Auto (Unattended) — pełna autonomia trust_mode=ON",
                shortcut: "Ctrl+T",
            },
        ]
    }

    pub fn filter(query: &str) -> Vec<PaletteItem> {
        let q = query.trim().to_lowercase();
        let q = q.trim_start_matches('/');

        let mut all = Self::get_all();
        if q.is_empty() {
            return all;
        }

        all.retain(|item| {
            item.command.to_lowercase().contains(q)
                || item.description.to_lowercase().contains(q)
                || item.category.to_lowercase().contains(q)
        });

        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_palette_get_all_and_taste() {
        let all = CommandPalette::get_all();
        assert!(all.len() >= 25, "palette should have 25+ items, got {}", all.len());
        assert!(all.iter().any(|i| i.command=="/taste"));
        assert!(all.iter().any(|i| i.command=="/skills"));
        assert!(all.iter().any(|i| i.command=="/build"));
    }
    #[test]
    fn test_palette_filter() {
        let f = CommandPalette::filter("taste");
        assert!(f.iter().any(|i| i.command=="/taste"));
        let all = CommandPalette::filter("");
        assert_eq!(all.len(), CommandPalette::get_all().len());
        let none = CommandPalette::filter("zzz_nope");
        assert!(none.is_empty());
    }
}
