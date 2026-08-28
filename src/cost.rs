pub struct CostEstimator;

impl CostEstimator {
    /// Oblicza szacowany koszt sesji w USD na podstawie użytego modelu i liczby znaków/tokenów
    pub fn estimate_cost(model_id: &str, total_chars: usize) -> f64 {
        let tokens = total_chars as f64 / 4.0;
        let millions = tokens / 1_000_000.0;

        let rate_per_million = if model_id.contains("ollama") {
            0.00 // Lokalny model - darmowy
        } else if model_id.contains("claude-3-7") || model_id.contains("claude-3-5-sonnet") {
            3.00 // $3.00 / 1M tokenów input
        } else if model_id.contains("gpt-4o") {
            2.50 // $2.50 / 1M tokenów
        } else if model_id.contains("gemini") {
            0.10 // $0.10 / 1M tokenów (Gemini 3.7 / 3.x / Flash)
        } else if model_id.contains("deepseek") {
            0.55 // $0.55 / 1M tokenów
        } else if model_id.contains("qwen") || model_id.contains("llama") {
            0.20 // $0.20 / 1M tokenów
        } else {
            0.50 // Domyślna stawka
        };

        millions * rate_per_million
    }

    /// Formatuje koszt do czytelnego stringa
    pub fn format_cost(cost: f64) -> String {
        if cost < 0.0001 && cost > 0.0 {
            "<$0.0001".to_string()
        } else if cost == 0.0 {
            "$0.00 (Lokalny)".to_string()
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
}
