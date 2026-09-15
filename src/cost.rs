pub struct CostEstimator;

/// Lepszy estymator tokenów niż `chars / 4`.
///
/// BPE (Byte Pair Encoding) — używane przez GPT/Claude/Gemini — tokenizuje:
/// - Angielski tekst: ~4 znaki/token
/// - Kod (Rust/TS/Python): ~3.2 znaki/token (więcej symboli, krótsze sub-words)
/// - JSON/structured: ~3.5 znaki/token
/// - Polski tekst: ~3.5 znaki/token (znaki diakrytyczne = multi-byte)
/// - Markdown: ~3.8 znaki/token
///
/// `chars / 4` niedoszacowuje kod o ~20%. Używamy heurystyki:
/// - Jeśli content ma dużo symboli ({}()[];=<>), użyj ~3.2
/// - Jeśli dużo liter (prose), użyj ~4.0
/// - W przeciwnym razie ~3.6
pub struct TokenEstimator;

impl TokenEstimator {
    /// Szacuje liczbę tokenów BPE na podstawie treści.
    /// Lepsze niż `chars / 4` — wykrywa kod vs tekst.
    pub fn estimate(content: &str) -> usize {
        if content.is_empty() {
            return 0;
        }

        let chars = content.chars().count();
        // Policz "code-like" znaki: symboli, interpunkcję, cyfry
        let code_chars = content.chars()
            .filter(|c| {
                matches!(c, '{' | '}' | '(' | ')' | '[' | ']' | ';' | '=' | '<' | '>' | '|' | '&' | '!' | '?' | ':' | '/' | '\\' | '*' | '+' | '-' | '%' | '@' | '$' | '`' | '"' | '\'')
                    || c.is_ascii_digit()
            })
            .count();

        // Ratio code_chars / total — jeśli >25%, to kod
        let code_ratio = code_chars as f64 / chars as f64;

        // Chars per token:
        // - code_ratio > 0.25 → kod → ~3.2 chars/token
        // - code_ratio < 0.08 → proza → ~4.0 chars/token
        // - middle → ~3.6 chars/token
        let chars_per_token = if code_ratio > 0.25 {
            3.2
        } else if code_ratio < 0.08 {
            4.0
        } else {
            3.6
        };

        (chars as f64 / chars_per_token).round() as usize
    }

    /// Szacuje tokeny na podstawie rozmiaru pliku (bez czytania treści).
    /// Szybsze niż czytanie + estymacja — używa metadane filesystem.
    /// - Pliki kodu (.rs, .ts, .py, .js): ~3.2 chars/token
    /// - Pliki tekstowe (.md, .txt): ~4.0 chars/token
    /// - Pliki JSON/YAML: ~3.5 chars/token
    /// - Inne: ~3.6 chars/token
    pub fn estimate_from_file_size(path: &str, size_bytes: usize) -> usize {
        if size_bytes == 0 {
            return 0;
        }

        let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
        let chars_per_token = match ext.as_str() {
            // Kod — dużo symboli
            "rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "c" | "cpp" | "h" | "java" | "kt" | "swift" | "rb" | "php" | "css" | "scss" => 3.2,
            // JSON/YAML/TOML — structured
            "json" | "yaml" | "yml" | "toml" | "xml" | "html" | "svg" => 3.5,
            // Tekst/proza
            "md" | "txt" | "rst" | "log" => 4.0,
            // Domyślne
            _ => 3.6,
        };

        // size_bytes ≈ chars dla ASCII, ~1.5x dla UTF-8 z diakrytykami
        // BPE liczy po znakach, nie bajtach — dla ASCII 1:1, dla UTF-8 ~0.7
        let est_chars = if size_bytes > 0 {
            // Szacuj chars z bytes — dla kodu ASCII (większość), 1:1
            // Dla bezpieczeństwa użyj 0.85 ratio (pokrywa UTF-8 z marginesem)
            (size_bytes as f64 * 0.85) as usize
        } else {
            0
        };

        (est_chars as f64 / chars_per_token).round() as usize
    }

    /// Szacuje tokeny dla całej listy wiadomości (system + user + assistant + tool results).
    /// Lepsze niż sumowanie `content.len() / 4` — używa per-message estymacji.
    pub fn estimate_messages(messages: &[crate::providers::ChatMessage]) -> usize {
        messages.iter().map(|m| Self::estimate(&m.content)).sum()
    }
}

impl CostEstimator {
    /// Oblicza szacowany koszt sesji w USD na podstawie użytego modelu i liczby tokenów.
    /// UWAGA: `total_tokens` to liczba TOKENÓW (nie znaków).
    pub fn estimate_cost_from_tokens(model_id: &str, total_tokens: usize) -> f64 {
        let millions = total_tokens as f64 / 1_000_000.0;

        let rate_per_million = if model_id.contains("ollama") || model_id.contains("lmstudio") || model_id.contains("llamacpp") {
            0.00 // Lokalny model - darmowy
        } else if model_id.contains("claude-3-7") || model_id.contains("claude-3-5-sonnet") || model_id.contains("claude-opus") {
            3.00 // $3.00 / 1M tokenów input
        } else if model_id.contains("claude-haiku") || model_id.contains("claude-sonnet") {
            0.80 // $0.80 / 1M tokenów
        } else if model_id.contains("gpt-4o") {
            2.50 // $2.50 / 1M tokenów
        } else if model_id.contains("gpt-4o-mini") || model_id.contains("gpt-mini") {
            0.15 // $0.15 / 1M tokenów
        } else if model_id.contains("gemini") {
            0.10 // $0.10 / 1M tokenów (Gemini 3.7 / 3.x / Flash)
        } else if model_id.contains("deepseek") {
            0.55 // $0.55 / 1M tokenów
        } else if model_id.contains("qwen") || model_id.contains("llama") {
            0.20 // $0.20 / 1M tokenów
        } else if model_id.contains("kilo") || model_id.contains("opencode") {
            0.00 // Forki opencode — darmowe modele
        } else if model_id.contains("devin") {
            0.00 // Devin — subskrypcja, nie pay-per-token
        } else {
            0.50 // Domyślna stawka
        };

        millions * rate_per_million
    }

    /// Oblicza szacowany koszt sesji w USD na podstawie użytego modelu i liczby znaków.
    /// Kompatybilne ze starym API — wewnętrznie używa TokenEstimator.
    pub fn estimate_cost(model_id: &str, total_chars: usize) -> f64 {
        let tokens = TokenEstimator::estimate(&"x".repeat(total_chars.min(10000)));
        // Skaluj: estimate("x"*10000) = ~2500, więc ratio = total_chars * (tokens/10000)
        let ratio = if total_chars > 0 {
            tokens as f64 / 10000.0
        } else {
            0.25
        };
        let est_tokens = (total_chars as f64 * ratio) as usize;
        Self::estimate_cost_from_tokens(model_id, est_tokens)
    }

    /// Formatuje koszt do czytelnego stringa
    pub fn format_cost(cost: f64) -> String {
        if cost < 0.0001 && cost > 0.0 {
            "<$0.0001".to_string()
        } else if cost == 0.0 {
            "$0.00 (Lokalny/Darmowy)".to_string()
        } else {
            format!("${:.4}", cost)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_calculation() {
        let cost_claude = CostEstimator::estimate_cost("commandcode-claude-3-7-sonnet", 400_000); // 100k tokens
        assert!(cost_claude > 0.20 && cost_claude < 0.40);

        let cost_ollama = CostEstimator::estimate_cost("ollama-codellama", 400_000);
        assert_eq!(cost_ollama, 0.0);
    }

    #[test]
    fn test_token_estimator_code_vs_text() {
        // Kod — dużo symboli, powinno dać więcej tokenów niż chars/4
        let code = "fn main() { let x = vec![1, 2, 3]; println!(\"{}\", x); }";
        let code_tokens = TokenEstimator::estimate(code);
        let code_naive = code.len() / 4;
        assert!(code_tokens > code_naive, "kod powinien mieć więcej tokenów niż chars/4: {} vs {}", code_tokens, code_naive);

        // Proza — mało symboli, powinno dać ~chars/4
        let text = "To jest zwykły tekst po polsku bez żadnych symboli programistycznych ani interpunkcji";
        let text_tokens = TokenEstimator::estimate(text);
        let text_naive = text.len() / 4;
        // Proza powinna być blisko chars/4 (może być nieco więcej przez polskie znaki)
        assert!((text_tokens as f64 - text_naive as f64).abs() < 50.0, "proza powinna być blisko chars/4: {} vs {}", text_tokens, text_naive);
    }

    #[test]
    fn test_token_estimator_empty() {
        assert_eq!(TokenEstimator::estimate(""), 0);
    }

    #[test]
    fn test_token_estimator_file_size() {
        // Plik .rs 3200 bajtów → ~1000 tokenów (3.2 chars/token, 0.85 ratio)
        let tokens_rs = TokenEstimator::estimate_from_file_size("main.rs", 3200);
        assert!(tokens_rs > 700 && tokens_rs < 1100, "main.rs 3200B powinno dać ~850 tokenów, got {}", tokens_rs);

        // Plik .md 4000 bajtów → ~850 tokenów (4.0 chars/token, 0.85 ratio)
        let tokens_md = TokenEstimator::estimate_from_file_size("README.md", 4000);
        assert!(tokens_md > 700 && tokens_md < 1000, "README.md 4000B powinno dać ~850 tokenów, got {}", tokens_md);

        // Plik .json 3500 bajtów → ~850 tokenów
        let tokens_json = TokenEstimator::estimate_from_file_size("config.json", 3500);
        assert!(tokens_json > 700 && tokens_json < 1000, "config.json 3500B powinno dać ~850 tokenów, got {}", tokens_json);

        // Plik 0 bajtów → 0 tokenów
        assert_eq!(TokenEstimator::estimate_from_file_size("empty.rs", 0), 0);
    }

    #[test]
    fn test_cost_free_models() {
        // Kilo/OpenCode — darmowe
        assert_eq!(CostEstimator::estimate_cost_from_tokens("kilo-run-free", 100_000), 0.0);
        assert_eq!(CostEstimator::estimate_cost_from_tokens("opencode-acp", 100_000), 0.0);
        assert_eq!(CostEstimator::estimate_cost_from_tokens("devin-acp", 100_000), 0.0);

        // Claude — płatny
        let claude_cost = CostEstimator::estimate_cost_from_tokens("claude-3-7-sonnet", 100_000);
        assert!(claude_cost > 0.25 && claude_cost < 0.35, "Claude 100k tokens powinno kosztować ~$0.30, got {}", claude_cost);

        // Gemini — tani
        let gemini_cost = CostEstimator::estimate_cost_from_tokens("gemini-3.7-pro", 100_000);
        assert!(gemini_cost > 0.005 && gemini_cost < 0.02, "Gemini 100k tokens powinno kosztować ~$0.01, got {}", gemini_cost);
    }
}
