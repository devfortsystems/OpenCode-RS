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
            client: reqwest::Client::new(),
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
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = ChatRequest {
            model,
            messages,
            stream: true,
        };

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow!("Nie można połączyć się z mostkiem ({url}): {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("Błąd mostka HTTP {status}: {err_text}"));
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_res) = stream.next().await {
            let bytes = chunk_res?;
            buffer.push_str(&String::from_utf8_lossy(&bytes));

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer.drain(..=newline_pos);

                if line.starts_with("data: ") {
                    let data = line.trim_start_matches("data: ").trim();
                    if data == "[DONE]" {
                        break;
                    }
                    if let Ok(parsed) = serde_json::from_str::<StreamChunk>(data) {
                        if let Some(choice) = parsed.choices.first() {
                            if let Some(ref text) = choice.delta.content {
                                if !text.is_empty() {
                                    let _ = token_tx.send(text.clone()).await;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
