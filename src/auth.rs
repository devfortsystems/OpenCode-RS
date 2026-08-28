use anyhow::{anyhow, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::AppConfig;

pub struct AuthManager;

impl AuthManager {
    /// Zwraca standardową ścieżkę do globalnego pliku .env użytkownika
    pub fn global_env_path() -> PathBuf {
        if let Some(proj_dirs) = directories::ProjectDirs::from("com", "opencode", "opencode-rs") {
            let config_dir = proj_dirs.config_dir();
            fs::create_dir_all(config_dir).ok();
            return config_dir.join(".env");
        }
        PathBuf::from(".env")
    }

    /// Wczytuje zmienne z .env z hierarchii katalogów:
    /// 1. Globalny .env (~/.config/opencode-rs/.env)
    /// 2. Plik ~/.opencode.env w katalogu domowym
    /// 3. Plik .env w bieżącym katalogu projektu
    pub fn auto_load_credentials(work_dir: &Path) {
        // 1. Globalny config dir .env
        let global_env = Self::global_env_path();
        if global_env.exists() {
            dotenvy::from_path(&global_env).ok();
        }

        // 2. Katalog domowy ~/.opencode.env oraz ~/.commandcode/auth.json
        if let Some(user_dirs) = directories::UserDirs::new() {
            let home = user_dirs.home_dir();
            let home_env = home.join(".opencode.env");
            if home_env.exists() {
                dotenvy::from_path(&home_env).ok();
            }

            // Automatyczny odczyt klucza z ~/.commandcode/auth.json
            let cmdcode_auth = home.join(".commandcode").join("auth.json");
            if cmdcode_auth.exists() {
                if let Ok(content) = fs::read_to_string(&cmdcode_auth) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(key) = val.get("command-code").and_then(|c| c.get("key")).and_then(|k| k.as_str()) {
                            if std::env::var("COMMANDCODE_API_KEY").is_err() {
                                std::env::set_var("COMMANDCODE_API_KEY", key);
                            }
                        }
                    }
                }
            }
        }

        // 3. Lokalny projekt .env
        let local_env = work_dir.join(".env");
        if local_env.exists() {
            dotenvy::from_path(&local_env).ok();
        } else {
            // Spróbuj domyślnego wyszukiwania w górę drzewa katalogów
            dotenvy::dotenv().ok();
        }
    }

    /// Eksportuje wszystkie aktywne tokeny autoryzacyjne i klucze do 1 pliku .env
    pub fn export_to_env(output_path: &Path, config: &AppConfig) -> Result<()> {
        let mut lines = Vec::new();
        lines.push("# ========================================================".to_string());
        lines.push("# ⚡ OpenCode-RS Universal Credentials & Auth File".to_string());
        lines.push("# Skopiuj ten jeden plik na dowolny komputer (do projektu lub ~/.config/opencode-rs/.env)".to_string());
        lines.push("# ========================================================\n".to_string());

        let mut write_key = |key: &str, val: Option<&String>, comment: &str| {
            lines.push(format!("# {}", comment));
            if let Some(v) = val {
                if !v.trim().is_empty() {
                    lines.push(format!("{}={}", key, v.trim()));
                } else {
                    lines.push(format!("#{key}="));
                }
            } else if let Ok(env_val) = std::env::var(key) {
                if !env_val.trim().is_empty() {
                    lines.push(format!("{}={}", key, env_val.trim()));
                } else {
                    lines.push(format!("#{key}="));
                }
            } else {
                lines.push(format!("#{key}="));
            }
            lines.push("".to_string());
        };

        // Direct AI APIs
        write_key("GEMINI_API_KEY", config.direct_gemini_api_key.as_ref(), "Google Gemini AI Direct API Key");
        write_key("GROQ_API_KEY", config.direct_groq_api_key.as_ref(), "Groq Cloud API Key (Qwen / Llama 1000 tok/s)");
        write_key("OPENAI_API_KEY", config.direct_openai_api_key.as_ref(), "OpenAI API Key");
        write_key("ANTHROPIC_API_KEY", config.direct_anthropic_api_key.as_ref(), "Anthropic Claude API Key");
        write_key("DEEPSEEK_API_KEY", config.direct_deepseek_api_key.as_ref(), "DeepSeek API Key");
        write_key("MISTRAL_API_KEY", config.direct_mistral_api_key.as_ref(), "Mistral AI API Key");
        write_key("OPENROUTER_API_KEY", config.direct_openrouter_api_key.as_ref(), "OpenRouter API Key (200+ models)");
        write_key("COMMANDCODE_API_KEY", config.commandcode_api_key.as_ref(), "Command Code API Key (user_...)");

        // Edytory & IDE Bridge Auth Tokens
        write_key("CURSOR_AUTH_TOKEN", None, "Cursor IDE Pro Session Token / Auth Key");
        write_key("WINDSURF_AUTH_TOKEN", None, "Windsurf Cascade AI Auth Token");
        write_key("TRAE_AUTH_TOKEN", None, "Trae AI Pro Token");
        write_key("COPILOT_AUTH_TOKEN", None, "GitHub Copilot OAuth / Access Token");

        // Sync Server
        write_key("OPENCODE_SYNC_URL", config.sync_server_url.as_ref(), "Adres zdalnego serwera synchronizacji czatów");
        write_key("OPENCODE_SYNC_TOKEN", config.sync_token.as_ref(), "Token autoryzacji do serwera synchronizacji");

        // URLs
        if let Some(ref ollama) = config.ollama_url {
            lines.push("# Adres lokalnego serwera Ollama".to_string());
            lines.push(format!("OLLAMA_URL={}", ollama));
            lines.push("".to_string());
        }

        if let Some(parent) = output_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        fs::write(output_path, lines.join("\n"))?;
        Ok(())
    }

    /// Wczytuje dane z pliku .env i aktualizuje AppConfig oraz bieżące środowisko
    pub fn import_from_env(input_path: &Path, config: &mut AppConfig) -> Result<usize> {
        if !input_path.exists() {
            return Err(anyhow!("Plik .env nie istnieje: {:?}", input_path));
        }

        let content = fs::read_to_string(input_path)?;
        let mut count = 0;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim().to_uppercase();
                let val = v.trim().trim_matches('"').trim_matches('\'').to_string();

                if !val.is_empty() {
                    std::env::set_var(&key, &val);
                    match key.as_str() {
                        "GEMINI_API_KEY" => config.direct_gemini_api_key = Some(val),
                        "GROQ_API_KEY" => config.direct_groq_api_key = Some(val),
                        "OPENAI_API_KEY" => config.direct_openai_api_key = Some(val),
                        "ANTHROPIC_API_KEY" => config.direct_anthropic_api_key = Some(val),
                        "DEEPSEEK_API_KEY" => config.direct_deepseek_api_key = Some(val),
                        "MISTRAL_API_KEY" => config.direct_mistral_api_key = Some(val),
                        "OPENROUTER_API_KEY" => config.direct_openrouter_api_key = Some(val),
                        "COMMANDCODE_API_KEY" => config.commandcode_api_key = Some(val),
                        "OPENCODE_SYNC_URL" => config.sync_server_url = Some(val),
                        "OPENCODE_SYNC_TOKEN" => config.sync_token = Some(val),
                        "OLLAMA_URL" => config.ollama_url = Some(val),
                        _ => {}
                    }
                    count += 1;
                }
            }
        }

        config.save().ok();
        Ok(count)
    }

    /// Zwraca maskowany ciąg znaków (np. "AIzaSy...4x9A")
    pub fn mask_token(token: &str) -> String {
        let t = token.trim();
        if t.is_empty() {
            return "[brak]".to_string();
        }
        if t.len() <= 8 {
            return "********".to_string();
        }
        format!("{}...{}", &t[..6], &t[t.len() - 4..])
    }

    /// Generuje podsumowanie stanu autoryzacji do wyświetlenia w czacie
    pub fn get_auth_status_report(config: &AppConfig, work_dir: &Path) -> String {
        let global_env = Self::global_env_path();
        let local_env = work_dir.join(".env");

        let mut lines = Vec::new();
        lines.push("🔐 Stan Autoryzacji i Kluczy OpenCode-RS:".to_string());
        lines.push(format!("• Plik .env projektu: {} ({})", local_env.display(), if local_env.exists() { "✅ aktywny" } else { "brak" }));
        lines.push(format!("• Globalny .env: {} ({})", global_env.display(), if global_env.exists() { "✅ aktywny" } else { "brak" }));
        lines.push("".to_string());

        let check_key = |key: &str, val: Option<&String>| -> String {
            if let Some(v) = val {
                if !v.is_empty() {
                    return format!("✅ {}", Self::mask_token(v));
                }
            }
            if let Ok(env_val) = std::env::var(key) {
                if !env_val.is_empty() {
                    return format!("✅ {}", Self::mask_token(&env_val));
                }
            }
            "❌ Brak".to_string()
        };

        lines.push("  [Direct APIs]".to_string());
        lines.push(format!("  - Google Gemini: {}", check_key("GEMINI_API_KEY", config.direct_gemini_api_key.as_ref())));
        lines.push(format!("  - Groq Cloud:    {}", check_key("GROQ_API_KEY", config.direct_groq_api_key.as_ref())));
        lines.push(format!("  - OpenAI:        {}", check_key("OPENAI_API_KEY", config.direct_openai_api_key.as_ref())));
        lines.push(format!("  - Anthropic:     {}", check_key("ANTHROPIC_API_KEY", config.direct_anthropic_api_key.as_ref())));
        lines.push(format!("  - DeepSeek:      {}", check_key("DEEPSEEK_API_KEY", config.direct_deepseek_api_key.as_ref())));
        lines.push(format!("  - Mistral:       {}", check_key("MISTRAL_API_KEY", config.direct_mistral_api_key.as_ref())));
        lines.push(format!("  - OpenRouter:    {}", check_key("OPENROUTER_API_KEY", config.direct_openrouter_api_key.as_ref())));
        lines.push(format!("  - Command Code:  {}", check_key("COMMANDCODE_API_KEY", config.commandcode_api_key.as_ref())));
        lines.push("".to_string());

        lines.push("  [Editor Tokens]".to_string());
        lines.push(format!("  - Cursor IDE:    {}", check_key("CURSOR_AUTH_TOKEN", None)));
        lines.push(format!("  - Windsurf:      {}", check_key("WINDSURF_AUTH_TOKEN", None)));
        lines.push(format!("  - Trae AI:       {}", check_key("TRAE_AUTH_TOKEN", None)));
        lines.push(format!("  - Copilot:       {}", check_key("COPILOT_AUTH_TOKEN", None)));
        lines.push("".to_string());

        lines.push("  [Sync Server]".to_string());
        lines.push(format!("  - Sync URL:      {}", config.sync_server_url.as_deref().unwrap_or("[nieustawiony]")));
        lines.push(format!("  - Sync Token:    {}", check_key("OPENCODE_SYNC_TOKEN", config.sync_token.as_ref())));
        lines.push("".to_string());

        lines.push("💡 Komendy zarządzania .env:".to_string());
        lines.push("  • /auth export-env [sciezka/.env] - Zapisz gotowy plik .env do przeniesienia".to_string());
        lines.push("  • /auth import-env <sciezka/.env> - Wczytaj klucze z pliku .env".to_string());
        lines.push("  • /auth set <NAZWA_KLUCZA> <WARTOSC> - Ustaw klucz i zapisz w .env".to_string());

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_export_and_import_env() {
        let temp_dir = std::env::temp_dir().join(format!("opencode_auth_{}", Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).unwrap();
        let env_file = temp_dir.join(".env");

        let mut config = AppConfig::default();
        config.direct_gemini_api_key = Some("AIzaSyTestKey123456789".to_string());
        config.direct_groq_api_key = Some("gsk_TestGroqKey987654321".to_string());
        config.sync_token = Some("secret_sync_token_abc".to_string());

        // 1. Eksportuj do .env
        assert!(AuthManager::export_to_env(&env_file, &config).is_ok());
        assert!(env_file.exists());

        // 2. Wczytaj w nowym konfigu
        let mut new_config = AppConfig::default();
        new_config.direct_gemini_api_key = None;
        new_config.direct_groq_api_key = None;
        new_config.sync_token = None;

        let count = AuthManager::import_from_env(&env_file, &mut new_config).unwrap();
        assert!(count >= 3);
        assert_eq!(new_config.direct_gemini_api_key.as_deref(), Some("AIzaSyTestKey123456789"));
        assert_eq!(new_config.direct_groq_api_key.as_deref(), Some("gsk_TestGroqKey987654321"));
        assert_eq!(new_config.sync_token.as_deref(), Some("secret_sync_token_abc"));

        // Posprzątaj
        fs::remove_dir_all(&temp_dir).ok();
    }
}
