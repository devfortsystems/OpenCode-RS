use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::AppConfig;

pub struct OpenCodeMigration;

impl OpenCodeMigration {
    /// Zwraca listę potencjalnych katalogów danych opencode-rs oraz oryginalnego OpenCode
    pub fn find_opencode_data_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        // 0. Katalog binarki wykonywalnej (tryb przenośny)
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                dirs.push(exe_dir.to_path_buf());
            }
        }

        if let Some(user_dirs) = directories::UserDirs::new() {
            let home = user_dirs.home_dir();
            dirs.push(home.join(".opencode-rs"));
            dirs.push(home.join(".local").join("share").join("opencode-rs"));
            dirs.push(home.join(".config").join("opencode-rs"));
            dirs.push(home.join(".local").join("share").join("opencode"));
            dirs.push(home.join(".config").join("opencode"));
            dirs.push(home.join(".opencode"));
            dirs.push(home.join(".commandcode"));
        }

        if let Some(proj_dirs) = directories::BaseDirs::new() {
            dirs.push(proj_dirs.data_dir().join("opencode-rs"));
            dirs.push(proj_dirs.config_dir().join("opencode-rs"));
            dirs.push(proj_dirs.data_dir().join("opencode"));
            dirs.push(proj_dirs.data_local_dir().join("opencode"));
            dirs.push(proj_dirs.config_dir().join("opencode"));
        }

        // Filtruj tylko istniejące katalogi
        dirs.into_iter().filter(|d| d.exists()).collect()
    }

    /// Wczytuje klucze autoryzacyjne z oryginalnego OpenCode (auth.json)
    pub fn import_credentials(config: &mut AppConfig) -> usize {
        let mut count = 0;
        let data_dirs = Self::find_opencode_data_dirs();

        for data_dir in data_dirs {
            let auth_file = data_dir.join("auth.json");
            if auth_file.exists() {
                if let Ok(content) = fs::read_to_string(&auth_file) {
                    if let Ok(json) = serde_json::from_str::<Value>(&content) {
                        if let Some(obj) = json.as_object() {
                            for (provider, details) in obj {
                                let key_opt = details.get("key").and_then(|k| k.as_str())
                                    .or_else(|| details.get("apiKey").and_then(|k| k.as_str()))
                                    .or_else(|| details.get("token").and_then(|k| k.as_str()));

                                if let Some(key) = key_opt {
                                    let key_str = key.trim().to_string();
                                    if key_str.is_empty() {
                                        continue;
                                    }

                                    match provider.to_lowercase().as_str() {
                                        "google" | "gemini" => {
                                            if config.direct_gemini_api_key.is_none() {
                                                config.direct_gemini_api_key = Some(key_str.clone());
                                                std::env::set_var("GEMINI_API_KEY", &key_str);
                                                count += 1;
                                            }
                                        }
                                        "command-code" | "commandcode" => {
                                            if config.commandcode_api_key.is_none() {
                                                config.commandcode_api_key = Some(key_str.clone());
                                                std::env::set_var("COMMANDCODE_API_KEY", &key_str);
                                                count += 1;
                                            }
                                        }
                                        "groq" => {
                                            if config.direct_groq_api_key.is_none() {
                                                config.direct_groq_api_key = Some(key_str.clone());
                                                std::env::set_var("GROQ_API_KEY", &key_str);
                                                count += 1;
                                            }
                                        }
                                        "openai" => {
                                            if config.direct_openai_api_key.is_none() {
                                                config.direct_openai_api_key = Some(key_str.clone());
                                                std::env::set_var("OPENAI_API_KEY", &key_str);
                                                count += 1;
                                            }
                                        }
                                        "anthropic" | "claude" => {
                                            if config.direct_anthropic_api_key.is_none() {
                                                config.direct_anthropic_api_key = Some(key_str.clone());
                                            }
                                            std::env::set_var("ANTHROPIC_API_KEY", &key_str);
                                            count += 1;
                                        }
                                        "openrouter" => {
                                            if config.direct_openrouter_api_key.is_none() {
                                                config.direct_openrouter_api_key = Some(key_str.clone());
                                            }
                                            std::env::set_var("OPENROUTER_API_KEY", &key_str);
                                            count += 1;
                                        }
                                        "deepseek" => {
                                            if config.direct_deepseek_api_key.is_none() {
                                                config.direct_deepseek_api_key = Some(key_str.clone());
                                            }
                                            std::env::set_var("DEEPSEEK_API_KEY", &key_str);
                                            count += 1;
                                        }
                                        "mistral" => {
                                            if config.direct_mistral_api_key.is_none() {
                                                config.direct_mistral_api_key = Some(key_str.clone());
                                            }
                                            std::env::set_var("MISTRAL_API_KEY", &key_str);
                                            count += 1;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if count > 0 {
            config.save().ok();
        }
        count
    }

    /// Wczytuje niestandardowe providery z opencode.jsonc / opencode.json — pełna auto-migracja
    pub fn import_providers(config: &mut AppConfig, project_dir: &Path) -> Vec<String> {
        let mut loaded_providers = Vec::new();

        let mut config_files = Vec::new();
        for dir in Self::find_opencode_data_dirs() {
            config_files.push(dir.join("opencode.jsonc"));
            config_files.push(dir.join("opencode.json"));
        }
        config_files.push(project_dir.join(".opencode").join("opencode.json"));
        config_files.push(project_dir.join(".opencode").join("opencode.jsonc"));

        for path in config_files {
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    let clean = strip_json_comments(&content);
                    if let Ok(val) = serde_json::from_str::<Value>(&clean) {
                        // 1. Providerzy OpenAI-compatible → custom_endpoints
                        if let Some(providers) = val.get("provider").and_then(|p| p.as_object()) {
                            for (p_name, p_val) in providers {
                                let name = p_val.get("name").and_then(|n| n.as_str()).unwrap_or(p_name);
                                let base_url = p_val.get("options").and_then(|o| o.get("baseURL")).and_then(|u| u.as_str());
                                let api_key = p_val.get("options").and_then(|o| o.get("apiKey")).and_then(|k| k.as_str()).map(|s| s.to_string());
                                let model = p_val.get("options").and_then(|o| o.get("model")).and_then(|m| m.as_str()).map(|s| s.to_string());
                                if let Some(url) = base_url {
                                    // Dodaj jako custom_endpoint jeśli nie istnieje
                                    if !config.custom_endpoints.iter().any(|e| e.id == *p_name) {
                                        config.custom_endpoints.push(crate::config::CustomEndpoint {
                                            id: p_name.clone(),
                                            name: name.to_string(),
                                            base_url: url.to_string(),
                                            api_key,
                                            default_model: model,
                                        });
                                        loaded_providers.push(format!("{name} ({url}) [auto-import]"));
                                    }
                                }
                            }
                        }
                        // 2. Theme / język / model domyślny — pełna migracja
                        if let Some(m) = val.get("theme").and_then(|v| v.as_str()) { config.theme = m.to_string(); }
                        if let Some(m) = val.get("language").and_then(|v| v.as_str()) { config.language = m.to_string(); }
                        if let Some(m) = val.get("default_model").or_else(|| val.get("model")).and_then(|v| v.as_str()) { config.default_model = m.to_string(); }
                        if let Some(b) = val.get("bridge_url").and_then(|v| v.as_str()) { config.bridge_url = b.to_string(); }
                    }
                }
            }
        }
        if !loaded_providers.is_empty() { config.save().ok(); }
        loaded_providers
    }

    /// Pełna auto-migracja sesji z opencode.db (SQLite) — importuje istniejące sesje
    pub fn import_sessions(project_dir: &Path) -> Vec<String> {
        let mut imported = Vec::new();
        for dir in Self::find_opencode_data_dirs() {
            let db = dir.join("opencode.db");
            if db.exists() {
                imported.push(format!("opencode.db: {} (auto-odczyt via SessionManager)", db.display()));
                // Sesje są w SQLite — SessionManager w opencode-rs czyta je lazy przy list_sessions()
                // Tu tylko sygnalizujemy wykrycie, pełny import robi SessionManager::import_from_opencode_db()
            }
        }
        // Projektowy .opencode/config.json → AppConfig już zrobiony w load_for_project()
        let proj_cfg = project_dir.join(".opencode").join("config.json");
        if proj_cfg.exists() { imported.push(format!("project config: {}", proj_cfg.display())); }
        imported
    }

    /// Generuje raport o wykrytych danych z oryginalnego OpenCode
    pub fn inspect_opencode_data(project_dir: &Path) -> String {
        let dirs = Self::find_opencode_data_dirs();
        let project_cfg = project_dir.join(".opencode").join("opencode.json");

        let mut lines = Vec::new();
        lines.push("🔎 Wykryte dane i klucze oryginalnego OpenCode w systemie:".to_string());

        if dirs.is_empty() {
            lines.push("• Katalog danych: ❌ Nie znaleziono katalogów OpenCode w ~/.local/share lub %APPDATA%".to_string());
        } else {
            for d in &dirs {
                lines.push(format!("• Katalog: {}", d.display()));
                let auth = d.join("auth.json");
                lines.push(format!("  - auth.json: {}", if auth.exists() { "✅ Znaleziono klucze API" } else { "brak" }));
                let db = d.join("opencode.db");
                lines.push(format!("  - opencode.db: {}", if db.exists() { "✅ Znaleziono bazę SQLite z sesjami" } else { "brak" }));
            }
        }

        lines.push(format!("• Konfiguracja tego projektu: {} ({})", project_cfg.display(), if project_cfg.exists() { "✅ Obecna" } else { "brak" }));
        lines.push("".to_string());
        lines.push("💡 OpenCode-RS automatycznie wczytuje i korzysta z powyższych kluczy bez potrzeby ponownej konfiguracji!".to_string());

        lines.join("\n")
    }

    // ─── OPENCLAW IMPORTER ────────────────────────────────────────────────────

    /// Zwraca katalogi danych OpenClaw
    pub fn find_openclaw_data_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(user_dirs) = directories::UserDirs::new() {
            let home = user_dirs.home_dir();
            dirs.push(home.join(".openclaw"));
        }
        // Windows: %USERPROFILE%\.openclaw
        if let Ok(profile) = std::env::var("USERPROFILE") {
            dirs.push(PathBuf::from(profile).join(".openclaw"));
        }
        dirs.into_iter().filter(|d| d.exists()).collect()
    }

    /// Importuje klucze API z OpenClaw (openclaw.json + .env)
    pub fn import_from_openclaw(config: &mut AppConfig) -> (usize, Vec<String>) {
        let mut count = 0;
        let mut sources = Vec::new();

        for dir in Self::find_openclaw_data_dirs() {
            // 1. openclaw.json → pole "env": { "OPENAI_API_KEY": "sk-..." }
            let cfg_file = dir.join("openclaw.json");
            if let Ok(content) = fs::read_to_string(&cfg_file) {
                if let Ok(json) = serde_json::from_str::<Value>(&content) {
                    if let Some(env_obj) = json.get("env").and_then(|e| e.as_object()) {
                        for (key, val) in env_obj {
                            if let Some(v) = val.as_str() {
                                count += apply_env_key(key, v, config);
                                sources.push(format!("openclaw.json → {key}"));
                            }
                        }
                    }
                }
            }

            // 2. ~/.openclaw/.env
            let env_file = dir.join(".env");
            if let Ok(content) = fs::read_to_string(&env_file) {
                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') { continue; }
                    if let Some((k, v)) = line.split_once('=') {
                        count += apply_env_key(k.trim(), v.trim().trim_matches('"'), config);
                        sources.push(format!("openclaw/.env → {}", k.trim()));
                    }
                }
            }
        }

        if count > 0 { config.save().ok(); }
        (count, sources)
    }

    // ─── KILO CODE IMPORTER ───────────────────────────────────────────────────

    /// Importuje klucze i endpointy z Kilo Code (kilo.jsonc)
    pub fn import_from_kilo_code(config: &mut AppConfig, project_dir: &Path) -> (usize, Vec<String>) {
        let mut count = 0;
        let mut sources = Vec::new();

        let mut candidates = vec![
            project_dir.join("kilo.jsonc"),
            project_dir.join(".vscode").join("kilo.jsonc"),
        ];

        // ~/.kilo/settings.json lub %USERPROFILE%\.kilo\
        if let Some(user_dirs) = directories::UserDirs::new() {
            let home = user_dirs.home_dir();
            candidates.push(home.join(".kilo").join("settings.json"));
            candidates.push(home.join(".kilo").join("kilo.jsonc"));
        }

        for path in candidates {
            if !path.exists() { continue; }
            if let Ok(content) = fs::read_to_string(&path) {
                let clean = strip_json_comments(&content);
                if let Ok(json) = serde_json::from_str::<Value>(&clean) {
                    // providers: [{ provider: "openai", apiKey: "...", baseURL: "..." }]
                    if let Some(providers) = json.get("providers").and_then(|p| p.as_array()) {
                        for prov in providers {
                            let provider_id = prov.get("provider").and_then(|p| p.as_str()).unwrap_or("unknown");
                            let api_key = prov.get("apiKey").or_else(|| prov.get("api_key")).and_then(|k| k.as_str()).unwrap_or("");
                            let base_url = prov.get("baseURL").or_else(|| prov.get("base_url")).and_then(|u| u.as_str());

                            if !api_key.is_empty() {
                                count += apply_env_key(&format!("{}_API_KEY", provider_id.to_uppercase()), api_key, config);
                                sources.push(format!("kilo.jsonc → {provider_id} apiKey"));
                            }

                            if let Some(url) = base_url {
                                let ep_id = format!("kilo-{provider_id}");
                                if !config.custom_endpoints.iter().any(|e| e.id == ep_id) {
                                    config.custom_endpoints.push(crate::config::CustomEndpoint {
                                        id: ep_id.clone(),
                                        name: format!("Kilo Code: {provider_id}"),
                                        base_url: url.to_string(),
                                        api_key: if api_key.is_empty() { None } else { Some(api_key.to_string()) },
                                        default_model: prov.get("model").and_then(|m| m.as_str()).map(|s| s.to_string()),
                                    });
                                    count += 1;
                                    sources.push(format!("kilo.jsonc → {provider_id} endpoint ({url})"));
                                }
                            }
                        }
                    }
                }
            }
        }

        if count > 0 { config.save().ok(); }
        (count, sources)
    }

    // ─── KIRO CODE (AWS) IMPORTER ─────────────────────────────────────────────

    /// Zwraca katalogi danych Kiro (AWS IDE)
    pub fn find_kiro_data_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(user_dirs) = directories::UserDirs::new() {
            let home = user_dirs.home_dir();
            dirs.push(home.join(".kiro"));
        }
        if let Ok(profile) = std::env::var("USERPROFILE") {
            dirs.push(PathBuf::from(profile).join(".kiro"));
        }
        dirs.into_iter().filter(|d| d.exists()).collect()
    }

    /// Importuje modele, klucze i konfigurację z Kiro (AWS IDE)
    pub fn import_from_kiro(config: &mut AppConfig) -> (usize, Vec<String>) {
        let mut count = 0;
        let mut sources = Vec::new();

        for kiro_dir in Self::find_kiro_data_dirs() {
            // ~/.kiro/settings/models.json
            let models_file = kiro_dir.join("settings").join("models.json");
            if let Ok(content) = fs::read_to_string(&models_file) {
                if let Ok(json) = serde_json::from_str::<Value>(&content) {
                    if let Some(providers) = json.get("providers").and_then(|p| p.as_object()) {
                        for (provider, pval) in providers {
                            let api_key = pval.get("apiKey").and_then(|k| k.as_str()).unwrap_or("");
                            if !api_key.is_empty() {
                                count += apply_env_key(&format!("{}_API_KEY", provider.to_uppercase()), api_key, config);
                                sources.push(format!("kiro models.json → {provider}"));
                            }
                        }
                    }
                }
            }

            // ~/.kiro/settings/mcp.json → merge z .opencode/mcp.json
            let mcp_file = kiro_dir.join("settings").join("mcp.json");
            if mcp_file.exists() {
                sources.push(format!("kiro mcp.json: {} [dostępny jako .opencode/mcp.json]", mcp_file.display()));
            }
        }

        if count > 0 { config.save().ok(); }
        (count, sources)
    }

    /// Wczytuje reguły stylu z Kiro Agent Hooks i Steering files
    pub fn load_kiro_rules(project_dir: &Path) -> String {
        let mut rules = String::new();

        // .kiro/hooks/*.md
        let hooks_dir = project_dir.join(".kiro").join("hooks");
        if let Ok(entries) = fs::read_dir(&hooks_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "md") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let fname = path.file_name().unwrap_or_default().to_string_lossy();
                        rules.push_str(&format!("\n[Kiro Agent Hook: {fname}]:\n{}\n", content.trim()));
                    }
                }
            }
        }

        // .kiro/steering/*.md
        let steering_dir = project_dir.join(".kiro").join("steering");
        if let Ok(entries) = fs::read_dir(&steering_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "md") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let fname = path.file_name().unwrap_or_default().to_string_lossy();
                        rules.push_str(&format!("\n[Kiro Steering: {fname}]:\n{}\n", content.trim()));
                    }
                }
            }
        }

        rules
    }

    /// Zbiorczy raport z importu wszystkich narzędzi
    pub fn import_all(config: &mut AppConfig, project_dir: &Path) -> String {
        let mut lines = vec!["🔄 **Import konfiguracji ze wszystkich wykrytych narzędzi AI:**\n".to_string()];

        // OpenCode
        let oc_count = Self::import_credentials(config);
        let oc_providers = Self::import_providers(config, project_dir);
        lines.push(format!("✅ **OpenCode**: {} kluczy API, {} providerów", oc_count, oc_providers.len()));

        // OpenClaw
        let (claw_count, claw_sources) = Self::import_from_openclaw(config);
        if claw_count > 0 {
            lines.push(format!("✅ **OpenClaw**: {} kluczy API ({})", claw_count, claw_sources.join(", ")));
        } else {
            lines.push("⚪ **OpenClaw**: Brak `~/.openclaw/` – niezainstalowany".to_string());
        }

        // Kilo Code
        let (kilo_count, kilo_sources) = Self::import_from_kilo_code(config, project_dir);
        if kilo_count > 0 {
            lines.push(format!("✅ **Kilo Code**: {} wpisów ({})", kilo_count, kilo_sources.join(", ")));
        } else {
            lines.push("⚪ **Kilo Code**: Brak `kilo.jsonc` – niezainstalowany lub brak konfiguracji".to_string());
        }

        // Kiro (AWS)
        let (kiro_count, kiro_sources) = Self::import_from_kiro(config);
        if kiro_count > 0 {
            lines.push(format!("✅ **Kiro (AWS)**: {} wpisów ({})", kiro_count, kiro_sources.join(", ")));
        } else {
            lines.push("⚪ **Kiro (AWS)**: Brak `~/.kiro/` – niezainstalowany".to_string());
        }

        lines.push("\n💡 Aby zaimportować konkretne narzędzie: `/import opencode|openclaw|kilo|kiro`".to_string());
        lines.join("\n")
    }
}

/// Pomocnik: aplikuje klucz API z nazwy zmiennej środowiskowej do AppConfig
fn apply_env_key(key: &str, value: &str, config: &mut AppConfig) -> usize {
    let value = value.trim().to_string();
    if value.is_empty() { return 0; }
    std::env::set_var(key, &value);
    match key.to_uppercase().as_str() {
        "OPENAI_API_KEY" => {
            if config.direct_openai_api_key.is_none() { config.direct_openai_api_key = Some(value); return 1; }
        }
        "ANTHROPIC_API_KEY" => {
            if config.direct_anthropic_api_key.is_none() { config.direct_anthropic_api_key = Some(value); return 1; }
        }
        "GEMINI_API_KEY" | "GOOGLE_API_KEY" => {
            if config.direct_gemini_api_key.is_none() { config.direct_gemini_api_key = Some(value); return 1; }
        }
        "GROQ_API_KEY" => {
            if config.direct_groq_api_key.is_none() { config.direct_groq_api_key = Some(value); return 1; }
        }
        "DEEPSEEK_API_KEY" => {
            if config.direct_deepseek_api_key.is_none() { config.direct_deepseek_api_key = Some(value); return 1; }
        }
        "OPENROUTER_API_KEY" => {
            if config.direct_openrouter_api_key.is_none() { config.direct_openrouter_api_key = Some(value); return 1; }
        }
        "MISTRAL_API_KEY" => {
            if config.direct_mistral_api_key.is_none() { config.direct_mistral_api_key = Some(value); return 1; }
        }
        "COMMANDCODE_API_KEY"
            if config.commandcode_api_key.is_none() => { config.commandcode_api_key = Some(value); return 1; }
        _ => {}
    }
    0
}

pub fn strip_json_comments(json: &str) -> String {
    let mut out = String::new();
    let mut in_string = false;
    let mut chars = json.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '"' {
            in_string = !in_string;
            out.push(c);
        } else if !in_string && c == '/' && chars.peek() == Some(&'/') {
            // Linia komentarza jednowierszowego
            for nc in chars.by_ref() {
                if nc == '\n' {
                    out.push('\n');
                    break;
                }
            }
        } else if !in_string && c == '/' && chars.peek() == Some(&'*') {
            // Blok komentarza wielowierszowego
            chars.next(); // pomiń *
            while let Some(nc) = chars.next() {
                if nc == '*' && chars.peek() == Some(&'/') {
                    chars.next(); // pomiń /
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_json_comments_line_comments() {
        let input = r#"{
            "key": "value", // to jest komentarz
            "num": 42
        }"#;
        let result = strip_json_comments(input);
        // Komentarz powinien być usunięty
        assert!(!result.contains("// to jest komentarz"));
        // Wartości powinny zostać
        assert!(result.contains("\"key\": \"value\""));
        assert!(result.contains("\"num\": 42"));
    }

    #[test]
    fn test_strip_json_comments_block_comments() {
        let input = r#"{
            /* blok komentarza */
            "key": "value"
        }"#;
        let result = strip_json_comments(input);
        assert!(!result.contains("blok komentarza"));
        assert!(result.contains("\"key\": \"value\""));
    }

    #[test]
    fn test_strip_json_comments_multiline_block() {
        let input = r#"{
            /* linia 1
               linia 2
               linia 3 */
            "key": "value"
        }"#;
        let result = strip_json_comments(input);
        assert!(!result.contains("linia 1"));
        assert!(!result.contains("linia 2"));
        assert!(!result.contains("linia 3"));
        assert!(result.contains("\"key\": \"value\""));
    }

    #[test]
    fn test_strip_json_comments_no_comments() {
        let input = r#"{"key": "value", "num": 42}"#;
        let result = strip_json_comments(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_strip_json_comments_preserves_strings() {
        // Komentarz wewnątrz stringa nie powinien być usunięty
        let input = r#"{"url": "http://example.com//path"}"#;
        let result = strip_json_comments(input);
        assert!(result.contains("http://example.com//path"));
    }

    #[test]
    fn test_strip_json_comments_empty_input() {
        assert_eq!(strip_json_comments(""), "");
    }

    #[test]
    fn test_find_opencode_data_dirs_returns_vec() {
        let dirs = OpenCodeMigration::find_opencode_data_dirs();
        // Może być puste jeśli nie ma oryginalnego opencode, ale powinno być Vec
        // (nie panic)
        let _ = dirs.len();
    }

    #[test]
    fn test_apply_env_key_openai() {
        let mut config = AppConfig::default();
        config.direct_openai_api_key = None;
        let count = apply_env_key("OPENAI_API_KEY", "sk-test123", &mut config);
        assert_eq!(count, 1);
        assert_eq!(config.direct_openai_api_key, Some("sk-test123".to_string()));
    }

    #[test]
    fn test_apply_env_key_anthropic() {
        let mut config = AppConfig::default();
        config.direct_anthropic_api_key = None;
        let count = apply_env_key("ANTHROPIC_API_KEY", "sk-ant-test", &mut config);
        assert_eq!(count, 1);
        assert_eq!(config.direct_anthropic_api_key, Some("sk-ant-test".to_string()));
    }

    #[test]
    fn test_apply_env_key_gemini() {
        let mut config = AppConfig::default();
        config.direct_gemini_api_key = None;
        let count = apply_env_key("GEMINI_API_KEY", "AIza-test", &mut config);
        assert_eq!(count, 1);
        assert_eq!(config.direct_gemini_api_key, Some("AIza-test".to_string()));
    }

    #[test]
    fn test_apply_env_key_google_alias() {
        let mut config = AppConfig::default();
        config.direct_gemini_api_key = None;
        let count = apply_env_key("GOOGLE_API_KEY", "AIza-google", &mut config);
        assert_eq!(count, 1);
        assert_eq!(config.direct_gemini_api_key, Some("AIza-google".to_string()));
    }

    #[test]
    fn test_apply_env_key_empty_value() {
        let mut config = AppConfig::default();
        let count = apply_env_key("OPENAI_API_KEY", "", &mut config);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_apply_env_key_already_set() {
        let mut config = AppConfig::default();
        config.direct_openai_api_key = Some("existing-key".to_string());
        let count = apply_env_key("OPENAI_API_KEY", "new-key", &mut config);
        // Nie powinno nadpisać istniejącego klucza
        assert_eq!(count, 0);
        assert_eq!(config.direct_openai_api_key, Some("existing-key".to_string()));
    }

    #[test]
    fn test_apply_env_key_unknown_key() {
        let mut config = AppConfig::default();
        let count = apply_env_key("UNKNOWN_API_KEY", "some-value", &mut config);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_apply_env_key_case_insensitive() {
        let mut config = AppConfig::default();
        config.direct_groq_api_key = None;
        let count = apply_env_key("groq_api_key", "gsk-test", &mut config);
        assert_eq!(count, 1);
        assert_eq!(config.direct_groq_api_key, Some("gsk-test".to_string()));
    }
}
