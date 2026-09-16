use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomLocale {
    pub code: String,
    pub display_name: String,
    pub chat_title: String,
    pub welcome_msg: String,
    pub welcome_hint: String,
    pub input_placeholder: String,
    pub sidebar_title: String,
    pub status_ready: String,
    pub status_generating: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[derive(Default)]
pub enum Language {
    #[default]
    English,
    Polish,
    Chinese, // 简体中文
    German,
    Spanish,
    French,
    Ukrainian,
    Custom(Box<CustomLocale>),
}


impl Language {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "en" | "english" => Some(Self::English),
            "pl" | "polish" | "polski" => Some(Self::Polish),
            "zh" | "cn" | "chinese" | "中文" => Some(Self::Chinese),
            "de" | "german" | "deutsch" => Some(Self::German),
            "es" | "spanish" | "español" => Some(Self::Spanish),
            "fr" | "french" | "français" => Some(Self::French),
            "uk" | "ua" | "ukrainian" | "українська" => Some(Self::Ukrainian),
            _ => None,
        }
    }

    pub fn from_custom_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let custom: CustomLocale = serde_json::from_str(&content)?;
        Ok(Self::Custom(Box::new(custom)))
    }

    pub fn export_template(path: &Path) -> Result<()> {
        let template = CustomLocale {
            code: "custom".to_string(),
            display_name: "My Custom Language".to_string(),
            chat_title: " 💬 Conversation ".to_string(),
            welcome_msg: "  ⚡ Welcome to OpenCode-RS!".to_string(),
            welcome_hint: "  Type a prompt, mention @file, @git, or press [Ctrl+P] for commands.".to_string(),
            input_placeholder: "Ask AI, mention @file, /help, [Ctrl+P] commands...".to_string(),
            sidebar_title: " ℹ️ Project Info [Ctrl+B] ".to_string(),
            status_ready: "○ READY".to_string(),
            status_generating: "● GENERATING...".to_string(),
        };

        let json = serde_json::to_string_pretty(&template)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        fs::write(path, json)?;
        Ok(())
    }

    pub fn code(&self) -> &str {
        match self {
            Self::English => "en",
            Self::Polish => "pl",
            Self::Chinese => "zh",
            Self::German => "de",
            Self::Spanish => "es",
            Self::French => "fr",
            Self::Ukrainian => "uk",
            Self::Custom(c) => &c.code,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Self::English => "English (Default)",
            Self::Polish => "Polski",
            Self::Chinese => "简体中文 (Simplified Chinese)",
            Self::German => "Deutsch",
            Self::Spanish => "Español",
            Self::French => "Français",
            Self::Ukrainian => "Українська",
            Self::Custom(c) => &c.display_name,
        }
    }

    pub fn list_all() -> Vec<(&'static str, &'static str)> {
        vec![
            ("en", "English (Default)"),
            ("pl", "Polski"),
            ("zh", "简体中文 (Chinese Simplified)"),
            ("de", "Deutsch"),
            ("es", "Español"),
            ("fr", "Français"),
            ("uk", "Українська"),
        ]
    }

    // ── Teksty UI ──

    pub fn chat_title(&self) -> &str {
        match self {
            Self::English => " 💬 Conversation ",
            Self::Polish => " 💬 Rozmowa ",
            Self::Chinese => " 💬 对话 ",
            Self::German => " 💬 Konversation ",
            Self::Spanish => " 💬 Conversación ",
            Self::French => " 💬 Conversation ",
            Self::Ukrainian => " 💬 Розмова ",
            Self::Custom(c) => &c.chat_title,
        }
    }

    pub fn welcome_msg(&self) -> &str {
        match self {
            Self::English => "  ⚡ Welcome to OpenCode-RS!",
            Self::Polish => "  ⚡ Witaj w OpenCode-RS!",
            Self::Chinese => "  ⚡ 欢迎使用 OpenCode-RS!",
            Self::German => "  ⚡ Willkommen bei OpenCode-RS!",
            Self::Spanish => "  ⚡ ¡Bienvenido a OpenCode-RS!",
            Self::French => "  ⚡ Bienvenue dans OpenCode-RS!",
            Self::Ukrainian => "  ⚡ Ласкаво просимо до OpenCode-RS!",
            Self::Custom(c) => &c.welcome_msg,
        }
    }

    pub fn welcome_hint(&self) -> &str {
        match self {
            Self::English => "  Type a prompt, mention @file, @git, or press [Ctrl+P] for commands.",
            Self::Polish => "  Wpisz zapytanie, dołącz @plik, @git lub naciśnij [Ctrl+P] dla komend.",
            Self::Chinese => "  输入提示词，使用 @file、@git 附加文件，或按 [Ctrl+P] 查看命令列表。",
            Self::German => "  Geben Sie einen Prompt ein, @file, @git oder drücken Sie [Ctrl+P].",
            Self::Spanish => "  Escribe un prompt, menciona @file, @git o presiona [Ctrl+P].",
            Self::French => "  Tapez une invite, mentionnez @file, @git ou appuyez sur [Ctrl+P].",
            Self::Ukrainian => "  Введіть запит, додайте @file, @git або натисніć [Ctrl+P].",
            Self::Custom(c) => &c.welcome_hint,
        }
    }

    pub fn input_placeholder(&self) -> &str {
        match self {
            Self::English => "Ask AI, mention @file, /help, [Ctrl+P] commands...",
            Self::Polish => "Wpisz zapytanie, @plik, /help, [Ctrl+P] komendy...",
            Self::Chinese => "向 AI 提问，输入 @文件，/help，[Ctrl+P] 命令...",
            Self::German => "Fragen Sie die KI, @Datei, /help, [Ctrl+P] Befehle...",
            Self::Spanish => "Pregunta a la IA, @archivo, /help, [Ctrl+P] comandos...",
            Self::French => "Demandez à l'IA, @fichier, /help, [Ctrl+P] commandes...",
            Self::Ukrainian => "Запитайте ШІ, @файл, /help, [Ctrl+P] команди...",
            Self::Custom(c) => &c.input_placeholder,
        }
    }

    pub fn sidebar_title(&self) -> &str {
        match self {
            Self::English => " ℹ️ Project Info [Ctrl+B] ",
            Self::Polish => " ℹ️ Panel Informacyjny [Ctrl+B] ",
            Self::Chinese => " ℹ️ 项目信息 [Ctrl+B] ",
            Self::German => " ℹ️ Projekt-Info [Ctrl+B] ",
            Self::Spanish => " ℹ️ Información [Ctrl+B] ",
            Self::French => " ℹ️ Infos Projet [Ctrl+B] ",
            Self::Ukrainian => " ℹ️ Інформація [Ctrl+B] ",
            Self::Custom(c) => &c.sidebar_title,
        }
    }

    pub fn status_ready(&self) -> &str {
        match self {
            Self::English => "○ READY",
            Self::Polish => "○ GOTOWY",
            Self::Chinese => "○ 就绪",
            Self::German => "○ BEREIT",
            Self::Spanish => "○ LISTO",
            Self::French => "○ PRÊT",
            Self::Ukrainian => "○ ГОТОВИЙ",
            Self::Custom(c) => &c.status_ready,
        }
    }

    pub fn status_generating(&self) -> &str {
        match self {
            Self::English => "● GENERATING...",
            Self::Polish => "● GENEROWANIE...",
            Self::Chinese => "● 正在生成...",
            Self::German => "● GENERIERT...",
            Self::Spanish => "● GENERANDO...",
            Self::French => "● GÉNÉRATION...",
            Self::Ukrainian => "● ГЕНЕРАЦІЯ...",
            Self::Custom(c) => &c.status_generating,
        }
    }
}
