use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use super::{ChatMessage, Provider};

pub struct DirectApiProvider {
    endpoint_url: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

#[derive(Serialize)]
struct DirectChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
}

#[derive(Deserialize)]
struct DirectStreamChunk {
    choices: Vec<DirectStreamChoice>,
}

#[derive(Deserialize)]
struct DirectStreamChoice {
    delta: DirectStreamDelta,
}

#[derive(Deserialize)]
struct DirectStreamDelta {
    content: Option<String>,
}

// ─── ANTHROPIC NATIVE MESSAGES API TYPES ─────────────────────────────────────

#[derive(Serialize)]
struct AnthropicMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct AnthropicChatRequest<'a> {
    model: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    messages: Vec<AnthropicMessage<'a>>,
    max_tokens: usize,
    stream: bool,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct AnthropicStreamEvent {
    #[serde(rename = "type")]
    event_type: Option<String>,
    delta: Option<AnthropicDelta>,
}

#[derive(Deserialize)]
struct AnthropicDelta {
    text: Option<String>,
}

impl DirectApiProvider {
    pub fn new(endpoint_url: String, api_key: Option<String>) -> Self {
        Self {
            endpoint_url,
            api_key,
            client: reqwest::Client::new(),
        }
    }

    fn is_anthropic(&self) -> bool {
        self.endpoint_url.contains("anthropic.com")
    }

    async fn stream_anthropic(
        &self,
        model: &str,
        messages: &[ChatMessage],
        token_tx: Sender<String>,
    ) -> Result<()> {
        let url = if self.endpoint_url.ends_with("/v1/messages") {
            self.endpoint_url.clone()
        } else {
            format!("{}/messages", self.endpoint_url.trim_end_matches('/'))
        };

        // W protokole Anthropic system prompt musi być w dedykowanym polu najwyższego poziomu
        let system_prompt = messages
            .iter()
            .find(|m| m.role == "system")
            .map(|m| m.content.as_str());

        let anthropic_messages: Vec<AnthropicMessage> = messages
            .iter()
            .filter(|m| m.role == "user" || m.role == "assistant")
            .map(|m| AnthropicMessage {
                role: if m.role == "user" { "user" } else { "assistant" },
                content: &m.content,
            })
            .collect();

        // Normalizacja nazwy modelu dla Anthropic API
        let target_model = if model.contains("claude-3-7-sonnet") {
            "claude-3-7-sonnet-20250219"
        } else if model.contains("claude-3-5-sonnet") {
            "claude-3-5-sonnet-20241022"
        } else if model.contains("claude-3-5-haiku") {
            "claude-3-5-haiku-20241022"
        } else if model.contains("opus") {
            "claude-3-opus-20240229"
        } else {
            model
        };

        let body = AnthropicChatRequest {
            model: target_model,
            system: system_prompt,
            messages: anthropic_messages,
            max_tokens: 8192,
            stream: true,
        };

        let mut req = self
            .client
            .post(&url)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body);

        if let Some(ref key) = self.api_key {
            req = req.header("x-api-key", key);
        }

        let response = req
            .send()
            .await
            .map_err(|e| anyhow!("Błąd połączenia z Anthropic API ({url}): {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("Anthropic API błąd {status}: {err_text}"));
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
                    if let Ok(event) = serde_json::from_str::<AnthropicStreamEvent>(data) {
                        if let Some(delta) = event.delta {
                            if let Some(text) = delta.text {
                                if !text.is_empty() {
                                    let _ = token_tx.send(text).await;
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

#[async_trait]
impl Provider for DirectApiProvider {
    fn name(&self) -> &str {
        if self.is_anthropic() {
            "Anthropic Direct API Provider (Claude Messages API)"
        } else {
            "Direct API Provider (OpenAI / Gemini / Groq / Ollama / DeepSeek)"
        }
    }

    async fn stream_chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        token_tx: Sender<String>,
    ) -> Result<()> {
        if self.is_anthropic() {
            return self.stream_anthropic(model, messages, token_tx).await;
        }

        let url = format!("{}/chat/completions", self.endpoint_url.trim_end_matches('/'));
        let body = DirectChatRequest {
            model,
            messages,
            stream: true,
        };

        let mut req = self.client.post(&url).json(&body);
        if let Some(ref key) = self.api_key {
            req = req.bearer_auth(key);
        }

        let response = req
            .send()
            .await
            .map_err(|e| anyhow!("Błąd połączenia z Direct API ({url}): {e}"))?;

        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("Direct API błąd {status}: {err_text}"));
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
                    if let Ok(parsed) = serde_json::from_str::<DirectStreamChunk>(data) {
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
