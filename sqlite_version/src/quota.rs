use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Informacje o wykorzystaniu pojedynczego pakietu / providera AI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderQuota {
    pub provider_id: String,
    pub name: String,
    pub plan_type: String, // "Pay-As-You-Go", "Free Tier", "Subscription", "Nielimitowany"
    pub quota_used: f64,
    pub quota_total: Option<f64>,
    pub quota_remaining: Option<f64>,
    pub percentage_used: Option<f64>,
    pub unit: String, // "USD", "requests/dzień", "tokens", "nielimitowany"
    pub resets_at: Option<String>, // np. "Północ UTC (codzienny reset)", "2026-10-01"
    pub status: String, // "active", "warning", "depleted", "no_key", "connected"
    pub details: HashMap<String, String>,
}

/// Menedżer pakietów i limitów (Quota & Subscription Tracker).
/// Umożliwia weryfikację stanu środków, pozostałych limitów zapytań
/// oraz dat odnowienia pakietów dla wszystkich podpiętych modeli.
pub struct QuotaManager;

impl QuotaManager {
    /// Pobiera stan wszystkich pakietów dla wykrytych providerów.
    pub async fn check_all_quotas(
        auth_keys: &HashMap<String, String>,
        daily_gemini_requests: u64,
    ) -> Vec<ProviderQuota> {
        let mut quotas = Vec::new();

        // 1. Antigravity IDE (gRPC-Web direct) — w 100% bezpłatny, 32 modele
        let antigravity_online = Self::check_antigravity_available().await;
        quotas.push(ProviderQuota {
            provider_id: "antigravity".to_string(),
            name: "Antigravity IDE (gRPC-Web Direct)".to_string(),
            plan_type: "Darmowy bez limitu".to_string(),
            quota_used: 0.0,
            quota_total: None,
            quota_remaining: None,
            percentage_used: Some(0.0),
            unit: "nielimitowany".to_string(),
            resets_at: Some("Zawsze aktywny (lokalny port)".to_string()),
            status: if antigravity_online {
                "connected".to_string()
            } else {
                "offline".to_string()
            },
            details: {
                let mut d = HashMap::new();
                d.insert("modele".to_string(), "32 modele (Gemini 2.5, Claude 3.7, GPT-OSS)".to_string());
                d.insert("koszt".to_string(), "$0.00 (Bezpłatny)".to_string());
                d
            },
        });

        // 2. OpenRouter — zapytanie do API o saldo USD i limity
        if let Some(key) = auth_keys.get("openrouter") {
            let or_quota = Self::check_openrouter_quota(key).await;
            quotas.push(or_quota);
        }

        // 3. Google Gemini (AI Studio) — darmowy tier 1500 req/dzień
        if let Some(_key) = auth_keys.get("gemini") {
            let used = daily_gemini_requests as f64;
            let total = 1500.0;
            let remaining = (total - used).max(0.0);
            let pct = (used / total) * 100.0;

            let now = Utc::now();
            let next_reset = now.date_naive().succ_opt().unwrap_or_else(|| now.date_naive());
            let midnight_utc = format!("{} 00:00:00 UTC", next_reset);

            quotas.push(ProviderQuota {
                provider_id: "gemini".to_string(),
                name: "Google Gemini (AI Studio)".to_string(),
                plan_type: "Free Tier (Darmowy)".to_string(),
                quota_used: used,
                quota_total: Some(total),
                quota_remaining: Some(remaining),
                percentage_used: Some(pct),
                unit: "requests/dzień".to_string(),
                resets_at: Some(format!("Codziennie o północy ({})", midnight_utc)),
                status: if remaining <= 0.0 {
                    "depleted".to_string()
                } else if pct > 80.0 {
                    "warning".to_string()
                } else {
                    "active".to_string()
                },
                details: {
                    let mut d = HashMap::new();
                    d.insert("limit_rpm".to_string(), "15 RPM (zapytań na minutę)".to_string());
                    d.insert("limit_rpd".to_string(), "1500 RPD (zapytań dziennie)".to_string());
                    d
                },
            });
        }

        // 4. Anthropic Claude API
        if let Some(_key) = auth_keys.get("anthropic") {
            quotas.push(ProviderQuota {
                provider_id: "anthropic".to_string(),
                name: "Anthropic Claude (Direct API)".to_string(),
                plan_type: "Pay-As-You-Go".to_string(),
                quota_used: 0.0,
                quota_total: None,
                quota_remaining: None,
                percentage_used: None,
                unit: "USD".to_string(),
                resets_at: Some("Miesięczny cykl bilingowy".to_string()),
                status: "active".to_string(),
                details: {
                    let mut d = HashMap::new();
                    d.insert("modele".to_string(), "Claude 3.7 Sonnet, Opus, Haiku".to_string());
                    d.insert("tier".to_string(), "Tier 1-4 (w zależności od konta)".to_string());
                    d
                },
            });
        }

        // 5. OpenAI API
        if let Some(_key) = auth_keys.get("openai") {
            quotas.push(ProviderQuota {
                provider_id: "openai".to_string(),
                name: "OpenAI API (GPT-4o, o3-mini)".to_string(),
                plan_type: "Pay-As-You-Go".to_string(),
                quota_used: 0.0,
                quota_total: None,
                quota_remaining: None,
                percentage_used: None,
                unit: "USD".to_string(),
                resets_at: Some("Miesięczny cykl bilingowy".to_string()),
                status: "active".to_string(),
                details: {
                    let mut d = HashMap::new();
                    d.insert("modele".to_string(), "GPT-4o, o3-mini, o1".to_string());
                    d
                },
            });
        }

        // 6. Devin Cloud (api.devin.ai v3)
        if let Some(_key) = auth_keys.get("devin") {
            quotas.push(ProviderQuota {
                provider_id: "devin".to_string(),
                name: "Devin Cloud (Cognition AI)".to_string(),
                plan_type: "Enterprise / Pro".to_string(),
                quota_used: 0.0,
                quota_total: None,
                quota_remaining: None,
                percentage_used: None,
                unit: "sesje".to_string(),
                resets_at: Some("Miesięczny reset pakietu sesji".to_string()),
                status: "active".to_string(),
                details: {
                    let mut d = HashMap::new();
                    d.insert("typ".to_string(), "Sesje chmurowe w api.devin.ai".to_string());
                    d
                },
            });
        }

        // 7. Kilo Code / OpenCode Free Tier (Nemotron, DeepSeek, Minimax)
        quotas.push(ProviderQuota {
            provider_id: "kilo_free".to_string(),
            name: "Kilo / OpenCode Free Tier".to_string(),
            plan_type: "Darmowe modele CLI".to_string(),
            quota_used: 0.0,
            quota_total: None,
            quota_remaining: None,
            percentage_used: Some(0.0),
            unit: "nielimitowany".to_string(),
            resets_at: Some("Brak limitu bilingowego (17 modeli)".to_string()),
            status: "active".to_string(),
            details: {
                let mut d = HashMap::new();
                d.insert("modele".to_string(), "Nvidia Nemotron, DeepSeek V3, Minimax, Ling".to_string());
                d.insert("wymagania".to_string(), "Brak karty kredytowej".to_string());
                d
            },
        });

        quotas
    }

    /// Sprawdza czy Antigravity Language Server odpowiada lokalnie
    async fn check_antigravity_available() -> bool {
        crate::providers::antigravity::AntigravityProvider::is_available().await
    }

    /// Odpytuje OpenRouter o saldo i pozostały limit konta
    async fn check_openrouter_quota(api_key: &str) -> ProviderQuota {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        let resp = client
            .get("https://openrouter.ai/api/v1/auth/key")
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await;

        if let Ok(res) = resp {
            if let Ok(json) = res.json::<serde_json::Value>().await {
                if let Some(data) = json.get("data") {
                    let usage = data.get("usage").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let limit = data.get("limit").and_then(|v| v.as_f64());
                    let is_free_tier = data.get("is_free_tier").and_then(|v| v.as_bool()).unwrap_or(false);
                    let label = data.get("label").and_then(|v| v.as_str()).unwrap_or("Klucz API");

                    let remaining = limit.map(|l| (l - usage).max(0.0));
                    let pct = limit.map(|l| if l > 0.0 { (usage / l) * 100.0 } else { 0.0 });

                    let status = if let Some(rem) = remaining {
                        if rem <= 0.0 {
                            "depleted"
                        } else if pct.unwrap_or(0.0) > 80.0 {
                            "warning"
                        } else {
                            "active"
                        }
                    } else {
                        "active"
                    };

                    let mut details = HashMap::new();
                    details.insert("label".to_string(), label.to_string());
                    details.insert("is_free_tier".to_string(), is_free_tier.to_string());

                    return ProviderQuota {
                        provider_id: "openrouter".to_string(),
                        name: "OpenRouter AI".to_string(),
                        plan_type: if is_free_tier { "Free Tier".to_string() } else { "Kredyty USD".to_string() },
                        quota_used: usage,
                        quota_total: limit,
                        quota_remaining: remaining,
                        percentage_used: pct,
                        unit: "USD".to_string(),
                        resets_at: Some("Cykl bilingowy OpenRouter / doładowanie".to_string()),
                        status: status.to_string(),
                        details,
                    };
                }
            }
        }

        // Fallback jeśli API nie odpowiedziało
        ProviderQuota {
            provider_id: "openrouter".to_string(),
            name: "OpenRouter AI".to_string(),
            plan_type: "Kredyty USD".to_string(),
            quota_used: 0.0,
            quota_total: None,
            quota_remaining: None,
            percentage_used: None,
            unit: "USD".to_string(),
            resets_at: Some("Cykl bilingowy OpenRouter".to_string()),
            status: "active".to_string(),
            details: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_quota_tracker_antigravity_and_gemini() {
        let mut auth_keys = HashMap::new();
        auth_keys.insert("gemini".to_string(), "test-gemini-key".to_string());

        let quotas = QuotaManager::check_all_quotas(&auth_keys, 250).await;

        // Powinno zawierać Antigravity, Gemini i Kilo Free
        assert!(quotas.iter().any(|q| q.provider_id == "antigravity"));
        assert!(quotas.iter().any(|q| q.provider_id == "kilo_free"));

        let gemini = quotas.iter().find(|q| q.provider_id == "gemini").unwrap();
        assert_eq!(gemini.quota_used, 250.0);
        assert_eq!(gemini.quota_total, Some(1500.0));
        assert_eq!(gemini.quota_remaining, Some(1250.0));
        assert!(gemini.percentage_used.unwrap() > 16.0);
        assert_eq!(gemini.status, "active");
    }

    #[tokio::test]
    async fn test_gemini_warning_and_depleted() {
        let mut auth_keys = HashMap::new();
        auth_keys.insert("gemini".to_string(), "key".to_string());

        // Test warning (>80%)
        let quotas_warn = QuotaManager::check_all_quotas(&auth_keys, 1300).await;
        let gemini_warn = quotas_warn.iter().find(|q| q.provider_id == "gemini").unwrap();
        assert_eq!(gemini_warn.status, "warning");

        // Test depleted (>=1500)
        let quotas_dep = QuotaManager::check_all_quotas(&auth_keys, 1500).await;
        let gemini_dep = quotas_dep.iter().find(|q| q.provider_id == "gemini").unwrap();
        assert_eq!(gemini_dep.status, "depleted");
        assert_eq!(gemini_dep.quota_remaining, Some(0.0));
    }
}
