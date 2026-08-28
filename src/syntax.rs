use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub struct SyntaxHighlighter;

impl SyntaxHighlighter {
    /// Koloruje linie kodu w zależności od wykrytego języka i słów kluczowych
    pub fn highlight_code_line(line: &str, _lang: &str) -> Line<'static> {
        let trimmed = line.trim_start();
        
        // Komentarze
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("--") {
            return Line::from(Span::styled(line.to_string(), Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)));
        }

        // Diff lines
        if line.starts_with('+') {
            return Line::from(Span::styled(line.to_string(), Style::default().fg(Color::LightGreen)));
        }
        if line.starts_with('-') {
            return Line::from(Span::styled(line.to_string(), Style::default().fg(Color::LightRed)));
        }

        let mut spans = Vec::new();
        let words = line.split_inclusive(|c: char| !c.is_alphanumeric() && c != '_');

        for word in words {
            let pure_word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');

            let style = match pure_word {
                // Słowa kluczowe Rust / JS / Python / Go
                "fn" | "pub" | "struct" | "enum" | "impl" | "use" | "mod" | "let" | "mut" | "const" | "match" | "if" | "else" | "return" | "async" | "await" | "for" | "while" | "loop"
                | "function" | "import" | "export" | "from" | "class" | "interface" | "type" | "def" | "package" | "SELECT" | "FROM" | "WHERE" | "INSERT" | "UPDATE" => {
                    Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)
                }
                // Typy wbudowane
                "String" | "str" | "i32" | "i64" | "u32" | "u64" | "usize" | "bool" | "Option" | "Result" | "Vec" | "Self" | "number" | "string" | "boolean" | "any" => {
                    Style::default().fg(Color::LightYellow)
                }
                // Wartości logiczne i stałe
                "true" | "false" | "Some" | "None" | "Ok" | "Err" | "null" | "undefined" | "nil" => {
                    Style::default().fg(Color::Cyan)
                }
                _ => {
                    if word.contains('"') || word.contains('\'') {
                        Style::default().fg(Color::Green)
                    } else if pure_word.chars().all(|c| c.is_numeric()) && !pure_word.is_empty() {
                        Style::default().fg(Color::LightCyan)
                    } else {
                        Style::default().fg(Color::White)
                    }
                }
            };

            spans.push(Span::styled(word.to_string(), style));
        }

        if spans.is_empty() {
            Line::from(Span::styled(line.to_string(), Style::default().fg(Color::White)))
        } else {
            Line::from(spans)
        }
    }
}
