use anyhow::{anyhow, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::AppConfig;

pub struct AuthManager;

impl AuthManager {
    /// Zwraca standardową ścieżkę do globalnego pliku .env użytkownika (~/.opencode-rs/.env)
    pub fn global_env_path() -> PathBuf {
        if let Some(user_dirs) = directories::UserDirs::new() {
            let home = user_dirs.home_dir();
            let rs_env = home.join(".opencode-rs").join(".env");
            if rs_env.exists() {
                return rs_env;
            }
        }
        if let Some(proj_dirs) = directories::ProjectDirs::from("com", "opencode", "opencode-rs") {
            let config_dir = proj_dirs.config_dir();
            let p = config_dir.join(".env");
            if p.exists() {
                return p;
            }
        }
        if let Some(user_dirs) = directories::UserDirs::new() {
            let rs_dir = user_dirs.home_dir().join(".opencode-rs");
            fs::create_dir_all(&rs_dir).ok();
            return rs_dir.join(".env");
        }
        PathBuf::from(".env")
    }

    /// Zwraca mapę wszystkich aktywnych kluczy API (provider -> klucz)
    pub fn get_active_keys() -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();
        let providers = [
            ("openai", "OPENAI_API_KEY"),
            ("anthropic", "ANTHROPIC_API_KEY"),
            ("gemini", "GEMINI_API_KEY"),
            ("groq", "GROQ_API_KEY"),
            ("deepseek", "DEEPSEEK_API_KEY"),
            ("mistral", "MISTRAL_API_KEY"),
            ("openrouter", "OPENROUTER_API_KEY"),
            ("devin", "DEVIN_API_KEY"),
        ];
        for (prov, env_var) in providers {
            if let Ok(key) = std::env::var(env_var) {
                if !key.trim().is_empty() {
                    map.insert(prov.to_string(), key.trim().to_string());
                }
            }
        }
        map
    }

    /// Pomocnicza funkcja do wczytywania kluczy z pliku auth.json
    fn load_auth_json_file(path: &Path) {
        if !path.exists() {
            return;
        }
        if let Ok(content) = fs::read_to_string(path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(obj) = val.as_object() {
                    for (provider, details) in obj {
                        let key_opt = details.get("key").and_then(|k| k.as_str())
                            .or_else(|| details.get("apiKey").and_then(|k| k.as_str()))
                            .or_else(|| details.get("token").and_then(|k| k.as_str()));
                        if let Some(key) = key_opt {
                            let key_trimmed = key.trim();
                            if key_trimmed.is_empty() {
                                continue;
                            }
                            let env_var = match provider.to_lowercase().as_str() {
                                "openai" => "OPENAI_API_KEY",
                                "anthropic" | "claude" => "ANTHROPIC_API_KEY",
                                "google" | "gemini" => "GEMINI_API_KEY",
                                "groq" => "GROQ_API_KEY",
                                "deepseek" => "DEEPSEEK_API_KEY",
                                "mistral" => "MISTRAL_API_KEY",
                                "openrouter" => "OPENROUTER_API_KEY",
                                "command-code" | "commandcode" => "COMMANDCODE_API_KEY",
                                "devin" => "DEVIN_API_KEY",
                                _ => continue,
                            };
                            if std::env::var(env_var).is_err() {
                                std::env::set_var(env_var, key_trimmed);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Wczytuje zmienne z .env z hierarchii katalogów:
    /// 0. Katalog binarki wykonywalnej opencode.exe (tryb przenośny / portable)
    /// 1. Globalny .env w ~/.opencode-rs/.env lub ~/.config/opencode-rs/.env
    /// 2. Plik ~/.opencode-rs.env w katalogu domowym (oraz ~/.opencode-rs/auth.json)
    ///    + fallbacki do legacy ~/.opencode.env, ~/.opencode/auth.json, ~/.commandcode/auth.json
    /// 3. Plik .opencode-rs/.env w projekcie (oraz .env)
    pub fn auto_load_credentials(work_dir: &Path) {
        // 0. Tryb przenośny (portable) — plik .env lub auth.json w tym samym katalogu co opencode.exe
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                for candidate in &[".env", ".opencode-rs.env", "opencode.env"] {
                    let exe_env = exe_dir.join(candidate);
                    if exe_env.exists() {
                        dotenvy::from_path(&exe_env).ok();
                    }
                }
                Self::load_auth_json_file(&exe_dir.join("auth.json"));
            }
        }

        // 1. Globalny config dir .env (~/.opencode-rs/.env lub ProjectDirs)
        let global_env = Self::global_env_path();
        if global_env.exists() {
            dotenvy::from_path(&global_env).ok();
        }

        // 2. Katalog domowy użytkownika
        if let Some(user_dirs) = directories::UserDirs::new() {
            let home = user_dirs.home_dir();

            // a) Własne pliki opencode-rs (priorytet)
            let rs_env = home.join(".opencode-rs.env");
            if rs_env.exists() {
                dotenvy::from_path(&rs_env).ok();
            }
            Self::load_auth_json_file(&home.join(".opencode-rs").join("auth.json"));

            // b) Fallbacki odczytu z innych narzędzi (opencode, commandcode)
            let legacy_home_env = home.join(".opencode.env");
            if legacy_home_env.exists() {
                dotenvy::from_path(&legacy_home_env).ok();
            }
            Self::load_auth_json_file(&home.join(".opencode").join("auth.json"));
            Self::load_auth_json_file(&home.join(".commandcode").join("auth.json"));
        }

        // 3. Lokalny projekt (.opencode-rs/.env ma priorytet nad .env)
        let rs_local_env = work_dir.join(".opencode-rs").join(".env");
        if rs_local_env.exists() {
            dotenvy::from_path(&rs_local_env).ok();
        }
        Self::load_auth_json_file(&work_dir.join(".opencode-rs").join("auth.json"));

        let local_env = work_dir.join(".env");
        if local_env.exists() {
            dotenvy::from_path(&local_env).ok();
        } else {
            dotenvy::dotenv().ok();
        }
        Self::load_auth_json_file(&work_dir.join(".opencode").join("auth.json"));
    }

    /// Pomocnicza funkcja łącząca nowe klucze z istniejącym plikiem auth.json
    pub fn save_to_auth_json(path: &Path, keys: &std::collections::HashMap<String, String>) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut root_val = if path.exists() {
            fs::read_to_string(path)
                .ok()
                .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
                .unwrap_or_else(|| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };

        if let Some(obj) = root_val.as_object_mut() {
            for (provider, key) in keys {
                obj.insert(
                    provider.clone(),
                    serde_json::json!({
                        "key": key,
                        "type": "api"
                    }),
                );
            }
        }

        fs::write(path, serde_json::to_string_pretty(&root_val)?)?;
        Ok(())
    }

    /// Automatycznie zapisuje i synchronizuje wszystkie aktywne klucze API do:
    /// 1. C:\Users\<User>\AppData\Roaming\opencode\opencode-rs\ (auth.json i .env)
    /// 2. C:\Users\<User>\.opencode-rs\ (auth.json i .env)
    /// 3. Katalogu z binarką opencode.exe jeśli istnieje tam plik .env lub auth.json (tryb portable)
    /// Dzięki temu przeniesienie folderu .opencode-rs lub AppData przenosi komplet kluczy.
    pub fn persist_credentials(config: &AppConfig, work_dir: &Path) -> Result<()> {
        let mut keys: std::collections::HashMap<String, String> = std::collections::HashMap::new();

        let mut check_and_add = |provider: &str, env_var: &str, cfg_val: Option<&String>| {
            if let Some(v) = cfg_val {
                if !v.trim().is_empty() {
                    keys.insert(provider.to_string(), v.trim().to_string());
                    return;
                }
            }
            if let Ok(v) = std::env::var(env_var) {
                if !v.trim().is_empty() {
                    keys.insert(provider.to_string(), v.trim().to_string());
                }
            }
        };

        check_and_add("gemini", "GEMINI_API_KEY", config.direct_gemini_api_key.as_ref());
        check_and_add("openai", "OPENAI_API_KEY", config.direct_openai_api_key.as_ref());
        check_and_add("anthropic", "ANTHROPIC_API_KEY", config.direct_anthropic_api_key.as_ref());
        check_and_add("groq", "GROQ_API_KEY", config.direct_groq_api_key.as_ref());
        check_and_add("deepseek", "DEEPSEEK_API_KEY", config.direct_deepseek_api_key.as_ref());
        check_and_add("mistral", "MISTRAL_API_KEY", config.direct_mistral_api_key.as_ref());
        check_and_add("openrouter", "OPENROUTER_API_KEY", config.direct_openrouter_api_key.as_ref());
        check_and_add("command-code", "COMMANDCODE_API_KEY", config.commandcode_api_key.as_ref());
        check_and_add("devin", "DEVIN_API_KEY", config.devin_api_key.as_ref());

        if keys.is_empty() {
            return Ok(());
        }

        // 1. Tryb przenośny (obok opencode.exe)
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                if exe_dir.join(".env").exists() || exe_dir.join("auth.json").exists() {
                    let _ = Self::save_to_auth_json(&exe_dir.join("auth.json"), &keys);
                    let _ = Self::export_to_env(&exe_dir.join(".env"), config);
                }
            }
        }

        // 2. Windows AppData Roaming (C:\Users\<User>\AppData\Roaming\opencode\opencode-rs\)
        if let Some(proj_dirs) = directories::ProjectDirs::from("com", "opencode", "opencode-rs") {
            let config_dir = proj_dirs.config_dir();
            fs::create_dir_all(config_dir).ok();
            let _ = Self::save_to_auth_json(&config_dir.join("auth.json"), &keys);
            let _ = Self::export_to_env(&config_dir.join(".env"), config);
        }

        // 3. Domowy katalog ~/.opencode-rs/ (C:\Users\<User>\.opencode-rs\)
        if let Some(user_dirs) = directories::UserDirs::new() {
            let rs_dir = user_dirs.home_dir().join(".opencode-rs");
            fs::create_dir_all(&rs_dir).ok();
            let _ = Self::save_to_auth_json(&rs_dir.join("auth.json"), &keys);
            let _ = Self::export_to_env(&rs_dir.join(".env"), config);
        }

        // 4. Jeśli w bieżącym projekcie istnieje katalog .opencode-rs/, zaktualizuj go również
        let proj_rs_dir = work_dir.join(".opencode-rs");
        if proj_rs_dir.exists() {
            let _ = Self::save_to_auth_json(&proj_rs_dir.join("auth.json"), &keys);
        }

        Ok(())
    }

    /// Zapisuje podany klucz do konfiguracji i natychmiast synchronizuje pliki autoryzacji
    pub fn set_credential(provider: &str, key: &str, work_dir: &Path) -> Result<()> {
        let p_lower = provider.to_lowercase();
        let env_var = match p_lower.as_str() {
            "openai" => "OPENAI_API_KEY",
            "anthropic" | "claude" => "ANTHROPIC_API_KEY",
            "google" | "gemini" => "GEMINI_API_KEY",
            "groq" => "GROQ_API_KEY",
            "deepseek" => "DEEPSEEK_API_KEY",
            "mistral" => "MISTRAL_API_KEY",
            "openrouter" => "OPENROUTER_API_KEY",
            "command-code" | "commandcode" => "COMMANDCODE_API_KEY",
            "devin" => "DEVIN_API_KEY",
            _ => provider,
        };
        std::env::set_var(env_var, key);

        let mut config = AppConfig::load_for_project(work_dir);
        match env_var {
            "OPENAI_API_KEY" => config.direct_openai_api_key = Some(key.to_string()),
            "ANTHROPIC_API_KEY" => config.direct_anthropic_api_key = Some(key.to_string()),
            "GEMINI_API_KEY" => config.direct_gemini_api_key = Some(key.to_string()),
            "GROQ_API_KEY" => config.direct_groq_api_key = Some(key.to_string()),
            "DEEPSEEK_API_KEY" => config.direct_deepseek_api_key = Some(key.to_string()),
            "MISTRAL_API_KEY" => config.direct_mistral_api_key = Some(key.to_string()),
            "OPENROUTER_API_KEY" => config.direct_openrouter_api_key = Some(key.to_string()),
            "COMMANDCODE_API_KEY" => config.commandcode_api_key = Some(key.to_string()),
            "DEVIN_API_KEY" => config.devin_api_key = Some(key.to_string()),
            _ => {}
        }
        let _ = config.save();
        Self::persist_credentials(&config, work_dir)?;
        Ok(())
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
