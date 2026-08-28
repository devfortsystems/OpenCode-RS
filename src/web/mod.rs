use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc::Sender;

use crate::app::AppEvent;
use crate::config::AppConfig;

pub struct WebCompanionServer {
    port: u16,
    work_dir: PathBuf,
    config: AppConfig,
}

impl WebCompanionServer {
    pub fn new(port: u16, work_dir: PathBuf, config: AppConfig) -> Self {
        Self {
            port,
            work_dir,
            config,
        }
    }

    /// Uruchamia asynchroniczny serwer Web Companion w tle
    pub fn start_background(self: Arc<Self>, _event_tx: Sender<AppEvent>) {
        let port = self.port;

        tokio::spawn(async move {
            let addr = format!("0.0.0.0:{}", port);
            let listener = match TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(_) => {
                    // Fallback na 127.0.0.1
                    match TcpListener::bind(format!("127.0.0.1:{}", port)).await {
                        Ok(l) => l,
                        Err(e) => {
                            eprintln!("⚠️ [Web Companion] Nie można uruchomić serwera na porcie {port}: {e}");
                            return;
                        }
                    }
                }
            };

            while let Ok((mut stream, _)) = listener.accept().await {
                let this = self.clone();
                tokio::spawn(async move {
                    let mut buffer = [0u8; 4096];
                    if let Ok(n) = stream.read(&mut buffer).await {
                        if n == 0 {
                            return;
                        }
                        let request = String::from_utf8_lossy(&buffer[..n]);

                        let (status_line, content_type, body) = if request.starts_with("GET /api/status") {
                            let json = format!(
                                r#"{{"status":"online","port":{},"project":"{}","version":"0.1.0"}}"#,
                                this.port,
                                this.work_dir.file_name().unwrap_or_default().to_string_lossy()
                            );
                            ("HTTP/1.1 200 OK", "application/json", json)
                        } else if request.starts_with("GET /api/config") {
                            let json = serde_json::to_string(&this.config).unwrap_or_else(|_| "{}".to_string());
                            ("HTTP/1.1 200 OK", "application/json", json)
                        } else {
                            // Domyślnie serwuj dashboard Web Companion Single Page App
                            let html = include_str!("dashboard.html");
                            ("HTTP/1.1 200 OK", "text/html; charset=utf-8", html.to_string())
                        };

                        let response = format!(
                            "{}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
                            status_line,
                            content_type,
                            body.len(),
                            body
                        );

                        let _ = stream.write_all(response.as_bytes()).await;
                        let _ = stream.flush().await;
                    }
                });
            }
        });
    }
}
