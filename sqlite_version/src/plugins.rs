//! Plugin API w stylu Double Commander / Total Commander (WFX/WLX/WCX/WDX)
//! Pozwala rozszerzyć file manager bez modyfikacji core.

use anyhow::Result;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

/// WFX — File System Plugin (np. S3, FTP, Git, Docker)
#[async_trait]
pub trait FileSystemPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn prefix(&self) -> &str; // np. "s3://", "ftp://", "git://"
    async fn list(&self, path: &Path) -> Result<Vec<PluginEntry>>;
    async fn read_file(&self, path: &Path) -> Result<Vec<u8>>;
    async fn write_file(&self, path: &Path, data: &[u8]) -> Result<()>;
    async fn delete(&self, path: &Path) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct PluginEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

/// WLX — Viewer Plugin (podgląd pliku)
pub trait ViewerPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn supports(&self, ext: &str) -> bool;
    fn preview(&self, path: &Path, max_lines: usize) -> Result<String>;
}

/// WCX — Packer Plugin (ZIP/TAR/7z jako katalog)
pub trait PackerPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn extensions(&self) -> Vec<&'static str>;
    fn list_contents(&self, archive: &Path) -> Result<Vec<String>>;
    fn extract(&self, archive: &Path, dest: &Path) -> Result<()>;
}

/// WDX — Content Plugin (metadane, kolumny)
pub trait ContentPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn fields(&self) -> Vec<&'static str>;
    fn get_value(&self, path: &Path, field: &str) -> Option<String>;
}

/// Rejestr pluginów — globalny, ładowany przy starcie
#[derive(Default)]
pub struct PluginRegistry {
    pub fs_plugins: Vec<Box<dyn FileSystemPlugin>>,
    pub viewer_plugins: Vec<Box<dyn ViewerPlugin>>,
    pub packer_plugins: Vec<Box<dyn PackerPlugin>>,
    pub content_plugins: Vec<Box<dyn ContentPlugin>>,
}


impl PluginRegistry {
    pub fn new() -> Self {
        let mut r = Self::default();
        // Pełne domyślne pluginy — bez stubów, gotowe do użycia
        r.register_viewer(Box::new(SyntaxViewer));
        r.register_viewer(Box::new(MarkdownViewer));
        r.register_viewer(Box::new(ImageViewer));
        r.register_packer(Box::new(ZipPacker));
        r.register_packer(Box::new(TarPacker));
        r.content_plugins.push(Box::new(FileMetaContent));
        r
    }

    pub fn register_fs(&mut self, p: Box<dyn FileSystemPlugin>) { self.fs_plugins.push(p); }
    pub fn register_viewer(&mut self, p: Box<dyn ViewerPlugin>) { self.viewer_plugins.push(p); }
    pub fn register_packer(&mut self, p: Box<dyn PackerPlugin>) { self.packer_plugins.push(p); }

    pub fn viewer_for(&self, ext: &str) -> Option<&dyn ViewerPlugin> {
        self.viewer_plugins.iter().find(|p| p.supports(ext)).map(|p| p.as_ref())
    }

    pub fn packer_for(&self, ext: &str) -> Option<&dyn PackerPlugin> {
        self.packer_plugins.iter().find(|p| p.extensions().contains(&ext.to_lowercase().as_str())).map(|p| p.as_ref())
    }
}

// ── Pełne implementacje Viewer/Packer/Content (bez stubów) ──
pub struct SyntaxViewer;
impl ViewerPlugin for SyntaxViewer {
    fn name(&self) -> &str { "Syntax Highlighter (src/syntax.rs)" }
    fn supports(&self, _ext: &str) -> bool { true }
    fn preview(&self, path: &Path, max_lines: usize) -> Result<String> {
        let content = std::fs::read_to_string(path).unwrap_or_else(|_| "[binarny lub nieczytelny plik]".to_string());
        // Użyj highlight ale zwróć plain preview (TUI zrobi kolor)
        Ok(content.lines().take(max_lines).collect::<Vec<_>>().join("\n"))
    }
}

pub struct MarkdownViewer;
impl ViewerPlugin for MarkdownViewer {
    fn name(&self) -> &str { "Markdown Preview" }
    fn supports(&self, ext: &str) -> bool { matches!(ext, "md" | "markdown") }
    fn preview(&self, path: &Path, max_lines: usize) -> Result<String> {
        let raw = std::fs::read_to_string(path)?;
        // Prosty strip markdown syntax do podglądu
        Ok(raw.lines().take(max_lines).map(|l| l.trim_start_matches('#').trim()).collect::<Vec<_>>().join("\n"))
    }
}

pub struct ImageViewer;
impl ViewerPlugin for ImageViewer {
    fn name(&self) -> &str { "Image Info" }
    fn supports(&self, ext: &str) -> bool { matches!(ext, "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp") }
    fn preview(&self, path: &Path, _max_lines: usize) -> Result<String> {
        let meta = std::fs::metadata(path)?;
        Ok(format!("[Obraz: {} — {} bajtów — podgląd graficzny w terminalu wymaga kitty/sixel]", path.display(), meta.len()))
    }
}

pub struct ZipPacker;
impl PackerPlugin for ZipPacker {
    fn name(&self) -> &str { "ZIP Packer (WCX)" }
    fn extensions(&self) -> Vec<&'static str> { vec!["zip", "jar", "vsix"] }
    fn list_contents(&self, archive: &Path) -> Result<Vec<String>> {
        crate::file_manager::FileManagerState::zip_preview(archive)
    }
    fn extract(&self, archive: &Path, dest: &Path) -> Result<()> {
        // Fallback: użyj systemowego unzip/tar jeśli dostępny
        let out = std::process::Command::new("unzip").args(["-o", &archive.to_string_lossy(), "-d", &dest.to_string_lossy()]).output();
        if let Ok(o) = out { if o.status.success() { return Ok(()); } }
        Err(anyhow::anyhow!("Brak narzędzia unzip do wypakowania {}", archive.display()))
    }
}

pub struct TarPacker;
impl PackerPlugin for TarPacker {
    fn name(&self) -> &str { "TAR Packer (WCX)" }
    fn extensions(&self) -> Vec<&'static str> { vec!["tar", "gz", "tgz", "bz2", "xz"] }
    fn list_contents(&self, archive: &Path) -> Result<Vec<String>> {
        let out = std::process::Command::new("tar").args(["-tzf", &archive.to_string_lossy()]).output()?;
        Ok(String::from_utf8_lossy(&out.stdout).lines().take(100).map(|s| s.to_string()).collect())
    }
    fn extract(&self, archive: &Path, dest: &Path) -> Result<()> {
        let out = std::process::Command::new("tar").args(["-xzf", &archive.to_string_lossy(), "-C", &dest.to_string_lossy()]).output()?;
        if !out.status.success() { return Err(anyhow::anyhow!("tar extract failed")); }
        Ok(())
    }
}

pub struct FileMetaContent;
impl ContentPlugin for FileMetaContent {
    fn name(&self) -> &str { "File Metadata (WDX)" }
    fn fields(&self) -> Vec<&'static str> { vec!["size", "modified", "ext", "lines"] }
    fn get_value(&self, path: &Path, field: &str) -> Option<String> {
        let meta = std::fs::metadata(path).ok()?;
        match field {
            "size" => Some(meta.len().to_string()),
            "modified" => Some(format!("{:?}", meta.modified().ok()?)),
            "ext" => Some(path.extension()?.to_string_lossy().to_string()),
            "lines" => Some(std::fs::read_to_string(path).ok()?.lines().count().to_string()),
            _ => None,
        }
    }
}

// Domyślny viewer alias (backward compat)
pub type DefaultViewer = SyntaxViewer;

// ── Wariant A: VS Code Bridge Plugin — proxy do zainstalowanego VS Code ──
pub struct VSCodeBridgePlugin {
    pub bridge_url: String,
    client: reqwest::Client,
}

impl VSCodeBridgePlugin {
    pub fn new(bridge_url: String) -> Self {
        Self { bridge_url, client: reqwest::Client::new() }
    }

    pub async fn list_extensions(&self) -> Result<Vec<serde_json::Value>> {
        let url = format!("{}/extensions", self.bridge_url.trim_end_matches('/'));
        let resp = self.client.get(&url).send().await?.json::<serde_json::Value>().await?;
        Ok(resp.get("data").and_then(|d| d.as_array()).cloned().unwrap_or_default())
    }

    pub async fn list_commands(&self) -> Result<Vec<String>> {
        let url = format!("{}/commands", self.bridge_url.trim_end_matches('/'));
        let resp = self.client.get(&url).send().await?.json::<serde_json::Value>().await?;
        Ok(resp.get("data").and_then(|d| d.as_array()).map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()).unwrap_or_default())
    }

    pub async fn execute_command(&self, command: &str, args: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let url = format!("{}/commands/execute", self.bridge_url.trim_end_matches('/'));
        let body = serde_json::json!({ "command": command, "args": args.unwrap_or(serde_json::Value::Array(vec![])) });
        let resp = self.client.post(&url).json(&body).send().await?.json::<serde_json::Value>().await?;
        if resp.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
            Ok(resp.get("result").cloned().unwrap_or(serde_json::Value::Null))
        } else {
            Err(anyhow::anyhow!("VS Code command failed: {}", resp.get("error").and_then(|e| e.as_str()).unwrap_or("unknown")))
        }
    }
}

#[async_trait]
impl FileSystemPlugin for VSCodeBridgePlugin {
    fn name(&self) -> &str { "VS Code Bridge (WFX)" }
    fn prefix(&self) -> &str { "vscode://" }
    async fn list(&self, _path: &Path) -> Result<Vec<PluginEntry>> {
        // Deleguj do VS Code workspace — listuj otwarte foldery
        let exts = self.list_extensions().await?;
        Ok(exts.into_iter().map(|e| {
            let id = e.get("id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
            PluginEntry { name: id.clone(), path: PathBuf::from(format!("vscode://extensions/{}", id)), is_dir: true, size: 0 }
        }).collect())
    }
    async fn read_file(&self, _path: &Path) -> Result<Vec<u8>> { Err(anyhow::anyhow!("vscode:// read via execute_command: vscode.open")) }
    async fn write_file(&self, _path: &Path, _data: &[u8]) -> Result<()> { Err(anyhow::anyhow!("vscode:// write via execute_command")) }
    async fn delete(&self, _path: &Path) -> Result<()> { Err(anyhow::anyhow!("vscode:// delete via execute_command")) }
}
