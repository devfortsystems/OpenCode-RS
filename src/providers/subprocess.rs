use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::Sender;

use super::{ChatMessage, Provider};

pub struct SubprocessProvider {
    command_name: String,
}

impl SubprocessProvider {
    pub fn new(command_name: String) -> Self {
        Self { command_name }
    }
}

#[async_trait]
impl Provider for SubprocessProvider {
    fn name(&self) -> &str {
        "Subprocess CLI Driver (CommandCode / Claude Code)"
    }

    async fn stream_chat(
        &self,
        _model: &str,
        messages: &[ChatMessage],
        token_tx: Sender<String>,
    ) -> Result<()> {
        let last_prompt = messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.clone())
            .unwrap_or_default();

        // Fallback: spróbuj `commandcode`, potem `cmd` (wersja `cmd` u użytkownika)
        let mut child = match Command::new(&self.command_name)
            .arg(&last_prompt)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) if self.command_name == "commandcode" => Command::new("cmd")
                .arg(&last_prompt)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|_| anyhow!("Nie udało się uruchomić procesu {} ani fallback `cmd`: {e}", self.command_name))?,
            Err(e) => return Err(anyhow!("Nie udało się uruchomić procesu {}: {e}", self.command_name)),
        };

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Brak uchwytu stdout dla procesu"))?;

        let mut reader = BufReader::new(stdout).lines();

        while let Some(line) = reader.next_line().await? {
            let _ = token_tx.send(format!("{}\n", line)).await;
        }

        let status = child.wait().await?;
        if !status.success() {
            return Err(anyhow!("Proces zakończył się kodem błędu: {:?}", status.code()));
        }

        Ok(())
    }
}
