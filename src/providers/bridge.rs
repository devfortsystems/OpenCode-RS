use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use super::{ChatMessage, Provider};

pub struct BridgeProvider {
    base_url: String,
    client: reqwest::Client,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
}

#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

impl BridgeProvider {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    fn candidate_urls(&self) -> Vec<String> {
        let base = self.base_url.trim_end_matches('/').to_string();
        // Jeśli base to 8765, próbuj też 8766/8767 dla multi-edytorów (Trae+Antigravity+Devin)
        if base.contains("8765") {
            vec![base.clone(), base.replace("8765", "8766"), base.replace("8765", "8767")]
        } else {
            vec![base]
        }
    }
}

#[async_trait]
impl Provider for BridgeProvider {
    fn name(&self) -> &str {
        "Universal Editor Bridge (Cursor / Windsurf / Trae / VS Code)"
    }

    async fn stream_chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        token_tx: Sender<String>,
    ) -> Result<()> {
        let body = ChatRequest {
            model,
            messages,
            stream: true,
        };
        let mut last_err = anyhow!("brak kandydata mostka");
        for base in self.candidate_urls() {
            let url = format!("{}/chat/completions", base.trim_end_matches('/'));
            let resp = self.client.post(&url).json(&body).send().await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    let mut stream = r.bytes_stream();
                    let mut buffer = String::new();
                    while let Some(chunk_res) = stream.next().await {
                        let bytes = chunk_res?;
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(newline_pos) = buffer.find('\n') {
                            let line = buffer[..newline_pos].trim().to_string();
                            buffer.drain(..=newline_pos);
                            if line.starts_with("data: ") {
                                let data = line.trim_start_matches("data: ").trim();
                                if data == "[DONE]" { break; }
                                if let Ok(parsed) = serde_json::from_str::<StreamChunk>(data) {
                                    if let Some(choice) = parsed.choices.first() {
                                        if let Some(ref text) = choice.delta.content {
                                            if !text.is_empty() { let _ = token_tx.send(text.clone()).await; }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    return Ok(());
                },
                Ok(r) => {
                    let status = r.status();
                    let err_text = r.text().await.unwrap_or_default();
                    last_err = anyhow!("Błąd mostka HTTP {status} @ {base}: {err_text}");
                    continue;
                },
                Err(e) => { last_err = anyhow!("Nie można połączyć się z mostkiem ({base}/chat/completions): {e}"); continue; }
            }
        }
        Err(last_err)
    }
}
