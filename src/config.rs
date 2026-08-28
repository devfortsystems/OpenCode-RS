use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomEndpoint {
    pub id: String,          // np. "lmstudio", "llamacpp", "vllm", "jan", "localai"
    pub name: String,        // np. "LM Studio Local Server"
    pub base_url: String,    // np. "http://localhost:1234/v1"
    pub api_key: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub default_provider: String,
    pub default_model: String,
    pub bridge_url: String,
    pub auto_failover: bool,
    pub fallback_chain: Vec<String>,
    pub direct_gemini_api_key: Option<String>,
    pub direct_groq_api_key: Option<String>,
    pub direct_openai_api_key: Option<String>,
    pub direct_anthropic_api_key: Option<String>,
    pub direct_deepseek_api_key: Option<String>,
    pub direct_mistral_api_key: Option<String>,
    pub direct_openrouter_api_key: Option<String>,
    pub commandcode_api_key: Option<String>,
    pub commandcode_base_url: Option<String>,
    pub ollama_url: Option<String>,
    pub lmstudio_url: Option<String>,
    pub llamacpp_url: Option<String>,
    pub custom_endpoints: Vec<CustomEndpoint>, // Dowolna liczba lokalnych / OpenAI compatible serwerów
    pub storage_mode: String, // "central" lub "in_project"
    pub theme: String,        // "opencode", "graphite", "oled", etc.
    pub language: String,     // "en" (default), "pl", "zh", "de", "es", "fr", "uk"
    pub git_sync_mode: String,// "both" (default), "public", "private"
    pub favorite_models: Vec<String>, // Ulubione modele z gwiazdką
    pub web_companion_enabled: bool,  // Włącz/wyłącz serwer Web Companion
    pub web_companion_port: u16,      // Domyślny port: 7711 (konfigurowalny)
    pub mcp_config_path: Option<String>,
    pub voice_plugin_command: Option<String>,
    pub trust_mode: bool,             // true = pełna autonomia, false = pytaj o zgodę (sandbox)
    pub agent_hooks_enabled: bool,    // Włącz/wyłącz Agent Hooks (.kiro/hooks/)
    pub spec_output_dir: String,      // Katalog na pliki spec (domyślnie ".kiro/specs")
    pub sync_server_url: Option<String>,
    pub sync_token: Option<String>,
    pub sync_auto: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_provider: "bridge".to_string(),
            default_model: "cursor-claude-3-7-sonnet".to_string(),
            bridge_url: "http://127.0.0.1:8765/v1".to_string(),
            auto_failover: true,
            fallback_chain: vec![
                "cursor-claude-3-7-sonnet".to_string(),
                "opencode-zen".to_string(),
                "antigravity-claude-3-7".to_string(),
                "commandcode-claude-3-7-sonnet".to_string(),
                "windsurf-cascade-sonnet".to_string(),
                "trae-sonnet".to_string(),
                "gemini-3.7-flash".to_string(),
            ],
            direct_gemini_api_key: std::env::var("GEMINI_API_KEY").ok(),
            direct_groq_api_key: std::env::var("GROQ_API_KEY").ok(),
            direct_openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            direct_anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            direct_deepseek_api_key: std::env::var("DEEPSEEK_API_KEY").ok(),
            direct_mistral_api_key: std::env::var("MISTRAL_API_KEY").ok(),
            direct_openrouter_api_key: std::env::var("OPENROUTER_API_KEY").ok(),
            commandcode_api_key: std::env::var("COMMANDCODE_API_KEY").ok(),
            commandcode_base_url: Some(std::env::var("COMMANDCODE_BASE_URL").unwrap_or_else(|_| "https://api.commandcode.ai/v1".to_string())),
            ollama_url: Some("http://localhost:11434/v1".to_string()),
            lmstudio_url: Some("http://localhost:1234/v1".to_string()),
            llamacpp_url: Some("http://localhost:8080/v1".to_string()),
            custom_endpoints: vec![
                CustomEndpoint {
                    id: "lmstudio".to_string(),
                    name: "LM Studio Local (Port 1234)".to_string(),
                    base_url: "http://localhost:1234/v1".to_string(),
                    api_key: None,
                    default_model: Some("local-model".to_string()),
                },
                CustomEndpoint {
                    id: "llamacpp".to_string(),
                    name: "Llama.cpp Server (Port 8080)".to_string(),
                    base_url: "http://localhost:8080/v1".to_string(),
                    api_key: None,
                    default_model: Some("default".to_string()),
                },
                CustomEndpoint {
                    id: "vllm".to_string(),
                    name: "vLLM High-Throughput (Port 8000)".to_string(),
                    base_url: "http://localhost:8000/v1".to_string(),
                    api_key: None,
                    default_model: Some("vllm-model".to_string()),
                },
            ],
            storage_mode: "central".to_string(),
            theme: "opencode".to_string(),
            language: "en".to_string(),
            git_sync_mode: "both".to_string(),
            favorite_models: vec![
                "cursor-claude-3-7-sonnet".to_string(),
                "opencode-zen".to_string(),
                "opencode-go".to_string(),
                "antigravity-claude-3-7".to_string(),
                "commandcode-claude-3-7-sonnet".to_string(),
                "lmstudio/local-model".to_string(),
                "gemini-3.7-flash".to_string(),
                "groq-qwen-coder".to_string(),
            ],
            web_companion_enabled: true,
            web_companion_port: 7711,
            mcp_config_path: Some(".opencode/mcp.json".to_string()),
            voice_plugin_command: None,
            trust_mode: false,
            agent_hooks_enabled: true,
            spec_output_dir: ".kiro/specs".to_string(),
            sync_server_url: std::env::var("OPENCODE_SYNC_URL").ok(),
            sync_token: std::env::var("OPENCODE_SYNC_TOKEN").ok(),
            sync_auto: false,
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        if let Some(proj_dirs) = directories::ProjectDirs::from("com", "opencode", "opencode-rs") {
            let config_dir = proj_dirs.config_dir();
            std::fs::create_dir_all(config_dir).ok();
            return config_dir.join("config.json");
        }
        PathBuf::from("opencode_config.json")
    }

    pub fn project_config_path(work_dir: &Path) -> PathBuf {
        work_dir.join(".opencode").join("config.json")
    }

    pub fn load_for_project(work_dir: &Path) -> Self {
        let mut cfg = Self::load();

        // Sprawdź czy projekt ma lokalny plik .opencode/config.json
        let local_path = Self::project_config_path(work_dir);
        if local_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&local_path) {
                if let Ok(local_cfg) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(mode) = local_cfg.get("git_sync_mode").and_then(|m| m.as_str()) {
                        cfg.git_sync_mode = mode.to_string();
                    }
                    if let Some(model) = local_cfg.get("default_model").and_then(|m| m.as_str()) {
                        cfg.default_model = model.to_string();
                    }
                    if let Some(theme) = local_cfg.get("theme").and_then(|m| m.as_str()) {
                        cfg.theme = theme.to_string();
                    }
                    if let Some(lang) = local_cfg.get("language").and_then(|m| m.as_str()) {
                        cfg.language = lang.to_string();
                    }
                }
            }
        }

        cfg
    }

    pub fn save_project_config(&self, work_dir: &Path) -> anyhow::Result<()> {
        let local_path = Self::project_config_path(work_dir);
        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(local_path, json)?;
        Ok(())
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str::<AppConfig>(&content) {
                    return cfg;
                }
            }
        }
        let default_cfg = Self::default();
        default_cfg.save().ok();
        default_cfg
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path();
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}
