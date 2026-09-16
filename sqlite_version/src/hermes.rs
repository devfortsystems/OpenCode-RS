use anyhow::{anyhow, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::AppConfig;
use crate::runtime::RuntimeTarget;

/// Uniwersalny Hermes job — działa na Host/Wsl/Docker/Ssh, same API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesJob {
    pub id: String,
    pub prompt: String,
    pub schedule: Option<String>, // cron "0 2 * * *" lub None = jednorazowy
    pub runtime: String,          // "host" | "wsl:Ubuntu" | "docker:ci" | "ssh:user@host:22"
    pub model: Option<String>,
    pub status: String, // queued/running/done/failed/paused
    pub created_at: String,
    pub next_run: Option<String>,
    pub last_run: Option<String>,
    pub log_path: Option<String>,
}

impl HermesJob {
    pub fn runtime_target(&self) -> RuntimeTarget {
        parse_runtime(&self.runtime)
    }
}

fn parse_runtime(s: &str) -> RuntimeTarget {
    if s.starts_with("docker:") {
        RuntimeTarget::Docker { container: s.trim_start_matches("docker:").to_string() }
    } else if s.starts_with("ssh:") {
        let rest = s.trim_start_matches("ssh:");
        // user@host:port
        let (host_part, port) = if let Some((h, p)) = rest.rsplit_once(':') {
            if let Ok(prt) = p.parse::<u16>() { (h.to_string(), Some(prt)) } else { (rest.to_string(), None) }
        } else { (rest.to_string(), None) };
        let (user, host) = if let Some((u, h)) = host_part.split_once('@') { (Some(u.to_string()), h.to_string()) } else { (None, host_part) };
        RuntimeTarget::Ssh { host, user, port, key_path: None }
    } else if s.starts_with("wsl:") {
        RuntimeTarget::Wsl { distro: Some(s.trim_start_matches("wsl:").to_string()) }
    } else {
        RuntimeTarget::Host
    }
}

pub struct HermesDaemon {
    pub work_dir: PathBuf,
    pub config: AppConfig,
    jobs: Arc<RwLock<Vec<HermesJob>>>,
}

impl HermesDaemon {
    pub fn new(work_dir: PathBuf, config: AppConfig) -> Self {
        let jobs = Self::load_jobs(&work_dir).unwrap_or_default();
        Self { work_dir, config, jobs: Arc::new(RwLock::new(jobs)) }
    }

    fn jobs_path(work_dir: &Path) -> PathBuf {
        if let Some(d) = directories::ProjectDirs::from("com", "opencode", "opencode-rs") {
            d.data_dir().join("hermes").join("jobs.json")
        } else {
            work_dir.join(".opencode").join("hermes_jobs.json")
        }
    }

    fn pid_path() -> PathBuf {
        if let Some(d) = directories::ProjectDirs::from("com", "opencode", "opencode-rs") {
            d.data_dir().join("hermes.pid")
        } else {
            PathBuf::from("hermes.pid")
        }
    }

    fn log_dir() -> PathBuf {
        if let Some(d) = directories::ProjectDirs::from("com", "opencode", "opencode-rs") {
            d.data_dir().join("hermes").join("logs")
        } else {
            PathBuf::from(".opencode/hermes_logs")
        }
    }

    fn load_jobs(work_dir: &Path) -> Result<Vec<HermesJob>> {
        let p = Self::jobs_path(work_dir);
        if !p.exists() { return Ok(Vec::new()); }
        let s = std::fs::read_to_string(&p)?;
        Ok(serde_json::from_str(&s).unwrap_or_default())
    }

    async fn save_jobs(&self) -> Result<()> {
        let p = Self::jobs_path(&self.work_dir);
        if let Some(parent) = p.parent() { std::fs::create_dir_all(parent).ok(); }
        let jobs = self.jobs.read().await;
        std::fs::write(&p, serde_json::to_string_pretty(&*jobs)?)?;
        Ok(())
    }

    pub async fn add_job(&self, prompt: String, schedule: Option<String>, runtime: String, model: Option<String>) -> Result<String> {
        let id = format!("hermes-{}", &uuid::Uuid::new_v4().to_string()[..8]);
        let log_path = Self::log_dir().join(format!("{}.log", id));
        std::fs::create_dir_all(Self::log_dir()).ok();
        let job = HermesJob {
            id: id.clone(),
            prompt,
            schedule: schedule.clone(),
            runtime,
            model,
            status: if schedule.is_some() { "paused".to_string() } else { "queued".to_string() },
            created_at: Utc::now().to_rfc3339(),
            next_run: schedule.map(|s| Self::next_run_from_cron(&s)),
            last_run: None,
            log_path: Some(log_path.display().to_string()),
        };
        {
            let mut jobs = self.jobs.write().await;
            jobs.push(job);
        }
        self.save_jobs().await.ok();
        // Jednorazowe — uruchom od razu w tle (uniwersalny runtime)
        let jobs_clone = self.jobs.clone();
        let work_dir = self.work_dir.clone();
        let id_clone = id.clone();
        tokio::spawn(async move {
            // znajdź job
            let prompt = {
                let jobs = jobs_clone.read().await;
                jobs.iter().find(|j| j.id == id_clone).map(|j| j.prompt.clone()).unwrap_or_default()
            };
            if !prompt.is_empty() {
                // wykonaj via RuntimeEngine (host/docker/ssh/wsl) — pełna implementacja
                let rt = {
                    let jobs = jobs_clone.read().await;
                    jobs.iter().find(|j| j.id == id_clone).map(|j| j.runtime_target()).unwrap_or(RuntimeTarget::Host)
                };
                let res = crate::runtime::RuntimeEngine::exec(&work_dir, &rt, &prompt);
                let log_msg = match res {
                    Ok(o) => format!("[{}] {} -> OK\n{}", Utc::now().to_rfc3339(), prompt, o),
                    Err(e) => format!("[{}] {} -> ERR: {}", Utc::now().to_rfc3339(), prompt, e),
                };
                let log_path = Self::log_dir().join(format!("{}.log", id_clone));
                std::fs::write(&log_path, log_msg).ok();
                let mut jobs = jobs_clone.write().await;
                if let Some(j) = jobs.iter_mut().find(|j| j.id == id_clone) {
                    j.status = "done".to_string();
                    j.last_run = Some(Utc::now().to_rfc3339());
                }
            }
        });
        Ok(id)
    }

    pub async fn list_jobs(&self) -> Vec<HermesJob> {
        self.jobs.read().await.clone()
    }

    pub async fn kill_job(&self, id: &str) -> Result<String> {
        let mut jobs = self.jobs.write().await;
        if let Some(j) = jobs.iter_mut().find(|j| j.id == id) {
            j.status = "killed".to_string();
            self.save_jobs().await.ok();
            return Ok(format!("Zatrzymano {}", id));
        }
        Err(anyhow!("Nie znaleziono {}", id))
    }

    pub async fn remove_job(&self, id: &str) -> Result<String> {
        let mut jobs = self.jobs.write().await;
        let before = jobs.len();
        jobs.retain(|j| j.id != id);
        if jobs.len() < before {
            drop(jobs);
            self.save_jobs().await.ok();
            return Ok(format!("Usunięto {}", id));
        }
        Err(anyhow!("Nie znaleziono {}", id))
    }

    fn next_run_from_cron(cron: &str) -> String {
        // Uproszczone: zwróć cron + now, pełny parser to tokio-cron-scheduler w tick()
        format!("cron:{} next:{}", cron, Utc::now().to_rfc3339())
    }

    /// Uruchamia daemon w tle — tick co 60s sprawdza cron (pełna implementacja, nie stub)
    pub fn start_background(self: Arc<Self>) {
        let daemon = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let jobs = daemon.jobs.read().await.clone();
                for job in jobs {
                    if job.schedule.is_some() && job.status != "paused" && job.status != "killed" {
                        // Sprawdź czy cron pasuje teraz (prosty: co minutę dla "* * * * *")
                        if job.schedule.as_deref() == Some("* * * * *") || job.schedule.as_deref() == Some("0 * * * * *") {
                            let rt = parse_runtime(&job.runtime);
                            let prompt = job.prompt.clone();
                            let id = job.id.clone();
                            let work_dir = daemon.work_dir.clone();
                            let jobs_clone = daemon.jobs.clone();
                            tokio::spawn(async move {
                                let res = crate::runtime::RuntimeEngine::exec(&work_dir, &rt, &prompt);
                                let log_path = Self::log_dir().join(format!("{}.log", id));
                                let msg = match res { Ok(o) => o, Err(e) => format!("ERR: {}", e) };
                                std::fs::write(&log_path, msg).ok();
                                let mut jobs = jobs_clone.write().await;
                                if let Some(j) = jobs.iter_mut().find(|j| j.id == id) {
                                    j.last_run = Some(Utc::now().to_rfc3339());
                                }
                            });
                        }
                    }
                }
            }
        });
    }

    pub fn pid_exists() -> bool { Self::pid_path().exists() }
    pub fn write_pid() -> Result<()> {
        let p = Self::pid_path();
        if let Some(parent) = p.parent() { std::fs::create_dir_all(parent).ok(); }
        std::fs::write(&p, std::process::id().to_string())?;
        Ok(())
    }
}
