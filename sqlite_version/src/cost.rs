use serde::{Deserialize, Serialize};

/// Informacje o modelu w katalogu z pełnym cennikiem, providerem i statusem
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelCatalogItem {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub provider_label: String,
    pub cost_per_1m_input: f64,
    pub cost_per_1m_output: f64,
    pub price_display: String,
    pub billing_type: String, // "Darmowy (Free Tier)", "Direct API (Pay-per-token)", "Subskrypcja (W abonamencie)", "Lokalny (Offline)"
    pub is_free: bool,
}

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

    /// Zwraca stawkę USD za 1M tokenów oraz etykietę cenową modelu
    pub fn get_model_rate(model_id: &str) -> (f64, &'static str) {
        if model_id.contains("ollama") || model_id.contains("lmstudio") || model_id.contains("llamacpp") {
            (0.00, "$0.00 (Lokalny / Offline)")
        } else if model_id.contains("antigravity") {
            (0.00, "$0.00 (Antigravity Darmowy)")
        } else if model_id.contains("kilo") || model_id.contains("opencode-acp") {
            (0.00, "$0.00 (Darmowy Tier)")
        } else if model_id.contains("claude-3-7") || model_id.contains("claude-3-5-sonnet") || model_id.contains("claude-opus") {
            (3.00, "$3.00 / 1M tokenów")
        } else if model_id.contains("claude-haiku") || model_id.contains("claude-sonnet") {
            (0.80, "$0.80 / 1M tokenów")
        } else if model_id.contains("gpt-4o") {
            (2.50, "$2.50 / 1M tokenów")
        } else if model_id.contains("gpt-4o-mini") || model_id.contains("gpt-mini") {
            (0.15, "$0.15 / 1M tokenów")
        } else if model_id.contains("gemini") {
            (0.10, "$0.10 / 1M tokenów")
        } else if model_id.contains("deepseek") {
            (0.55, "$0.55 / 1M tokenów")
        } else if model_id.contains("qwen") || model_id.contains("llama") {
            (0.20, "$0.20 / 1M tokenów")
        } else if model_id.contains("devin") {
            (0.00, "Subskrypcja sesyjna")
        } else {
            (0.50, "$0.50 / 1M tokenów")
        }
    }

    /// Zwraca pełne dane katalogowe i cennikowe dla danego modelu
    pub fn get_model_catalog_item(id: &str, name: &str, provider: &str) -> ModelCatalogItem {
        let p = provider.to_lowercase();
        let m = id.to_lowercase();

        let (provider_label, in_cost, out_cost, price_display, billing_type, is_free) = if p == "antigravity" || m.contains("antigravity") {
            ("Google Antigravity", 0.00, 0.00, "$0.00 (100% Darmowy)".to_string(), "Darmowy (Free Tier)", true)
        } else if p.contains("kilo") || m.contains("kilo-run-free") || m.contains("nemotron") {
            ("Kilo Code CLI", 0.00, 0.00, "$0.00 (Darmowy Tier)".to_string(), "Darmowy (Free Tier)", true)
        } else if p.contains("opencode-acp") || m.contains("opencode-zen") || m.contains("opencode-go") {
            ("OpenCode ACP", 0.00, 0.00, "$0.00 (Darmowy Tier)".to_string(), "Darmowy (Free Tier)", true)
        } else if p == "gemini-acp" || m.contains("gemini-acp") {
            ("Google Gemini CLI", 0.00, 0.00, "$0.00 (Darmowy - 60 RPM)".to_string(), "Darmowy (Free Tier)", true)
        } else if p == "ollama" || p == "lmstudio" || p == "llamacpp" || m.contains("ollama") || m.contains("lmstudio") || m.contains("llamacpp") {
            ("Lokalny Serwer AI", 0.00, 0.00, "$0.00 (Lokalny Offline)".to_string(), "Lokalny (Offline)", true)
        } else if p == "cursor" || p == "windsurf" || p == "trae" || p == "copilot" || p == "amazon-q" || p == "augment" {
            ("VS Code Bridge (Edytor)", 0.00, 0.00, "W cenie abonamentu edytora".to_string(), "Subskrypcja (W abonamencie)", true)
        } else if p == "devin-cli" || p == "devin-acp" || m.contains("devin-cli") || m.contains("devin-acp") {
            ("Cognition Devin CLI", 0.00, 0.00, "W cenie subskrypcji Devin".to_string(), "Subskrypcja (W abonamencie)", false)
        } else if p == "devin-cloud" || m.contains("devin-cloud") {
            ("Devin Cloud VM", 2.00, 2.00, "$2.00 / sesja chmurowa".to_string(), "Subskrypcja (Chmura)", false)
        } else if p == "claude-code-cli" || p == "claude-code-acp" {
            ("Claude Code CLI", 3.00, 15.00, "Claude Pro lub Anthropic API".to_string(), "Subskrypcja / Direct API", false)
        } else if p == "codex-cli" || p == "codex-acp" {
            ("OpenAI Codex CLI", 2.50, 10.00, "OpenAI API Key".to_string(), "Direct API (Pay-per-token)", false)
        } else if p == "gemini" || m.contains("gemini") {
            if m.contains("flash") {
                ("Google AI Direct", 0.10, 0.40, "$0.10 / 1M tokenów".to_string(), "Direct API (Pay-per-token)", false)
            } else {
                ("Google AI Direct", 1.25, 5.00, "$1.25 / 1M tokenów".to_string(), "Direct API (Pay-per-token)", false)
            }
        } else if p == "openai" || m.contains("gpt") || m.contains("o1") || m.contains("o3") {
            if m.contains("gpt-4o-mini") || m.contains("mini") {
                ("OpenAI Direct", 0.15, 0.60, "$0.15 / 1M tokenów".to_string(), "Direct API (Pay-per-token)", false)
            } else if m.contains("o1") {
                ("OpenAI Direct", 15.00, 60.00, "$15.00 / 1M tokenów".to_string(), "Direct API (Pay-per-token)", false)
            } else if m.contains("o3") {
                ("OpenAI Direct", 1.10, 4.40, "$1.10 / 1M tokenów".to_string(), "Direct API (Pay-per-token)", false)
            } else {
                ("OpenAI Direct", 2.50, 10.00, "$2.50 / 1M tokenów".to_string(), "Direct API (Pay-per-token)", false)
            }
        } else if p == "anthropic" || m.contains("claude") {
            if m.contains("haiku") {
                ("Anthropic Direct", 0.80, 4.00, "$0.80 / 1M tokenów".to_string(), "Direct API (Pay-per-token)", false)
            } else if m.contains("opus") {
                ("Anthropic Direct", 15.00, 75.00, "$15.00 / 1M tokenów".to_string(), "Direct API (Pay-per-token)", false)
            } else {
                ("Anthropic Direct", 3.00, 15.00, "$3.00 / 1M tokenów".to_string(), "Direct API (Pay-per-token)", false)
            }
        } else if p == "deepseek" || m.contains("deepseek") {
            if m.contains("reasoner") || m.contains("r1") {
                ("DeepSeek Direct", 0.55, 2.19, "$0.55 / 1M tokenów".to_string(), "Direct API (Pay-per-token)", false)
            } else {
                ("DeepSeek Direct", 0.27, 1.10, "$0.27 / 1M tokenów".to_string(), "Direct API (Pay-per-token)", false)
            }
        } else if p == "groq" || m.contains("groq") {
            ("Groq High-Speed", 0.59, 0.79, "$0.59 / 1M tokenów".to_string(), "Direct API (Pay-per-token)", false)
        } else if p == "mistral" || m.contains("mistral") || m.contains("codestral") {
            if m.contains("codestral") {
                ("Mistral AI Direct", 0.30, 0.90, "$0.30 / 1M tokenów".to_string(), "Direct API (Pay-per-token)", false)
            } else {
                ("Mistral AI Direct", 2.00, 6.00, "$2.00 / 1M tokenów".to_string(), "Direct API (Pay-per-token)", false)
            }
        } else if p == "openrouter" || m.contains("openrouter") {
            ("OpenRouter Aggregator", 0.50, 1.50, "Wg stawek OpenRouter".to_string(), "Direct API (Pay-per-token)", false)
        } else {
            ("Provider zewnętrzny", 0.50, 1.50, "$0.50 / 1M tokenów".to_string(), "Direct API (Pay-per-token)", false)
        };

        ModelCatalogItem {
            id: id.to_string(),
            name: name.to_string(),
            provider: provider.to_string(),
            provider_label: provider_label.to_string(),
            cost_per_1m_input: in_cost,
            cost_per_1m_output: out_cost,
            price_display,
            billing_type: billing_type.to_string(),
            is_free,
        }
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

    #[test]
    fn test_model_catalog_item_pricing() {
        let ag = CostEstimator::get_model_catalog_item("antigravity-gemini-2.5", "Gemini 2.5", "antigravity");
        assert!(ag.is_free);
        assert_eq!(ag.cost_per_1m_input, 0.0);
        assert_eq!(ag.provider_label, "Google Antigravity");

        let kilo = CostEstimator::get_model_catalog_item("kilo-run-free", "Nemotron", "kilo-run");
        assert!(kilo.is_free);
        assert_eq!(kilo.cost_per_1m_input, 0.0);

        let gpt = CostEstimator::get_model_catalog_item("openai/gpt-4o", "GPT-4o", "openai");
        assert!(!gpt.is_free);
        assert_eq!(gpt.cost_per_1m_input, 2.50);
        assert_eq!(gpt.billing_type, "Direct API (Pay-per-token)");

        let claude = CostEstimator::get_model_catalog_item("anthropic/claude-3-7-sonnet", "Claude 3.7", "anthropic");
        assert!(!claude.is_free);
        assert_eq!(claude.cost_per_1m_input, 3.00);
    }
}
