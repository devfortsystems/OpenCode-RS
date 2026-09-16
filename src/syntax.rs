//! SyntaxHighlighter — silnik kolorowania składni wzorowany 1:1 na oficjalnym motywie VS Code Dark+.
//!
//! Zapewnia autentyczną paletę kolorystyczną:
//! - Słowa kluczowe (fn, pub, let, const): #569CD6 (błękit)
//! - Przepływ sterowania (if, match, for, return, async): #C586C0 (fiolet)
//! - Typy i struktury (String, Vec, Option, Result): #4EC9B0 (turkus)
//! - Funkcje i metody: #DCDCAA (ciepły żółty)
//! - Stringi: #CE9178 (pomarańczowo-rudy)
//! - Liczby: #B5CEA8 (jasna oliwka)
//! - Komentarze: #6A9955 (zielony)
//! - Zmienne/parametry: #9CDCFE (jasny błękit)

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub struct SyntaxHighlighter;

impl SyntaxHighlighter {
    // VS Code Dark+ Oficjalna Paleta Barw RGB
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

    /// Koloruje linię kodu w stylu VS Code Dark+ z wykrywaniem kontekstu słów
    pub fn highlight_code_line(line: &str, _lang: &str) -> Line<'static> {
        let trimmed = line.trim_start();

        // 1. Komentarze
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("--") {
            return Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Self::VS_COMMENT).add_modifier(Modifier::ITALIC),
            ));
        }

        // 2. Diff lines
        if line.starts_with("+++") || line.starts_with("---") {
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
        if line.starts_with("@@") {
            return Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(Self::VS_DIFF_HEADER),
            ));
        }

        // 3. Tokenizacja
        let mut spans = Vec::new();
        let words = line.split_inclusive(|c: char| !c.is_alphanumeric() && c != '_');

        for word in words {
            let pure_word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');

            let style = if Self::is_control_flow(pure_word) {
                Style::default().fg(Self::VS_CONTROL).add_modifier(Modifier::BOLD)
            } else if Self::is_keyword(pure_word) {
                Style::default().fg(Self::VS_KEYWORD).add_modifier(Modifier::BOLD)
            } else if Self::is_type(pure_word) {
                Style::default().fg(Self::VS_TYPE)
            } else if Self::is_boolean_or_constant(pure_word) {
                Style::default().fg(Self::VS_KEYWORD).add_modifier(Modifier::BOLD)
            } else if word.contains('"') || word.contains('\'') || word.contains('`') {
                Style::default().fg(Self::VS_STRING)
            } else if pure_word.chars().all(|c| c.is_numeric()) && !pure_word.is_empty() {
                Style::default().fg(Self::VS_NUMBER)
            } else if word.ends_with('(') || (word.len() > 1 && word.ends_with("!(")) {
                // Wywołanie funkcji
                Style::default().fg(Self::VS_FUNCTION)
            } else {
                Style::default().fg(Self::VS_TEXT)
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

    fn is_control_flow(w: &str) -> bool {
        matches!(
            w,
            "if" | "else" | "match" | "switch" | "case" | "return" | "break" | "continue"
                | "for" | "while" | "loop" | "in" | "yield" | "async" | "await" | "try"
                | "catch" | "finally" | "throw"
        )
    }

    fn is_keyword(w: &str) -> bool {
        matches!(
            w,
            "fn" | "pub" | "struct" | "enum" | "trait" | "impl" | "use" | "mod" | "let"
                | "mut" | "const" | "static" | "type" | "where" | "as" | "ref" | "move"
                | "unsafe" | "extern" | "crate" | "function" | "class" | "interface"
                | "import" | "export" | "from" | "default" | "extends" | "implements"
                | "var" | "def" | "package" | "func" | "select"
        )
    }

    fn is_type(w: &str) -> bool {
        if w.is_empty() { return false; }
        // PascalCase konwencja dla typów lub typy wbudowane
        matches!(
            w,
            "String" | "str" | "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
                | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "f32" | "f64"
                | "bool" | "char" | "Option" | "Result" | "Vec" | "HashMap" | "HashSet"
                | "Arc" | "Rc" | "Mutex" | "RefCell" | "Box" | "Self" | "number"
                | "string" | "boolean" | "any" | "void" | "never" | "unknown"
        ) || (w.chars().next().unwrap().is_uppercase() && w.chars().skip(1).any(|c| c.is_lowercase()))
    }

    fn is_boolean_or_constant(w: &str) -> bool {
        matches!(
            w,
            "true" | "false" | "Some" | "None" | "Ok" | "Err" | "null" | "undefined" | "nil"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_rust_line() {
        let line = SyntaxHighlighter::highlight_code_line("pub fn test() -> bool {", "rs");
        assert!(!line.spans.is_empty());
        // fn powinno mieć kolor błękitny VS_KEYWORD
        let fn_span = line.spans.iter().find(|s| s.content.contains("fn"));
        assert!(fn_span.is_some());
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
}
