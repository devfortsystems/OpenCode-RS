//! SyntaxHighlighter — silnik kolorowania składni oparty w 100% na architekturze Visual Studio Code.
//!
//! Wykorzystuje:
//! 1. `syntect` — silnik TextMate grammars (identyczny jak w VS Code i Sublime Text)
//! 2. Oficjalną paletę barw **VS Code Dark+**:
//!    - Słowa kluczowe (fn, pub, let, const): #569CD6 (błękit)
//!    - Przepływ sterowania (if, match, for, return, async): #C586C0 (fiolet)
//!    - Typy i struktury (String, Vec, Option, Result): #4EC9B0 (turkus)
//!    - Funkcje i metody: #DCDCAA (ciepły żółty)
//!    - Stringi: #CE9178 (pomarańczowo-rudy)
//!    - Liczby: #B5CEA8 (jasna oliwka)
//!    - Komentarze: #6A9955 (zielony)
//!    - Zmienne i właściwości: #9CDCFE (jasny błękit)
//! 3. `syntect-tui` — bezstratną konwersję tokenów TextMate na `ratatui::text::Span` i `Line`
//! 4. Szybkie wykrywanie diffów Git (+/-) w stylu GitHub / VS Code diff editor

use std::str::FromStr;
use std::sync::LazyLock;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use syntect::easy::HighlightLines;
use syntect::highlighting::{
    Color as SynColor, FontStyle, ScopeSelectors, StyleModifier, Theme, ThemeItem, ThemeSettings,
};
use syntect::parsing::{SyntaxReference, SyntaxSet};

/// Singleton zestawu gramatyk TextMate (wbudowane 50+ języków)
pub static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);

/// Aktywny motyw TextMate (domyślnie oficjalny VS Code Dark+, konfigurowalny przez motywy VSIX)
pub static CURRENT_THEME: LazyLock<std::sync::RwLock<Theme>> = LazyLock::new(|| std::sync::RwLock::new(create_vscode_dark_theme()));

pub struct SyntaxHighlighter;

impl SyntaxHighlighter {
    // VS Code Dark+ Oficjalna Paleta Barw RGB (dla elementów UI i diffów)
    pub const VS_KEYWORD: Color = Color::Rgb(86, 156, 214);      // #569CD6
    pub const VS_CONTROL: Color = Color::Rgb(197, 134, 192);     // #C586C0
    pub const VS_TYPE: Color = Color::Rgb(78, 201, 176);         // #4EC9B0
    pub const VS_FUNCTION: Color = Color::Rgb(220, 220, 170);    // #DCDCAA
    pub const VS_STRING: Color = Color::Rgb(206, 145, 120);      // #CE9178
    pub const VS_NUMBER: Color = Color::Rgb(181, 206, 168);      // #B5CEA8
    pub const VS_COMMENT: Color = Color::Rgb(106, 153, 85);      // #6A9955
    pub const VS_VARIABLE: Color = Color::Rgb(156, 220, 254);    // #9CDCFE
    pub const VS_TEXT: Color = Color::Rgb(212, 212, 212);        // #D4D4D4
    pub const VS_LINE_NUM: Color = Color::Rgb(110, 118, 129);    // #6E7681

    pub const VS_DIFF_ADD: Color = Color::Rgb(46, 160, 67);      // GitHub/VS Code Green
    pub const VS_DIFF_DEL: Color = Color::Rgb(248, 81, 73);      // GitHub/VS Code Red
    pub const VS_DIFF_HEADER: Color = Color::Rgb(121, 192, 255); // Cyan header

    /// Wyszukuje gramatykę TextMate po rozszerzeniu lub nazwie języka
    pub fn find_syntax(lang: &str) -> &'static SyntaxReference {
        let clean = lang.trim().to_lowercase();
        if clean.is_empty() {
            return SYNTAX_SET.find_syntax_plain_text();
        }

        // 1. Sprawdzenie po rozszerzeniu pliku
        if let Some(s) = SYNTAX_SET.find_syntax_by_extension(&clean) {
            return s;
        }

        // 2. Sprawdzenie po tokenie/nazwie (np. "rust", "python", "javascript")
        if let Some(s) = SYNTAX_SET.find_syntax_by_token(&clean) {
            return s;
        }

        // 3. Popularne aliasy
        let mapped = match clean.as_str() {
            "rs" | "rust" => "rs",
            "py" | "python" | "py3" => "py",
            "js" | "javascript" | "jsx" | "node" => "js",
            "ts" | "typescript" | "tsx" => "js", // TextMate fallback dla TS
            "c" | "h" => "c",
            "cpp" | "c++" | "cc" | "cxx" | "hpp" => "cpp",
            "cs" | "csharp" => "cs",
            "go" | "golang" => "go",
            "java" => "java",
            "php" => "php",
            "rb" | "ruby" => "rb",
            "sh" | "bash" | "zsh" | "shell" => "sh",
            "json" | "jsonc" => "json",
            "toml" => "toml",
            "yaml" | "yml" => "yaml",
            "html" | "htm" => "html",
            "css" => "css",
            "sql" => "sql",
            "md" | "markdown" => "md",
            "diff" | "patch" => "diff",
            _ => "",
        };

        if !mapped.is_empty() {
            if let Some(s) = SYNTAX_SET.find_syntax_by_extension(mapped) {
                return s;
            }
        }

        SYNTAX_SET.find_syntax_plain_text()
    }

    /// Koloruje pojedynczą linię kodu w stylu VS Code Dark+ z zerowym narzutem (nano-lexer bez regexów)
    pub fn highlight_code_line(line: &str, _lang: &str) -> Line<'static> {
        let trimmed = line.trim_start();

        // 1. Szybka obsługa diffów git (GitHub / VS Code diff)
        if line.starts_with("+++") || line.starts_with("---") || line.starts_with("@@") {
            return Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Self::VS_DIFF_HEADER).add_modifier(Modifier::BOLD),
            ));
        }
        if line.starts_with('+') {
            return Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Self::VS_DIFF_ADD),
            ));
        }
        if line.starts_with('-') {
            return Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Self::VS_DIFF_DEL),
            ));
        }

        // 2. Komentarze jednoliniowe: #6A9955
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("--") {
            return Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Self::VS_COMMENT).add_modifier(Modifier::ITALIC),
            ));
        }

        // 3. Błyskawiczny nano-lexer słownikowy dla palety VS Code Dark+
        let mut spans = Vec::new();
        let words = line.split_inclusive(|c: char| !c.is_alphanumeric() && c != '_');

        for word in words {
            let pure_word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');

            let style = match pure_word {
                // Słowa kluczowe definicji i deklaracji: #569CD6
                "fn" | "pub" | "struct" | "enum" | "impl" | "use" | "mod" | "let" | "mut" | "const"
                | "function" | "class" | "interface" | "type" | "def" | "package" | "import" | "export"
                | "from" | "trait" | "SELECT" | "FROM" | "WHERE" | "INSERT" | "UPDATE" | "DELETE" => {
                    Style::default().fg(Self::VS_KEYWORD)
                }
                // Przepływ sterowania (Control Flow): #C586C0
                "if" | "else" | "match" | "return" | "async" | "await" | "for" | "while" | "loop"
                | "break" | "continue" | "try" | "catch" | "finally" | "throw" | "switch" | "case" => {
                    Style::default().fg(Self::VS_CONTROL)
                }
                // Typy i struktury wbudowane: #4EC9B0
                "String" | "str" | "i32" | "i64" | "u32" | "u64" | "usize" | "isize" | "f32" | "f64"
                | "bool" | "Option" | "Result" | "Vec" | "Self" | "Box" | "Arc" | "Rc" | "Mutex"
                | "number" | "string" | "boolean" | "any" | "void" | "int" | "float" | "double" => {
                    Style::default().fg(Self::VS_TYPE)
                }
                // Stałe logiczne i literały: #569CD6
                "true" | "false" | "Some" | "None" | "Ok" | "Err" | "null" | "undefined" | "nil" => {
                    Style::default().fg(Self::VS_KEYWORD)
                }
                _ => {
                    // Stringi: #CE9178
                    if word.contains('"') || word.contains('\'') || word.contains('`') {
                        Style::default().fg(Self::VS_STRING)
                    // Liczby: #B5CEA8
                    } else if pure_word.chars().all(|c| c.is_numeric()) && !pure_word.is_empty() {
                        Style::default().fg(Self::VS_NUMBER)
                    // Wywołania funkcji / metod: #DCDCAA
                    } else if word.ends_with('(') && !pure_word.is_empty() {
                        Style::default().fg(Self::VS_FUNCTION)
                    // Domyślny tekst: #D4D4D4
                    } else {
                        Style::default().fg(Self::VS_TEXT)
                    }
                }
            };

            spans.push(Span::styled(word.to_string(), style));
        }

        if spans.is_empty() {
            Line::from(Span::styled(line.to_string(), Style::default().fg(Self::VS_TEXT)))
        } else {
            Line::from(spans)
        }
    }

    /// Koloruje linię kodu z numerem linii w stylu edytora VS Code
    pub fn highlight_editor_line(line_num: usize, line: &str, lang: &str) -> Line<'static> {
        let num_span = Span::styled(
            format!("{:>4} │ ", line_num),
            Style::default().fg(Self::VS_LINE_NUM),
        );
        let mut highlighted = Self::highlight_code_line(line, lang);
        highlighted.spans.insert(0, num_span);
        highlighted
    }

    /// Ustawia aktywny motyw TextMate
    pub fn set_active_theme(theme: Theme) {
        if let Ok(mut lock) = CURRENT_THEME.write() {
            *lock = theme;
        }
    }

    /// Pobiera nazwę aktualnie aktywnego motywu
    pub fn current_theme_name() -> String {
        let lock = CURRENT_THEME.read().unwrap_or_else(|e| e.into_inner());
        lock.name.clone().unwrap_or_else(|| "VS Code Dark+".to_string())
    }

    /// Ładuje i aktywuje motyw z pliku (.tmTheme lub VS Code theme JSON z wtyczki VSIX)
    pub fn load_theme_from_file(path: &std::path::Path) -> anyhow::Result<String> {
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
        if ext == "tmtheme" {
            let file = std::fs::File::open(path)?;
            let mut reader = std::io::BufReader::new(file);
            let theme = syntect::highlighting::ThemeSet::load_from_reader(&mut reader)
                .map_err(|e| anyhow::anyhow!("Błąd parsowania pliku .tmTheme: {e:?}"))?;
            let name = theme.name.clone().unwrap_or_else(|| {
                path.file_stem().unwrap_or_default().to_string_lossy().to_string()
            });
            Self::set_active_theme(theme);
            Ok(name)
        } else {
            // Format JSON motywu VS Code (contributes.themes)
            let content = std::fs::read_to_string(path)?;
            let clean_json = crate::importer::strip_json_comments(&content);
            let val: serde_json::Value = serde_json::from_str(&clean_json)
                .map_err(|e| anyhow::anyhow!("Błąd parsowania JSON motywu VS Code: {e}"))?;

            let mut theme = create_vscode_dark_theme();
            if let Some(name) = val.get("name").and_then(|v| v.as_str()) {
                theme.name = Some(name.to_string());
            }

            if let Some(tokens) = val.get("tokenColors").and_then(|v| v.as_array()) {
                for item in tokens {
                    let scopes: Vec<String> = if let Some(s) = item.get("scope").and_then(|v| v.as_str()) {
                        vec![s.to_string()]
                    } else if let Some(arr) = item.get("scope").and_then(|v| v.as_array()) {
                        arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
                    } else {
                        Vec::new()
                    };

                    if let Some(settings) = item.get("settings") {
                        if let Some(hex) = settings.get("foreground").and_then(|v| v.as_str()) {
                            if let Some(color) = parse_hex_color(hex) {
                                for sc in scopes {
                                    if let Ok(sel) = ScopeSelectors::from_str(&sc) {
                                        theme.scopes.push(ThemeItem {
                                            scope: sel,
                                            style: StyleModifier {
                                                foreground: Some(color),
                                                background: None,
                                                font_style: Some(FontStyle::empty()),
                                            },
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let name = theme.name.clone().unwrap_or_else(|| {
                path.file_stem().unwrap_or_default().to_string_lossy().to_string()
            });
            Self::set_active_theme(theme);
            Ok(name)
        }
    }

    /// Koloruje cały wieloliniowy blok kodu z zachowaniem pełnego kontekstu składni (np. wieloliniowe komentarze i stringi)
    pub fn highlight_block(source: &str, lang: &str) -> Vec<Line<'static>> {
        let syntax = Self::find_syntax(lang);
        let theme_guard = CURRENT_THEME.read().unwrap_or_else(|e| e.into_inner());
        let mut highlighter = HighlightLines::new(syntax, &*theme_guard);
        let mut lines = Vec::new();

        for raw_line in syntect::util::LinesWithEndings::from(source) {
            let line_no_nl = raw_line.trim_end_matches(['\r', '\n']);

            // Obsługa diffów git w bloku
            if line_no_nl.starts_with("+++") || line_no_nl.starts_with("---") {
                lines.push(Line::from(Span::styled(
                    line_no_nl.to_string(),
                    Style::default().fg(Self::VS_DIFF_HEADER).add_modifier(Modifier::BOLD),
                )));
                continue;
            }
            if line_no_nl.starts_with('+') {
                lines.push(Line::from(Span::styled(
                    line_no_nl.to_string(),
                    Style::default().fg(Self::VS_DIFF_ADD),
                )));
                continue;
            }
            if line_no_nl.starts_with('-') {
                lines.push(Line::from(Span::styled(
                    line_no_nl.to_string(),
                    Style::default().fg(Self::VS_DIFF_DEL),
                )));
                continue;
            }
            if line_no_nl.starts_with("@@") {
                lines.push(Line::from(Span::styled(
                    line_no_nl.to_string(),
                    Style::default().fg(Self::VS_DIFF_HEADER),
                )));
                continue;
            }

            match highlighter.highlight_line(raw_line, &SYNTAX_SET) {
                Ok(ranges) => {
                    let mut spans = Vec::new();
                    for segment in ranges {
                        let text = segment.1.trim_end_matches(['\r', '\n']);
                        if text.is_empty() {
                            continue;
                        }
                        if let Ok(span) = syntect_tui::into_span((segment.0, text)) {
                            spans.push(Span::styled(span.content.into_owned(), span.style));
                        } else {
                            spans.push(Span::raw(text.to_string()));
                        }
                    }
                    if spans.is_empty() {
                        lines.push(Line::from(Span::styled(line_no_nl.to_string(), Style::default().fg(Self::VS_TEXT))));
                    } else {
                        lines.push(Line::from(spans));
                    }
                }
                Err(_) => {
                    lines.push(Line::from(Span::styled(line_no_nl.to_string(), Style::default().fg(Self::VS_TEXT))));
                }
            }
        }

        lines
    }
}

/// Tworzy motyw w 100% wiernie odwzorowujący oficjalny **VS Code Dark+** dla TextMate Scope Selectors.
fn create_vscode_dark_theme() -> Theme {
    let mut theme = Theme {
        name: Some("VS Code Dark+".to_string()),
        author: Some("Microsoft / OpenCode-RS".to_string()),
        settings: ThemeSettings {
            foreground: Some(SynColor { r: 212, g: 212, b: 212, a: 255 }), // #D4D4D4
            accent: Some(SynColor { r: 0, g: 122, b: 204, a: 255 }),
            ..Default::default()
        },
        scopes: Vec::new(),
    };

    let items = [
        // 1. Słowa kluczowe (fn, pub, let, const, import, export): #569CD6
        ("keyword, storage, storage.type, storage.modifier", 86, 156, 214, FontStyle::empty()),

        // 2. Przepływ sterowania (if, else, match, return, for, while, async, await, try, catch): #C586C0
        ("keyword.control, keyword.control.flow, keyword.control.import, keyword.control.directive", 197, 134, 192, FontStyle::empty()),

        // 3. Typy i struktury (String, Vec, Option, Result, Class, Struct, Interface): #4EC9B0
        ("entity.name.type, entity.name.class, entity.name.struct, entity.name.enum, entity.name.trait, entity.name.interface, support.type, support.class", 78, 201, 176, FontStyle::empty()),

        // 4. Funkcje, metody i makra: #DCDCAA
        ("entity.name.function, support.function, entity.name.macro, meta.function-call, entity.name.method", 220, 220, 170, FontStyle::empty()),

        // 5. Stringi i znaki: #CE9178
        ("string, string.quoted, string.template, string.quoted.double, string.quoted.single", 206, 145, 120, FontStyle::empty()),

        // 6. Liczby: #B5CEA8
        ("constant.numeric, constant.numeric.integer, constant.numeric.float, constant.numeric.hex", 181, 206, 168, FontStyle::empty()),

        // 7. Stałe językowe (true, false, null, None, Some, Ok, Err): #569CD6
        ("constant.language, constant.character", 86, 156, 214, FontStyle::empty()),

        // 8. Komentarze: #6A9955 (italic)
        ("comment, comment.line, comment.block, comment.documentation", 106, 153, 85, FontStyle::ITALIC),

        // 9. Zmienne, właściwości i parametry: #9CDCFE
        ("variable, variable.parameter, variable.other, variable.other.property, support.variable, meta.property.object", 156, 220, 254, FontStyle::empty()),

        // 10. Tagi HTML / XML: #569CD6
        ("entity.name.tag", 86, 156, 214, FontStyle::empty()),

        // 11. Atrybuty HTML / XML: #9CDCFE
        ("entity.other.attribute-name", 156, 220, 254, FontStyle::empty()),

        // 12. Wyrażenia regularne (Regex): #D16969
        ("string.regexp", 209, 105, 105, FontStyle::empty()),

        // 13. Operatory: #D4D4D4
        ("keyword.operator", 212, 212, 212, FontStyle::empty()),
    ];

    for (scope_str, r, g, b, font_style) in items {
        if let Ok(selectors) = ScopeSelectors::from_str(scope_str) {
            theme.scopes.push(ThemeItem {
                scope: selectors,
                style: StyleModifier {
                    foreground: Some(SynColor { r, g, b, a: 255 }),
                    background: None,
                    font_style: Some(font_style),
                },
            });
        }
    }

    theme
}

/// Pomocnicze parsowanie koloru hex (#RRGGBB lub #RRGGBBAA)
fn parse_hex_color(hex: &str) -> Option<SynColor> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(SynColor { r, g, b, a: 255 })
    } else if hex.len() == 8 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
        Some(SynColor { r, g, b, a })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_rust_line() {
        let line = SyntaxHighlighter::highlight_code_line("pub fn test() -> bool {", "rs");
        assert!(!line.spans.is_empty());
        // Upewnij się, że "fn" lub "pub" zostało pokolorowane w stylu VS Code
        let has_keyword = line.spans.iter().any(|s| {
            s.style.fg == Some(SyntaxHighlighter::VS_KEYWORD) || s.style.fg == Some(SyntaxHighlighter::VS_CONTROL)
        });
        assert!(has_keyword, "Powinno znaleźć słowo kluczowe z palety VS Code");
    }

    #[test]
    fn test_highlight_python_line() {
        let line = SyntaxHighlighter::highlight_code_line("def hello_world(name: str):", "py");
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_highlight_diff_lines() {
        let add = SyntaxHighlighter::highlight_code_line("+added line", "");
        assert_eq!(add.spans[0].style.fg, Some(SyntaxHighlighter::VS_DIFF_ADD));

        let del = SyntaxHighlighter::highlight_code_line("-removed line", "");
        assert_eq!(del.spans[0].style.fg, Some(SyntaxHighlighter::VS_DIFF_DEL));
    }

    #[test]
    fn test_highlight_editor_line_has_line_number() {
        let line = SyntaxHighlighter::highlight_editor_line(42, "let x = 10;", "rs");
        assert!(line.spans[0].content.contains("42"));
    }

    #[test]
    fn test_highlight_block_multiline() {
        let code = r#"
// To jest komentarz
pub struct Point {
    pub x: f64,
    pub y: f64,
}
"#;
        let lines = SyntaxHighlighter::highlight_block(code, "rs");
        assert!(lines.len() >= 5);
    }

    #[test]
    fn test_highlight_various_languages() {
        // JavaScript / TypeScript
        let js = SyntaxHighlighter::highlight_code_line("const compute = (x) => x * 2;", "js");
        assert!(!js.spans.is_empty());

        // JSON
        let json = SyntaxHighlighter::highlight_code_line("{\"status\": \"ok\", \"code\": 200}", "json");
        assert!(!json.spans.is_empty());

        // SQL
        let sql = SyntaxHighlighter::highlight_code_line("SELECT id, name FROM users WHERE active = 1;", "sql");
        assert!(!sql.spans.is_empty());

        // HTML
        let html = SyntaxHighlighter::highlight_code_line("<div class=\"container\">Hello</div>", "html");
        assert!(!html.spans.is_empty());

        // Shell
        let sh = SyntaxHighlighter::highlight_code_line("echo \"Witaj $USER\"", "sh");
        assert!(!sh.spans.is_empty());

        // C / C++
        let c = SyntaxHighlighter::highlight_code_line("int main(void) { return 0; }", "c");
        assert!(!c.spans.is_empty());
    }
}

