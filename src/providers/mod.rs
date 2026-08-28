use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

use crate::config::AppConfig;

pub mod bridge;
pub mod direct;
pub mod subprocess;

use bridge::BridgeProvider;
use direct::DirectApiProvider;
use subprocess::SubprocessProvider;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    async fn stream_chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        token_tx: Sender<String>,
    ) -> Result<()>;
}

pub struct ProviderRouter {
    config: AppConfig,
    bridge: Arc<BridgeProvider>,
    direct_gemini: Option<Arc<DirectApiProvider>>,
    direct_groq: Option<Arc<DirectApiProvider>>,
    direct_openai: Option<Arc<DirectApiProvider>>,
    direct_anthropic: Option<Arc<DirectApiProvider>>,
    direct_deepseek: Option<Arc<DirectApiProvider>>,
    direct_mistral: Option<Arc<DirectApiProvider>>,
    direct_openrouter: Option<Arc<DirectApiProvider>>,
    direct_commandcode: Option<Arc<DirectApiProvider>>,
    ollama: Arc<DirectApiProvider>,
    lmstudio: Arc<DirectApiProvider>,
    llamacpp: Arc<DirectApiProvider>,
    custom_endpoints: Vec<(String, Arc<DirectApiProvider>)>,
    commandcode: Arc<SubprocessProvider>,
}

impl ProviderRouter {
    pub fn new(config: AppConfig) -> Self {
        let bridge = Arc::new(BridgeProvider::new(config.bridge_url.clone()));
        
        let direct_gemini = config.direct_gemini_api_key.as_ref().map(|k| {
            Arc::new(DirectApiProvider::new(
                "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
                Some(k.clone()),
            ))
        });

        let direct_groq = config.direct_groq_api_key.as_ref().map(|k| {
            Arc::new(DirectApiProvider::new(
                "https://api.groq.com/openai/v1".to_string(),
                Some(k.clone()),
            ))
        });

        let direct_openai = config.direct_openai_api_key.as_ref().map(|k| {
            Arc::new(DirectApiProvider::new(
                "https://api.openai.com/v1".to_string(),
                Some(k.clone()),
            ))
        });

        let direct_anthropic = config.direct_anthropic_api_key.as_ref().map(|k| {
            Arc::new(DirectApiProvider::new(
                "https://api.anthropic.com/v1".to_string(),
                Some(k.clone()),
            ))
        });

        let direct_deepseek = config.direct_deepseek_api_key.as_ref().map(|k| {
            Arc::new(DirectApiProvider::new(
                "https://api.deepseek.com/v1".to_string(),
                Some(k.clone()),
            ))
        });

        let direct_mistral = config.direct_mistral_api_key.as_ref().map(|k| {
            Arc::new(DirectApiProvider::new(
                "https://api.mistral.ai/v1".to_string(),
                Some(k.clone()),
            ))
        });

        let direct_openrouter = config.direct_openrouter_api_key.as_ref().map(|k| {
            Arc::new(DirectApiProvider::new(
                "https://openrouter.ai/api/v1".to_string(),
                Some(k.clone()),
            ))
        });

        let direct_commandcode = config.commandcode_api_key.as_ref().map(|k| {
            Arc::new(DirectApiProvider::new(
                config.commandcode_base_url.clone().unwrap_or_else(|| "https://api.commandcode.ai/v1".to_string()),
                Some(k.clone()),
            ))
        });

        let ollama = Arc::new(DirectApiProvider::new(
            config.ollama_url.clone().unwrap_or_else(|| "http://localhost:11434/v1".to_string()),
            None,
        ));

        let lmstudio = Arc::new(DirectApiProvider::new(
            config.lmstudio_url.clone().unwrap_or_else(|| "http://localhost:1234/v1".to_string()),
            None,
        ));

        let llamacpp = Arc::new(DirectApiProvider::new(
            config.llamacpp_url.clone().unwrap_or_else(|| "http://localhost:8080/v1".to_string()),
            None,
        ));

        let mut custom_endpoints = Vec::new();
        for ep in &config.custom_endpoints {
            custom_endpoints.push((
                ep.id.clone(),
                Arc::new(DirectApiProvider::new(ep.base_url.clone(), ep.api_key.clone())),
            ));
        }

        let commandcode = Arc::new(SubprocessProvider::new("commandcode".to_string()));

        Self {
            config,
            bridge,
            direct_gemini,
            direct_groq,
            direct_openai,
            direct_anthropic,
            direct_deepseek,
            direct_mistral,
            direct_openrouter,
            direct_commandcode,
            ollama,
            lmstudio,
            llamacpp,
            custom_endpoints,
            commandcode,
        }
    }

    pub fn get_available_models(&self) -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            // ⚡ OpenCode Native Ecosystem
            ("opencode-zen", "OpenCode Zen (Claude 3.7 Hybrid & Thinking)", "opencode"),
            ("opencode-go", "OpenCode Go (Fast & Lightweight)", "opencode"),
            ("opencode-flash", "OpenCode Flash 3.7 (Instant)", "opencode"),
            ("opencode-pro", "OpenCode Pro (Deep Reasoning)", "opencode"),
            ("opencode-claude-3-7-sonnet", "Claude 3.7 Sonnet (OpenCode Native)", "opencode"),
            ("opencode-claude-3-5-sonnet", "Claude 3.5 Sonnet (OpenCode Native)", "opencode"),
            ("opencode-gpt-4o", "GPT-4o Omnimodal (OpenCode Native)", "opencode"),
            ("opencode-o3-mini", "o3-mini High Reasoning (OpenCode Native)", "opencode"),
            ("opencode-deepseek-r1", "DeepSeek R1 Full Reasoning (OpenCode)", "opencode"),
            ("opencode-gemini-3-7-pro", "Gemini 3.7 Pro (OpenCode Native)", "opencode"),
            ("opencode-gemini-3-7-flash", "Gemini 3.7 Flash (OpenCode Native)", "opencode"),

            // 🪐 Google Antigravity SDK & IDE
            ("antigravity-gemini-3-7-flash", "Gemini 3.7 Flash Instant (Antigravity)", "antigravity"),
            ("antigravity-gemini-3-7-pro", "Gemini 3.7 Pro 2M Context (Antigravity)", "antigravity"),
            ("antigravity-gemini-3-7-flash-thinking", "Gemini 3.7 Flash Thinking (Antigravity)", "antigravity"),
            ("antigravity-claude-3-7", "Claude 3.7 Sonnet Thinking (Antigravity)", "antigravity"),
            ("antigravity-claude-3-5-sonnet", "Claude 3.5 Sonnet (Antigravity SDK)", "antigravity"),
            ("antigravity-gemini-3-1-pro", "Gemini 3.1 Pro Preview (Antigravity)", "antigravity"),
            ("antigravity-deepseek-r1", "DeepSeek R1 Reasoning (Antigravity)", "antigravity"),
            ("antigravity-gpt-4o", "GPT-4o Omnimodal (Antigravity SDK)", "antigravity"),

            // 🎯 Trae AI — realne modele z docs.trae.ai (Claude usunięty 11.2025, teraz Seed/Kimi/MiniMax/Gemini/GPT-5)
            // wildcard: każdy `trae-*` → Bridge (np. przyszły Seed-2.5 zadziała bez zmiany kodu)
            ("trae-seed-2.1-turbo", "Seed 2.1 Turbo (Trae/ByteDance)", "trae"),
            ("trae-seed-2.1-pro", "Seed 2.1 Pro (Trae/ByteDance)", "trae"),
            ("trae-kimi-k2.5", "Kimi K2.5 (Trae/Moonshot)", "trae"),
            ("trae-minimax-m3", "MiniMax M3 (Trae/MiniMax)", "trae"),
            ("trae-minimax-m2.7", "MiniMax M2.7 (Trae)", "trae"),
            ("trae-gemini-3.1-pro", "Gemini 3.1 Pro Preview (Trae)", "trae"),
            ("trae-gpt-5.4", "GPT-5.4 (Trae/OpenAI) *nie-US", "trae"),

            // 🌊 Windsurf Cascade (rebrand → Devin)
            ("windsurf-cascade-sonnet", "Claude 3.7 Sonnet (Cascade Flow)", "windsurf"),
            ("windsurf-cascade-gpt-4o", "GPT-4o (Cascade Flow)", "windsurf"),
            ("devin-cascade-sonnet", "Claude 3.7 Sonnet (Devin Cascade)", "windsurf"),
            ("devin-cascade-gpt-4o", "GPT-4o (Devin Cascade)", "windsurf"),

            // 🔮 Cursor Pro Bridge
            ("cursor-claude-3-7-sonnet", "Claude 3.7 Sonnet Thinking (Cursor Pro)", "cursor"),
            ("cursor-claude-3-5-sonnet", "Claude 3.5 Sonnet (Cursor Pro)", "cursor"),
            ("cursor-gpt-4o", "GPT-4o (Cursor Pro)", "cursor"),
            ("cursor-o3-mini", "o3-mini High (Cursor Pro)", "cursor"),
            ("cursor-deepseek-r1", "DeepSeek R1 (Cursor Pro)", "cursor"),

            // 📎 GitHub Copilot (via vscode.lm / Bridge)
            ("copilot-gpt-4o", "GPT-4o (GitHub Copilot)", "copilot"),
            ("copilot-claude-3-7-sonnet", "Claude 3.7 Sonnet (GitHub Copilot)", "copilot"),

            // ⌨️ Command Code
            ("commandcode-claude-3-7-sonnet", "Claude 3.7 Sonnet Thinking (Command Code)", "commandcode"),
            ("commandcode-claude-3-7-thinking", "Claude 3.7 Extended Thinking (Command Code)", "commandcode"),
            ("commandcode-claude-3-5-sonnet", "Claude 3.5 Sonnet v2 (Command Code)", "commandcode"),
            ("commandcode-claude-3-5-haiku", "Claude 3.5 Haiku Ultra Fast (Command Code)", "commandcode"),
            ("commandcode-opus-3", "Claude 3 Opus (Command Code)", "commandcode"),
            ("commandcode-gpt-4o", "GPT-4o Omnimodal (Command Code)", "commandcode"),
            ("commandcode-o3-mini", "o3-mini High Reasoning (Command Code)", "commandcode"),
            ("commandcode-deepseek-r1", "DeepSeek R1 (Command Code)", "commandcode"),
            ("commandcode-cli", "Command Code CLI (Direct Subprocess Native)", "commandcode"),

            // 🌐 Direct Gemini API
            ("gemini-3.7-flash", "Gemini 3.7 Flash (Direct API)", "gemini"),
            ("gemini-3.7-pro", "Gemini 3.7 Pro (Direct API)", "gemini"),
            ("gemini-3.7-flash-thinking", "Gemini 3.7 Flash Thinking (Direct API)", "gemini"),
            ("gemini-3.1-pro", "Gemini 3.1 Pro Preview (Direct API)", "gemini"),

            // 🤖 Direct OpenAI API
            ("openai/gpt-4o", "GPT-4o Omnimodal (Direct OpenAI)", "openai"),
            ("openai/o3-mini", "o3-mini High Reasoning (Direct OpenAI)", "openai"),
            ("openai/o1", "o1 Full Reasoning (Direct OpenAI)", "openai"),
            ("openai/gpt-4o-mini", "GPT-4o Mini Fast (Direct OpenAI)", "openai"),

            // 🧠 Direct DeepSeek API
            ("deepseek/deepseek-chat", "DeepSeek V3 (Direct DeepSeek API)", "deepseek"),
            ("deepseek/deepseek-reasoner", "DeepSeek R1 Reasoning (DeepSeek API)", "deepseek"),

            // ⚡ Direct Groq High-Speed API
            ("groq-llama-3.3-70b", "Llama 3.3 70B Versatile (Groq 1000 tok/s)", "groq"),
            ("groq-deepseek-r1", "DeepSeek R1 Distill 70B (Groq)", "groq"),
            ("groq-qwen-coder", "Qwen 2.5 Coder 32B (Groq Fast)", "groq"),

            // 🇫🇷 Mistral & Codestral
            ("mistral/codestral-2501", "Codestral 2501 (Mistral AI Code)", "mistral"),
            ("mistral/mistral-large-2", "Mistral Large 2 (Mistral AI)", "mistral"),

            // 🔀 OpenRouter Aggregator
            ("openrouter/auto", "OpenRouter Best Router (200+ Models)", "openrouter"),
            ("openrouter/claude-3.7-sonnet", "Claude 3.7 Sonnet (OpenRouter)", "openrouter"),
            ("openrouter/deepseek-r1", "DeepSeek R1 (OpenRouter)", "openrouter"),

            // 🤝 Amazon Q & Augment (Bridge)
            ("amazon-q", "Amazon Q Developer (Bridge)", "amazon-q"),
            ("amazon-q-claude", "Claude via Amazon Q (Bridge)", "amazon-q"),
            ("augment-code", "Augment Code Agent (Bridge)", "augment"),

            // 🖥️ LM Studio (Local Port 1234)
            ("lmstudio/local-model", "LM Studio Active Model (Port 1234)", "lmstudio"),
            ("lmstudio/qwen2.5-coder", "Qwen 2.5 Coder (LM Studio)", "lmstudio"),
            ("lmstudio/deepseek-r1", "DeepSeek R1 (LM Studio)", "lmstudio"),

            // 🦙 Llama.cpp Server (Local Port 8080)
            ("llamacpp/default", "Llama.cpp Default Server (Port 8080)", "llamacpp"),
            ("llamacpp/llama-3.3", "Llama 3.3 Instruct (Llama.cpp)", "llamacpp"),

            // 🦙 Ollama Local
            ("ollama/llama3.2", "Llama 3.2 (Ollama Local)", "ollama"),
            ("ollama/deepseek-r1", "DeepSeek R1 8B/14B (Ollama Local)", "ollama"),
            ("ollama/qwen2.5-coder", "Qwen 2.5 Coder (Ollama Local)", "ollama"),
        ]
    }

    pub async fn stream_with_failover(
        &self,
        requested_model: &str,
        messages: &[ChatMessage],
        token_tx: Sender<String>,
    ) -> Result<String> {
        let mut models_to_try = vec![requested_model.to_string()];

        if self.config.auto_failover {
            for fallback in &self.config.fallback_chain {
                if fallback != requested_model && !models_to_try.contains(fallback) {
                    models_to_try.push(fallback.clone());
                }
            }
        }

        let mut last_err = anyhow!("Brak dostępnych operatorów");

        for model in &models_to_try {
            let res = self.execute_provider(model, messages, token_tx.clone()).await;
            match res {
                Ok(_) => return Ok(model.clone()),
                Err(err) => {
                    let _ = token_tx.send(format!("\n⚠️ [Failover] Błąd operatora {model}: {err}. Przełączanie na kolejny...\n")).await;
                    last_err = err;
                }
            }
        }

        Err(last_err)
    }

    async fn execute_provider(
        &self,
        model: &str,
        messages: &[ChatMessage],
        token_tx: Sender<String>,
    ) -> Result<()> {
        if model == "commandcode-cli" || model == "commandcode" || model == "cmd" || model == "cmd-cli" {
            return self.commandcode.stream_chat(model, messages, token_tx).await;
        }

        if model.starts_with("commandcode-") || model.starts_with("cmd-") {
            if let Some(ref cmdcode) = self.direct_commandcode {
                let target_model = model
                    .strip_prefix("commandcode-")
                    .or_else(|| model.strip_prefix("cmd-"))
                    .unwrap_or(model);
                return cmdcode.stream_chat(target_model, messages, token_tx).await;
            }
        }

        if model.starts_with("gemini") {
            if let Some(ref gemini) = self.direct_gemini {
                return gemini.stream_chat(model, messages, token_tx).await;
            }
        }

        if model.starts_with("groq") {
            if let Some(ref groq) = self.direct_groq {
                let target_model = match model {
                    "groq-llama-3.3-70b" => "llama-3.3-70b-versatile",
                    "groq-deepseek-r1" => "deepseek-r1-distill-llama-70b",
                    "groq-qwen-coder" => "qwen-2.5-coder-32b",
                    _ => "llama-3.3-70b-versatile",
                };
                return groq.stream_chat(target_model, messages, token_tx).await;
            }
        }

        if model.starts_with("openai") {
            if let Some(ref openai) = self.direct_openai {
                let target_model = model.strip_prefix("openai/").or_else(|| model.strip_prefix("openai-")).unwrap_or(model);
                return openai.stream_chat(target_model, messages, token_tx).await;
            }
        }

        if model.starts_with("anthropic") {
            if let Some(ref anthropic) = self.direct_anthropic {
                let target_model = model.strip_prefix("anthropic/").unwrap_or(model);
                return anthropic.stream_chat(target_model, messages, token_tx).await;
            }
        }

        if model.starts_with("deepseek") {
            if let Some(ref deepseek) = self.direct_deepseek {
                let target_model = model.strip_prefix("deepseek/").unwrap_or(model);
                return deepseek.stream_chat(target_model, messages, token_tx).await;
            }
        }

        if model.starts_with("mistral") {
            if let Some(ref mistral) = self.direct_mistral {
                let target_model = model.strip_prefix("mistral/").unwrap_or(model);
                return mistral.stream_chat(target_model, messages, token_tx).await;
            }
        }

        if model.starts_with("openrouter") {
            if let Some(ref openrouter) = self.direct_openrouter {
                let target_model = model.strip_prefix("openrouter/").unwrap_or(model);
                return openrouter.stream_chat(target_model, messages, token_tx).await;
            }
        }

        // Trae alias: legacy `trae-sonnet` / `trae-claude-*` → Seed (Claude usunięty z Trae 11.2025)
        if model == "trae-sonnet" || model == "trae-claude-3-7-sonnet" || model == "trae-claude-3-5-sonnet" {
            return self.bridge.stream_chat("trae-seed-2.1-turbo", messages, token_tx).await;
        }

        // Devin to alias Windsurf (rebrand)
        if model.starts_with("devin-") {
            let aliased = model.replacen("devin-", "windsurf-", 1);
            return self.bridge.stream_chat(&aliased, messages, token_tx).await;
        }

        // Copilot / Amazon Q / Augment - zawsze Bridge (vscode.lm)
        if model.starts_with("copilot-") || model.starts_with("amazon-q") || model.starts_with("augment") {
            return self.bridge.stream_chat(model, messages, token_tx).await;
        }

        if model.starts_with("lmstudio") {
            let target_model = model.strip_prefix("lmstudio/").unwrap_or(model);
            return self.lmstudio.stream_chat(target_model, messages, token_tx).await;
        }

        if model.starts_with("llamacpp") {
            let target_model = model.strip_prefix("llamacpp/").unwrap_or(model);
            return self.llamacpp.stream_chat(target_model, messages, token_tx).await;
        }

        if model.starts_with("ollama") {
            let target_model = model.strip_prefix("ollama/").unwrap_or(model);
            return self.ollama.stream_chat(target_model, messages, token_tx).await;
        }

        // Sprawdź dynamiczne endpointy użytkownika (custom_endpoints)
        for (prefix, provider) in &self.custom_endpoints {
            if model.starts_with(prefix) {
                let target_model = model.strip_prefix(&format!("{}/", prefix)).unwrap_or(model);
                return provider.stream_chat(target_model, messages, token_tx).await;
            }
        }

        // Domyślnie używaj zunifikowanego Bridge (Cursor, Antigravity, Trae, Windsurf, OpenCode)
        self.bridge.stream_chat(model, messages, token_tx).await
    }

    /// Asynchroniczne dynamiczne wykrywanie modeli ze wszystkich aktywnych źródeł (Bridge, Ollama, LM Studio, Custom)
    pub async fn discover_models(&self) -> Vec<(String, String, String)> {
        let mut results: Vec<(String, String, String)> = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        // 1. Dodaj modele bazowe
        for (id, name, prov) in self.get_available_models() {
            if seen_ids.insert(id.to_string()) {
                results.push((id.to_string(), name.to_string(), prov.to_string()));
            }
        }

        // 2. Dodaj modele z custom_endpoints
        for ep in &self.config.custom_endpoints {
            let model_id = format!("{}/{}", ep.id, ep.default_model.as_deref().unwrap_or("default"));
            if seen_ids.insert(model_id.clone()) {
                results.push((model_id, ep.name.clone(), ep.id.clone()));
            }
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(800))
            .build()
            .unwrap_or_default();

        // 3. Odpytaj mostek Bridge (/v1/models)
        let bridge_url = format!("{}/models", self.config.bridge_url.trim_end_matches('/'));
        if let Ok(resp) = client.get(&bridge_url).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
                    for item in data {
                        if let (Some(id), Some(name)) = (
                            item.get("id").and_then(|v| v.as_str()),
                            item.get("name").and_then(|v| v.as_str()),
                        ) {
                            let prov = item.get("provider").and_then(|v| v.as_str()).unwrap_or("bridge");
                            if seen_ids.insert(id.to_string()) {
                                results.push((id.to_string(), name.to_string(), prov.to_string()));
                            }
                        }
                    }
                }
            }
        }

        // 4. Odpytaj Ollama (/api/tags)
        let ollama_url = self.config.ollama_url.as_deref().unwrap_or("http://localhost:11434/v1");
        let ollama_tags_url = format!("{}/api/tags", ollama_url.trim_end_matches("/v1").trim_end_matches('/'));
        if let Ok(resp) = client.get(&ollama_tags_url).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(models) = json.get("models").and_then(|m| m.as_array()) {
                    for m in models {
                        if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                            let id = format!("ollama/{}", name);
                            if seen_ids.insert(id.clone()) {
                                results.push((id, format!("{} (Ollama Local)", name), "ollama".to_string()));
                            }
                        }
                    }
                }
            }
        }

        // 5. Odpytaj LM Studio (/v1/models)
        let lm_url = self.config.lmstudio_url.as_deref().unwrap_or("http://localhost:1234/v1");
        let lm_models_url = format!("{}/models", lm_url.trim_end_matches('/'));
        if let Ok(resp) = client.get(&lm_models_url).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
                    for item in data {
                        if let Some(id_str) = item.get("id").and_then(|v| v.as_str()) {
                            let id = format!("lmstudio/{}", id_str);
                            if seen_ids.insert(id.clone()) {
                                results.push((id, format!("{} (LM Studio)", id_str), "lmstudio".to_string()));
                            }
                        }
                    }
                }
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    #[test]
    fn test_get_available_models_count_and_aliases() {
        let cfg = AppConfig::default();
        let router = ProviderRouter::new(cfg);
        let models = router.get_available_models();
        assert!(models.len() >= 70, "should have 75+ models, got {}", models.len());
        assert!(models.iter().any(|(id,_,_)| *id=="cursor-claude-3-7-sonnet"));
        assert!(models.iter().any(|(id,_,_)| *id=="trae-kimi-k2.5"));
        assert!(models.iter().any(|(id,_,_)| *id=="groq-llama-3.3-70b"));
        assert!(models.iter().any(|(id,_,_)| *id=="openrouter/auto"));
    }
    #[test]
    fn test_provider_tabs_count() {
        let tabs = crate::app::App::model_provider_tabs();
        assert!(tabs.len() >= 15);
        assert_eq!(tabs[0].1, "fav");
    }
    #[test]
    fn test_filtered_models_all() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let mut cfg = AppConfig::default();
            cfg.favorite_models = vec!["cursor-claude-3-7-sonnet".to_string()];
            let _router = ProviderRouter::new(cfg.clone());
            let app = crate::app::App::new(std::env::temp_dir(), cfg);
            assert!(!app.filtered_models().is_empty());
            // fav filter should contain the favorite
            // switch to fav tab is index 0
        });
    }
}
