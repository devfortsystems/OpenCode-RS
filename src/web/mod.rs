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
                    let mut buffer = vec![0u8; 65536]; // większy bufor dla uploadów
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
                        } else if request.starts_with("POST /api/upload") {
                            // Multipart upload — pliki lądują w dropzone
                            let body_bytes = Self::extract_body(&buffer, n, &request);
                            match Self::handle_upload(&body_bytes, &request) {
                                Ok(msg) => {
                                    let json = format!(r#"{{"status":"ok","message":"{}","files":{}}}"#, msg.replace('"', "\\\""), "[]");
                                    ("HTTP/1.1 200 OK", "application/json", json)
                                }
                                Err(e) => {
                                    let json = format!(r#"{{"status":"error","message":"{}"}}"#, e.to_string().replace('"', "\\\""));
                                    ("HTTP/1.1 400 Bad Request", "application/json", json)
                                }
                            }
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

    /// Wyciąga body z requestu HTTP (po \r\n\r\n).
    fn extract_body(buffer: &[u8], n: usize, request: &str) -> Vec<u8> {
        if let Some(idx) = request.find("\r\n\r\n") {
            buffer[idx + 4..n].to_vec()
        } else {
            Vec::new()
        }
    }

    /// Obsługa multipart/form-data upload — zapisuje pliki do dropzone.
    fn handle_upload(body: &[u8], request: &str) -> Result<String, std::io::Error> {
        // Znajdź boundary z Content-Type header
        let boundary = request
            .lines()
            .find(|l| l.to_lowercase().starts_with("content-type:"))
            .and_then(|l| l.split("boundary=").nth(1))
            .map(|b| b.trim().trim_matches('"').to_string())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Brak boundary"))?;

        let dropzone = crate::transfer::dropzone_dir();
        std::fs::create_dir_all(&dropzone)?;

        let boundary_bytes = format!("--{boundary}").into_bytes();
        let mut files_saved = 0;
        let mut pos = 0;

        while pos < body.len() {
            // Znajdź następny boundary
            let next = Self::find_subslice(&body[pos..], &boundary_bytes);
            if next.is_none() {
                break;
            }
            let start = pos + next.unwrap() + boundary_bytes.len();
            // Pomiń \r\n po boundary
            let part_start = if start + 2 <= body.len() && body[start..start + 2] == *b"\r\n" { start + 2 } else { start };

            // Znajdź koniec part (następny boundary lub koniec)
            let next_boundary = Self::find_subslice(&body[part_start..], &boundary_bytes);
            let part_end = if let Some(ne) = next_boundary {
                let abs_end = part_start + ne;
                // Pomiń \r\n przed boundary
                if abs_end >= 2 && body[abs_end - 2..abs_end] == *b"\r\n" { abs_end - 2 } else { abs_end }
            } else {
                body.len()
            };

            let part = &body[part_start..part_end];
            // Wyciągnij filename z nagłówka part
            let part_str = String::from_utf8_lossy(part);
            if let Some(hdr_end) = part_str.find("\r\n\r\n") {
                let headers = &part_str[..hdr_end];
                let content = &part[hdr_end + 4..];

                // Szukaj filename="..."
                let filename = headers
                    .split(';')
                    .find_map(|seg| {
                        let seg = seg.trim();
                        if seg.starts_with("filename=") {
                            Some(seg[9..].trim_matches('"').to_string())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| format!("upload_{}", std::time::SystemTime::now().elapsed().unwrap_or_default().as_millis()));

                if !filename.is_empty() && !content.is_empty() {
                    // Bezpieczna nazwa pliku — usuń ścieżki
                    let safe_name = std::path::Path::new(&filename)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or(filename);
                    let dest = dropzone.join(&safe_name);
                    std::fs::write(&dest, content)?;
                    files_saved += 1;
                }
            }

            pos = part_end;
        }

        Ok(format!("Zapisano {files_saved} plik(ów) do dropzone"))
    }

    /// Znajduje podciąg w buforze — prosty searcher.
    fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() || haystack.len() < needle.len() {
            return None;
        }
        haystack.windows(needle.len()).position(|w| w == needle)
    }
}
