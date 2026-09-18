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

    pub fn candidate_urls(&self) -> Vec<String> {
        let base = self.base_url.trim_end_matches('/').to_string();
        // Wyciągnij "prefiks" (host:port) i "suffix" (ścieżka np. /v1) z URL.
        // Schematy wejściowe:
        //   http://127.0.0.1:8765/v1  → prefix="http://127.0.0.1:", suffix_start=/v1
        //   http://127.0.0.1:8767     → prefix="http://127.0.0.1:", suffix_start=""
        //   http://localhost:9000/bridge → custom port, dodany na początek, potem standardowe.
        let mut candidates: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        // Schemat: znajdź ostatni ':' — po nim jest numer portu i opcjonalnie '/'.
        if let Some(colon_idx) = base.rfind(':') {
            let after_colon = colon_idx + 1;
            // Znajdź '/' po ':' (oznacza początek ścieżki).
            let slash_idx = base[after_colon..].find('/').map(|i| after_colon + i);
            let prefix = base[..colon_idx + 1].to_string(); // np. "http://127.0.0.1:"
            let suffix = if let Some(s) = slash_idx { base[s..].to_string() } else { String::new() }; // np. "/v1"
            let current_port_str = &base[after_colon..slash_idx.unwrap_or(base.len())];
            let current_port_is_standard = ["8765","8766","8767"].contains(&current_port_str);
            // Jeśli użytkownik podał custom port (nie standard) → najpierw próbuj dokładnie jego.
            if !current_port_is_standard {
                let custom = format!("{prefix}{current_port_str}{suffix}");
                if seen.insert(custom.clone()) { candidates.push(custom); }
            }
            // Następnie wszystkie 3 standardowe porty.
            for port in [8765u16, 8766u16, 8767u16] {
                let url = format!("{prefix}{port}{suffix}");
                if seen.insert(url.clone()) { candidates.push(url); }
            }
        } else {
            // Brak ':' — dodajemy base i standardowe porty jako fallback (raczej nie wystąpi).
            candidates.push(base.clone());
        }
        candidates
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

        // Szybki klient do pre-check /health z timeoutem per kandydat.
        // Poprzednio (przed fazą 5): 3 kandydatów × 30s timeout = 90s czekania przy OFFLINE bridge.
        // Faza 5 (ROOT #6): 3 × 400ms = 1.2s pre-check → natychmiast Err(offline) — za krótko dla Trae/Volta node.
        // ROOT #10 dzisiaj: 3 × 1200ms = 3.6s — bezpieczne dla wolno startujących mostków (Node http server),
        // a nadal 25× szybciej niż stare 90s.
        let fast_client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_millis(900))
            .timeout(std::time::Duration::from_millis(1200))
            .build()
            .unwrap_or_else(|_| self.client.clone());

        for base in self.candidate_urls() {
            // ══════════════════════════════════════════════════════════════════
            // PRE-CHECK /health — szybko pomiń kandydata zanim wyślesz ciężki
            // POST /chat/completions z 30s TCP timeoutem.
            // Jeśli mostek nie odpowie w 400ms na GET /health → nie istnieje.
            // ══════════════════════════════════════════════════════════════════
            let health_url = format!("{}/health", base.trim_end_matches('/'));
            match fast_client.get(&health_url).send().await {
                Ok(r) if r.status().is_success() => {} // Bridge ACTIVE → kontynuuj POST
                _ => {
                    last_err = anyhow!("Mostek offline (GET /health nie odpowiada): {base}");
                    continue;
                }
            }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(base: &str) -> Vec<String> {
        let b = BridgeProvider::new(base.to_string());
        b.candidate_urls()
    }

    #[test]
    fn candidate_urls_default_8765_zwraca_wszystkie_trzy() {
        let list = urls("http://127.0.0.1:8765/v1");
        assert_eq!(list.len(), 3);
        assert!(list.contains(&"http://127.0.0.1:8765/v1".to_string()));
        assert!(list.contains(&"http://127.0.0.1:8766/v1".to_string()));
        assert!(list.contains(&"http://127.0.0.1:8767/v1".to_string()));
    }

    #[test]
    fn candidate_urls_default_8767_bez_v1_też_wszystkie_trzy() {
        let list = urls("http://127.0.0.1:8767");
        assert_eq!(list.len(), 3);
        assert!(list.contains(&"http://127.0.0.1:8765".to_string()));
        assert!(list.contains(&"http://127.0.0.1:8766".to_string()));
        assert!(list.contains(&"http://127.0.0.1:8767".to_string()));
    }

    #[test]
    fn candidate_urls_custom_port_najpierw_custom_potem_standardowe() {
        let list = urls("http://localhost:9123/bridge");
        assert_eq!(list.len(), 4);
        assert_eq!(list[0], "http://localhost:9123/bridge".to_string());
        assert!(list.contains(&"http://localhost:8765/bridge".to_string()));
        assert!(list.contains(&"http://localhost:8766/bridge".to_string()));
        assert!(list.contains(&"http://localhost:8767/bridge".to_string()));
    }
}
