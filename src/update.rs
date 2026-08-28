use anyhow::Result;
use serde::Deserialize;

const REPO: &str = "anomalyco/opencode"; // zmień na właściwy repo po publikacji
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
}

pub struct Updater;

impl Updater {
    pub fn current_version() -> &'static str { CURRENT_VERSION }

    /// Sprawdza GitHub Releases — zwraca Some(nowsza_wersja) jeśli dostępna
    pub async fn check_for_update() -> Result<Option<(String, String)>> {
        let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
        let client = reqwest::Client::builder().user_agent("opencode-rs-updater").build()?;
        let resp = client.get(&url).header("Accept", "application/vnd.github+json").send().await?;
        if !resp.status().is_success() { return Ok(None); }
        let rel: GhRelease = resp.json().await?;
        let latest = rel.tag_name.trim_start_matches('v').to_string();
        let current = CURRENT_VERSION.trim_start_matches('v');
        if latest != current && is_newer(&latest, current) {
            Ok(Some((rel.tag_name, rel.html_url)))
        } else {
            Ok(None)
        }
    }

    /// Pobiera i podmienia binarkę (Windows: .exe, Unix: binary) — wywoływane przez /update
    pub async fn perform_update(download_url: &str, dest: &std::path::Path) -> Result<String> {
        let client = reqwest::Client::builder().user_agent("opencode-rs-updater").build()?;
        let bytes = client.get(download_url).send().await?.bytes().await?;
        std::fs::write(dest, &bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perm = std::fs::metadata(dest)?.permissions();
            perm.set_mode(0o755);
            std::fs::set_permissions(dest, perm).ok();
        }
        Ok(format!("Pobrano aktualizację do {}", dest.display()))
    }

    pub fn should_check_today() -> bool {
        let stamp_path = Self::stamp_path();
        if let Ok(s) = std::fs::read_to_string(&stamp_path) {
            if let Ok(t) = s.trim().parse::<i64>() {
                let now = chrono::Utc::now().timestamp();
                return now - t > 24 * 3600;
            }
        }
        true
    }

    pub fn mark_checked() {
        let p = Self::stamp_path();
        if let Some(parent) = p.parent() { std::fs::create_dir_all(parent).ok(); }
        std::fs::write(p, chrono::Utc::now().timestamp().to_string()).ok();
    }

    fn stamp_path() -> std::path::PathBuf {
        if let Some(d) = directories::ProjectDirs::from("com", "opencode", "opencode-rs") {
            d.cache_dir().join("last_update_check")
        } else {
            std::path::PathBuf::from(".opencode_last_update")
        }
    }
}

fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| s.split('.').filter_map(|p| p.parse::<u64>().ok()).collect::<Vec<_>>();
    parse(latest) > parse(current)
}
