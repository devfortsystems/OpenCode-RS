use anyhow::{anyhow, Result};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
#[derive(Default)]
pub enum RuntimeTarget {
    #[default]
    Host,
    Wsl { distro: Option<String> },
    Docker { container: String },
    Ssh {
        host: String,
        user: Option<String>,
        port: Option<u16>,
        key_path: Option<String>,
    },
}


pub struct RuntimeEngine;

impl RuntimeEngine {
    /// Dekoduje wyjście z procesów Windows / WSL, obsługując zarówno standardowe UTF-8, jak i Windowsowe UTF-16LE
    pub fn decode_output(bytes: &[u8]) -> String {
        if bytes.is_empty() {
            return String::new();
        }

        // Sprawdź czy to UTF-16LE (BOM 0xFF,0xFE lub co drugi bajt zerowy)
        if bytes.len() >= 2 && (bytes[1] == 0 || (bytes[0] == 0xFF && bytes[1] == 0xFE)) {
            let u16_slice: Vec<u16> = bytes
                .as_chunks::<2>().0.iter()
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&u16_slice)
                .trim_start_matches('\u{feff}')
                .replace('\r', "")
                .trim()
                .to_string()
        } else {
            String::from_utf8_lossy(bytes)
                .replace('\r', "")
                .trim()
                .to_string()
        }
    }

    /// Wykonuje polecenie powłoki w wybranym środowisku (Host / WSL / Docker / SSH Linux)
    pub fn exec(work_dir: &Path, runtime: &RuntimeTarget, command: &str) -> Result<String> {
        let out = match runtime {
            RuntimeTarget::Host => {
                if cfg!(target_os = "windows") {
                    Command::new("powershell")
                        .args(["-NoProfile", "-Command", command])
                        .current_dir(work_dir)
                        .output()?
                } else {
                    Command::new("sh")
                        .args(["-c", command])
                        .current_dir(work_dir)
                        .output()?
                }
            }
            RuntimeTarget::Wsl { distro } => {
                let mut cmd = Command::new("wsl");
                if let Some(d) = distro {
                    if !d.is_empty() {
                        cmd.args(["-d", d]);
                    }
                }
                if let Some(dir_str) = work_dir.to_str() {
                    cmd.args(["--cd", dir_str]);
                }
                cmd.args(["--", "bash", "-c", command]);
                cmd.output()?
            }
            RuntimeTarget::Docker { container } => {
                Command::new("docker")
                    .args(["exec", "-i", container, "sh", "-c", command])
                    .current_dir(work_dir)
                    .output()?
            }
            RuntimeTarget::Ssh { host, user, port, key_path } => {
                let mut cmd = Command::new("ssh");
                if let Some(p) = port {
                    cmd.args(["-p", &p.to_string()]);
                }
                if let Some(k) = key_path {
                    cmd.args(["-i", k]);
                }

                let target = if let Some(u) = user {
                    format!("{u}@{host}")
                } else {
                    host.clone()
                };

                cmd.arg(target);
                cmd.arg(command);
                cmd.current_dir(work_dir);
                cmd.output()?
            }
        };

        let stdout = Self::decode_output(&out.stdout);
        let stderr = Self::decode_output(&out.stderr);

        if !out.status.success() {
            if !stderr.is_empty() {
                return Err(anyhow!("Błąd wykonania (kod: {}):\n{}", out.status.code().unwrap_or(-1), stderr));
            } else {
                return Err(anyhow!("Błąd wykonania (kod: {}):\n{}", out.status.code().unwrap_or(-1), stdout));
            }
        }

        Ok(stdout)
    }

    /// Pobiera listę aktywnych kontenerów Docker
    pub fn list_docker_containers() -> Result<String> {
        let out = Command::new("docker")
            .args(["ps", "--format", "table {{.ID}}\t{{.Names}}\t{{.Image}}\t{{.Status}}"])
            .output()?;

        if !out.status.success() {
            let err = Self::decode_output(&out.stderr);
            return Err(anyhow!("Błąd Docker: {err}"));
        }

        Ok(Self::decode_output(&out.stdout))
    }

    /// Pobiera listę zainstalowanych dystrybucji WSL (dekoduje UTF-16LE z Windows)
    pub fn list_wsl_distros() -> Result<String> {
        let out = Command::new("wsl").args(["-l", "-v"]).output()?;
        if !out.status.success() {
            let err = Self::decode_output(&out.stderr);
            return Err(anyhow!("Błąd WSL: {err}"));
        }
        let list = Self::decode_output(&out.stdout);
        if list.is_empty() {
            Ok("Brak zainstalowanych dystrybucji WSL. Zainstaluj poleceniem: wsl --install -d Ubuntu".to_string())
        } else {
            Ok(list)
        }
    }
}
