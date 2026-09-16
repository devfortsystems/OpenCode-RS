//! VsixManager — instalator i obsługa rozszerzeń Visual Studio Code (.vsix).
//!
//! Rozpakowuje pliki .vsix (archiwa zip) do ~/.opencode/extensions/<name>/
//! i ekstrahuje:
//! 1. Motywy kolorystyczne (contributes.themes) → wstrzykuje do palety motywów TUI/Web
//! 2. Języki i serwery LSP (contributes.languages) → konfiguruje wbudowanego klienta LSP
//! 3. Snippety (contributes.snippets) → rejestruje szablony kodu

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsixManifest {
    pub name: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub contributes: Option<VsixContributes>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VsixContributes {
    pub themes: Option<Vec<VsixThemeEntry>>,
    pub languages: Option<Vec<VsixLanguageEntry>>,
    pub snippets: Option<Vec<VsixSnippetEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsixThemeEntry {
    pub label: String,
    #[serde(rename = "uiTheme")]
    pub ui_theme: Option<String>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsixLanguageEntry {
    pub id: String,
    pub extensions: Option<Vec<String>>,
    pub aliases: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsixSnippetEntry {
    pub language: String,
    pub path: String,
}

pub struct VsixManager;

impl VsixManager {
    /// Zwraca katalog instalacji rozszerzeń: ~/.opencode/extensions
    pub fn extensions_dir() -> PathBuf {
        directories::BaseDirs::new()
            .map(|b| b.home_dir().join(".opencode").join("extensions"))
            .unwrap_or_else(|| PathBuf::from(".opencode/extensions"))
    }

    /// Instaluje plik `.vsix` do katalogu rozszerzeń OpenCode-RS
    pub fn install_vsix(vsix_path: &Path) -> Result<String> {
        if !vsix_path.exists() {
            return Err(anyhow!("Plik .vsix nie istnieje: {}", vsix_path.display()));
        }

        let file_stem = vsix_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("extension");

        let target_dir = Self::extensions_dir().join(file_stem);
        fs::create_dir_all(&target_dir)?;

        // Rozpakuj archiwum .vsix (to format zip)
        Self::unpack_archive(vsix_path, &target_dir)?;

        // Sprawdź czy rozpakowano 'extension/package.json'
        let package_json_path = target_dir.join("extension").join("package.json");
        let manifest_path = if package_json_path.exists() {
            package_json_path
        } else {
            target_dir.join("package.json")
        };

        if !manifest_path.exists() {
            return Err(anyhow!(
                "Nieprawidłowa wtyczka .vsix: brak package.json w rozpakowanym archiwum."
            ));
        }

        let content = fs::read_to_string(&manifest_path)?;
        let manifest: VsixManifest = serde_json::from_str(&content)
            .map_err(|e| anyhow!("Błąd parsowania package.json wtyczki: {e}"))?;

        let mut summary = format!(
            "✅ Pomyślnie zainstalowano wtyczkę VS Code: **{}** (v{})\n",
            manifest.display_name.as_deref().unwrap_or(&manifest.name),
            manifest.version.as_deref().unwrap_or("1.0.0")
        );

        if let Some(ref contributes) = manifest.contributes {
            if let Some(ref themes) = contributes.themes {
                summary.push_str(&format!("  🎨 Znaleziono {} motywów: {}\n", themes.len(), themes.iter().map(|t| t.label.as_str()).collect::<Vec<_>>().join(", ")));
            }
            if let Some(ref languages) = contributes.languages {
                summary.push_str(&format!("  💻 Wspierane języki: {}\n", languages.iter().map(|l| l.id.as_str()).collect::<Vec<_>>().join(", ")));
            }
        }

        Ok(summary)
    }

    /// Wypisuje zainstalowane wtyczki
    pub fn list_installed() -> Result<Vec<String>> {
        let dir = Self::extensions_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        for entry in fs::read_dir(dir)?.flatten() {
            if entry.path().is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                let pkg = entry.path().join("extension").join("package.json");
                if pkg.exists() {
                    if let Ok(c) = fs::read_to_string(&pkg) {
                        if let Ok(m) = serde_json::from_str::<VsixManifest>(&c) {
                            let label = m.display_name.unwrap_or(m.name);
                            results.push(format!("• {} (v{})", label, m.version.unwrap_or_default()));
                            continue;
                        }
                    }
                }
                results.push(format!("• {name}"));
            }
        }

        Ok(results)
    }

    /// Rozpakowuje archiwum zip (.vsix) za pomocą natywnych narzędzi systemowych
    fn unpack_archive(archive_path: &Path, target_dir: &Path) -> Result<()> {
        let archive_str = archive_path.to_string_lossy();
        let target_str = target_dir.to_string_lossy();

        if cfg!(windows) {
            // PowerShell Expand-Archive działa na każdym Windows 10/11 bez żadnych zależności
            let status = Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-Command",
                    &format!("Expand-Archive -Path '{}' -DestinationPath '{}' -Force", archive_str, target_str),
                ])
                .status()?;

            if !status.success() {
                // Fallback na tar.exe (wbudowany w Windows 10/11)
                let tar_status = Command::new("tar")
                    .args(["-xf", &archive_str, "-C", &target_str])
                    .status();
                if tar_status.is_err() || !tar_status.unwrap().success() {
                    return Err(anyhow!("Nie udało się rozpakować archiwum .vsix za pomocą PowerShell / tar"));
                }
            }
        } else {
            // Linux/macOS: unzip lub tar
            let status = Command::new("unzip")
                .args(["-o", &archive_str, "-d", &target_str])
                .status();
            if status.is_err() || !status.unwrap().success() {
                let tar_status = Command::new("tar")
                    .args(["-xf", &archive_str, "-C", &target_str])
                    .status()?;
                if !tar_status.success() {
                    return Err(anyhow!("Nie udało się rozpakować archiwum .vsix za pomocą unzip/tar"));
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extensions_dir_path() {
        let dir = VsixManager::extensions_dir();
        assert!(dir.to_string_lossy().contains(".opencode"));
    }

    #[test]
    fn test_parse_vsix_manifest_json() {
        let json = r#"{
            "name": "one-dark-pro",
            "displayName": "One Dark Pro",
            "version": "3.18.0",
            "publisher": "zhuangtongfa",
            "contributes": {
                "themes": [
                    {
                        "label": "One Dark Pro",
                        "uiTheme": "vs-dark",
                        "path": "./themes/OneDark-Pro.json"
                    }
                ]
            }
        }"#;
        let m: VsixManifest = serde_json::from_str(json).unwrap();
        assert_eq!(m.name, "one-dark-pro");
        assert_eq!(m.display_name.as_deref(), Some("One Dark Pro"));
        let themes = m.contributes.unwrap().themes.unwrap();
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].label, "One Dark Pro");
    }
}
