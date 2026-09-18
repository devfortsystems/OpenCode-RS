//! Transfer plików — SCP, SSH pipe upload/download, dropzone watcher.
//! "Ultimate CLI" — pełny transfer plików przez SSH bez dodatkowych deps.

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::runtime::RuntimeTarget;

/// Konfiguracja połączenia SCP/SSH wyciągnięta z RuntimeTarget::Ssh.
#[derive(Debug, Clone)]
pub struct SshTarget {
    pub host: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub key_path: Option<String>,
}

impl SshTarget {
    /// Wyciąga SshTarget z RuntimeTarget jeśli to SSH.
    pub fn from_runtime(rt: &RuntimeTarget) -> Option<Self> {
        match rt {
            RuntimeTarget::Ssh { host, user, port, key_path } => Some(Self {
                host: host.clone(),
                user: user.clone(),
                port: *port,
                key_path: key_path.clone(),
            }),
            _ => None,
        }
    }

    /// Zwraca `user@host` lub `host`.
    pub fn target_str(&self) -> String {
        if let Some(u) = &self.user {
            format!("{u}@{}", self.host)
        } else {
            self.host.clone()
        }
    }

    /// Dodaje flagi SSH (-p, -i) do istniejącego Command.
    pub fn add_ssh_args(&self, cmd: &mut Command) {
        if let Some(p) = self.port {
            cmd.args(["-p", &p.to_string()]);
        }
        if let Some(k) = &self.key_path {
            cmd.args(["-i", k]);
        }
        cmd.args(["-o", "StrictHostKeyChecking=accept-new"]);
        cmd.args(["-o", "ConnectTimeout=10"]);
    }
}

/// Upload pliku lokalnego → zdalny przez `scp`.
pub fn scp_upload(local: &Path, remote_path: &str, target: &SshTarget) -> Result<String> {
    if !local.exists() {
        return Err(anyhow!("Plik lokalny nie istnieje: {}", local.display()));
    }
    let mut cmd = Command::new("scp");
    target.add_ssh_args(&mut cmd);
    cmd.arg(local);
    cmd.arg(format!("{}:{}", target.target_str(), remote_path));
    let out = cmd.output()?;
    interpret_scp_output(&out, "upload", local.to_string_lossy().as_ref(), remote_path)
}

/// Download pliku zdalnego → lokalny przez `scp`.
pub fn scp_download(remote_path: &str, local: &Path, target: &SshTarget) -> Result<String> {
    if let Some(parent) = local.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut cmd = Command::new("scp");
    target.add_ssh_args(&mut cmd);
    cmd.arg(format!("{}:{}", target.target_str(), remote_path));
    cmd.arg(local);
    let out = cmd.output()?;
    interpret_scp_output(&out, "download", remote_path, local.to_string_lossy().as_ref())
}

/// Upload przez SSH + base64 pipe — fallback gdy `scp` niedostępny.
/// Działa na każdym systemie z `ssh` + `base64`/`base` na zdalnym.
pub fn ssh_pipe_upload(local: &Path, remote_path: &str, target: &SshTarget) -> Result<String> {
    if !local.exists() {
        return Err(anyhow!("Plik lokalny nie istnieje: {}", local.display()));
    }
    let data = std::fs::read(local)?;
    let b64 = base64_encode(&data);
    // ssh host "echo <b64> | base64 -d > /remote/path"
    let remote_cmd = if cfg!(target_os = "windows") {
        // Na Windows zdalny to zazwyczaj Linux — base64 -d
        format!("echo '{b64}' | base64 -d > '{remote_path}'")
    } else {
        format!("echo '{b64}' | base64 -d > '{remote_path}'")
    };
    let mut cmd = Command::new("ssh");
    target.add_ssh_args(&mut cmd);
    cmd.arg(target.target_str());
    cmd.arg(&remote_cmd);
    let out = cmd.output()?;
    interpret_scp_output(&out, "upload (pipe)", local.to_string_lossy().as_ref(), remote_path)
}

/// Download przez SSH + base64 pipe — fallback gdy `scp` niedostępny.
pub fn ssh_pipe_download(remote_path: &str, local: &Path, target: &SshTarget) -> Result<String> {
    let remote_cmd = format!("base64 '{remote_path}'");
    let mut cmd = Command::new("ssh");
    target.add_ssh_args(&mut cmd);
    cmd.arg(target.target_str());
    cmd.arg(&remote_cmd);
    let out = cmd.output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!("SSH download błąd: {err}"));
    }
    let b64 = String::from_utf8_lossy(&out.stdout);
    let b64_clean: String = b64.chars().filter(|c| !c.is_whitespace()).collect();
    let data = base64_decode(&b64_clean)?;
    if let Some(parent) = local.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(local, &data)?;
    Ok(format!("✅ Pobrano (pipe): {remote_path} → {}", local.display()))
}

/// Upload całego katalogu przez `scp -r`.
pub fn scp_upload_dir(local_dir: &Path, remote_path: &str, target: &SshTarget) -> Result<String> {
    if !local_dir.is_dir() {
        return Err(anyhow!("Nie jest katalogiem: {}", local_dir.display()));
    }
    let mut cmd = Command::new("scp");
    cmd.arg("-r");
    target.add_ssh_args(&mut cmd);
    cmd.arg(local_dir);
    cmd.arg(format!("{}:{}", target.target_str(), remote_path));
    let out = cmd.output()?;
    interpret_scp_output(&out, "upload dir", local_dir.to_string_lossy().as_ref(), remote_path)
}

/// Download całego katalogu przez `scp -r`.
pub fn scp_download_dir(remote_path: &str, local_dir: &Path, target: &SshTarget) -> Result<String> {
    std::fs::create_dir_all(local_dir).ok();
    let mut cmd = Command::new("scp");
    cmd.arg("-r");
    target.add_ssh_args(&mut cmd);
    cmd.arg(format!("{}:{}", target.target_str(), remote_path));
    cmd.arg(local_dir);
    let out = cmd.output()?;
    interpret_scp_output(&out, "download dir", remote_path, local_dir.to_string_lossy().as_ref())
}

/// Lista plików zdalnych przez `ssh host ls -la path`.
pub fn ssh_ls(remote_path: &str, target: &SshTarget) -> Result<String> {
    let mut cmd = Command::new("ssh");
    target.add_ssh_args(&mut cmd);
    cmd.arg(target.target_str());
    cmd.arg(format!("ls -la '{remote_path}' 2>&1"));
    let out = cmd.output()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() && stdout.is_empty() {
        Ok(format!("❌ {stderr}"))
    } else {
        Ok(stdout.to_string())
    }
}

/// Sprawdza czy `scp` jest dostępny na PATH.
pub fn is_scp_available() -> bool {
    if cfg!(windows) {
        Command::new("where.exe")
            .arg("scp")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else {
        Command::new("which")
            .arg("scp")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Parsuje ścieżkę zdalną `user@host:/path` → (SshTarget, remote_path).
pub fn parse_remote_path(spec: &str) -> Result<(SshTarget, String)> {
    // Format: [user@]host:/remote/path
    let colon = spec.find(':').ok_or_else(|| anyhow!("Brak ':' w ścieżce zdalnej. Format: user@host:/path"))?;
    let host_part = &spec[..colon];
    let remote_path = &spec[colon + 1..];
    let (user, host) = if let Some(at) = host_part.find('@') {
        (Some(host_part[..at].to_string()), host_part[at + 1..].to_string())
    } else {
        (None, host_part.to_string())
    };
    if host.is_empty() {
        return Err(anyhow!("Pusty host w: {spec}"));
    }
    if remote_path.is_empty() {
        return Err(anyhow!("Pusta ścieżka zdalna w: {spec}"));
    }
    Ok((SshTarget { host, user, port: None, key_path: None }, remote_path.to_string()))
}

// ─── Dropzone watcher ───────────────────────────────────────────────

/// Zwraca ścieżkę dropzone: `~/.opencode-rs/dropzone/`.
pub fn dropzone_dir() -> PathBuf {
    if let Some(base) = directories::BaseDirs::new() {
        base.home_dir().join(".opencode-rs").join("dropzone")
    } else {
        PathBuf::from(".opencode-rs").join("dropzone")
    }
}

/// Inicjalizuje dropzone — tworzy katalog jeśli nie istnieje.
pub fn init_dropzone() -> Result<PathBuf> {
    let dir = dropzone_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// Skanuje dropzone i zwraca listę plików (z pełną ścieżką).
/// Po odczycie pliki są usuwane (konsumpcja jednorazowa).
pub fn consume_dropzone() -> Vec<PathBuf> {
    let dir = dropzone_dir();
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_file() {
                // Pomiń pliki tymczasowe (.tmp, .part, ~)
                if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    if !name.starts_with('.') && !name.ends_with(".tmp") && !name.ends_with(".part") {
                        files.push(p);
                    }
                }
            }
        }
    }
    files
}

/// Czyści dropzone po konsumpcji — usuwa przetworzone pliki.
pub fn cleanup_dropzone(files: &[PathBuf]) {
    for f in files {
        std::fs::remove_file(f).ok();
    }
}

// ─── Helpery ────────────────────────────────────────────────────────

fn interpret_scp_output(out: &std::process::Output, op: &str, src: &str, dst: &str) -> Result<String> {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if out.status.success() {
        Ok(format!("✅ SCP {op}: {src} → {dst}"))
    } else {
        let err = if stderr.is_empty() { &stdout } else { &stderr };
        Err(anyhow!("SCP {op} błąd (kod {}): {}", out.status.code().unwrap_or(-1), err.trim()))
    }
}

/// Prosty encoder base64 — bez zewnętrznych deps.
fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(TABLE[((n >> 6) & 63) as usize] as char);
        out.push(TABLE[(n & 63) as usize] as char);
        i += 3;
    }
    let rem = data.len() - i;
    if rem == 1 {
        let n = (data[i] as u32) << 16;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        out.push(TABLE[((n >> 6) & 63) as usize] as char);
        out.push('=');
    }
    out
}

/// Prosty decoder base64 — bez zewnętrznych deps.
fn base64_decode(input: &str) -> Result<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = input.bytes().filter(|b| *b != b'\n' && *b != b'\r' && *b != b' ').collect();
    if bytes.len() % 4 != 0 {
        return Err(anyhow!("Base64: nieprawidłowa długość"));
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut i = 0;
    while i < bytes.len() {
        let a = val(bytes[i]).ok_or_else(|| anyhow!("Base64: nieprawidłowy znak"))?;
        let b = val(bytes[i + 1]).ok_or_else(|| anyhow!("Base64: nieprawidłowy znak"))?;
        let c = if bytes[i + 2] == b'=' { 0 } else { val(bytes[i + 2]).ok_or_else(|| anyhow!("Base64: nieprawidłowy znak"))? };
        let d = if bytes[i + 3] == b'=' { 0 } else { val(bytes[i + 3]).ok_or_else(|| anyhow!("Base64: nieprawidłowy znak"))? };
        let n = ((a as u32) << 18) | ((b as u32) << 12) | ((c as u32) << 6) | (d as u32);
        out.push((n >> 16) as u8);
        if bytes[i + 2] != b'=' { out.push((n >> 8) as u8); }
        if bytes[i + 3] != b'=' { out.push(n as u8); }
        i += 4;
    }
    Ok(out)
}

/// Formatuje rozmiar w czytelny sposób (1.5 MB, 320 KB).
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB { format!("{:.1} GB", bytes as f64 / GB as f64) }
    else if bytes >= MB { format!("{:.1} MB", bytes as f64 / MB as f64) }
    else if bytes >= KB { format!("{:.0} KB", bytes as f64 / KB as f64) }
    else { format!("{bytes} B") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_roundtrip() {
        let data = b"Hello, World! OpenCode-RS transfer test 123";
        let encoded = base64_encode(data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_base64_empty() {
        let encoded = base64_encode(b"");
        assert_eq!(encoded, "");
        let decoded = base64_decode("").unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_base64_one_byte() {
        let encoded = base64_encode(b"A");
        assert_eq!(encoded, "QQ==");
        let decoded = base64_decode("QQ==").unwrap();
        assert_eq!(decoded, b"A");
    }

    #[test]
    fn test_base64_two_bytes() {
        let encoded = base64_encode(b"AB");
        assert_eq!(encoded, "QUI=");
        let decoded = base64_decode("QUI=").unwrap();
        assert_eq!(decoded, b"AB");
    }

    #[test]
    fn test_parse_remote_path_with_user() {
        let (target, path) = parse_remote_path("root@192.168.1.50:/etc/nginx/nginx.conf").unwrap();
        assert_eq!(target.user.as_deref(), Some("root"));
        assert_eq!(target.host, "192.168.1.50");
        assert_eq!(path, "/etc/nginx/nginx.conf");
    }

    #[test]
    fn test_parse_remote_path_no_user() {
        let (target, path) = parse_remote_path("10.0.0.1:/var/log/syslog").unwrap();
        assert!(target.user.is_none());
        assert_eq!(target.host, "10.0.0.1");
        assert_eq!(path, "/var/log/syslog");
    }

    #[test]
    fn test_parse_remote_path_invalid() {
        assert!(parse_remote_path("no-colon-here").is_err());
        assert!(parse_remote_path(":/empty-host").is_err());
        assert!(parse_remote_path("host:").is_err());
    }

    #[test]
    fn test_dropzone_dir_ends_with_dropzone() {
        let dir = dropzone_dir();
        assert!(dir.ends_with("dropzone"));
    }

    #[test]
    fn test_init_and_consume_dropzone() {
        let dir = init_dropzone().unwrap();
        // Wyczyść przed testem
        cleanup_dropzone(&consume_dropzone());
        // Utwórz plik testowy
        let test_file = dir.join("test_dropzone.txt");
        std::fs::write(&test_file, "test content").unwrap();
        // Skonsumuj
        let files = consume_dropzone();
        assert!(files.iter().any(|f| f == &test_file));
        // Wyczyść
        cleanup_dropzone(&files);
        assert!(!test_file.exists());
    }
}
