//! Generyczny driver dla agentów-CLI uruchamianych jako subprocess.
//!
//! Wzorzec: większość agentów-CLI (Devin, Aider, Claude Code, Gemini CLI, Codex)
//! wspiera tryb non-interactive: przyjmują prompt jako argument (lub z stdin),
//! wypisują odpowiedź do stdout i kończą. Ten provider uruchamia taki CLI,
//! streamuje stdout token po tokenie i deleguje do `token_tx`.
//!
//! Dzięki temu opencode-rs staje się meta-agent: może delegować zadania do
//! innych agentów-CLI tak jak do modeli LLM. Każdy agent-CLI to osobna instancja
//! `CliSubprocessProvider` z własnym command + args.
//!
//! Przykłady:
//! - Devin CLI:  `devin --permission-mode dangerous -p "<prompt>"`
//! - Claude Code: `claude -p "<prompt>"`
//! - Aider:      `aider --message "<prompt>"`
//! - Gemini CLI: `gemini -p "<prompt>"`
//! - Codex CLI:  `codex "<prompt>"`

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::Sender;

use super::{ChatMessage, Provider};

/// Konfiguracja jednego agenta-CLI uruchamianego jako subprocess.
#[derive(Debug, Clone)]
pub struct CliSpec {
    /// Nazwa wyświetlana (np. "Devin CLI", "Claude Code CLI").
    pub display_name: &'static str,
    /// Komenda do uruchomienia (np. "devin", "aider", "claude").
    pub command: String,
    /// Stałe argumenty przed promptem (np. ["--permission-mode", "dangerous", "-p"] dla devina).
    pub pre_prompt_args: Vec<String>,
    /// Czy przekazać prompt przez stdin (true) czy jako ostatni argument (false).
    /// Niektóre CLIs (aider) czytają prompt z stdin, inne (devin, claude) jako arg.
    pub prompt_via_stdin: bool,
    /// Opcjonalny flag do ustawienia modelu (np. "--model" dla devina).
    /// Jeśli None, model z opencode-rs jest ignorowany (CLI używa swojego domyślnego).
    pub model_flag: Option<String>,
}

impl CliSpec {
    /// Spec dla Devin CLI (`devin -p "<prompt>"`).
    /// `--permission-mode dangerous` auto-approves all tools (wymagane dla non-interactive).
    /// `--respect-workspace-trust false` pomija trust prompt w -p mode.
    pub fn devin() -> Self {
        Self {
            display_name: "Devin CLI (Subprocess)",
            command: "devin".to_string(),
            pre_prompt_args: vec![
                "--permission-mode".to_string(),
                "dangerous".to_string(),
                "--respect-workspace-trust".to_string(),
                "false".to_string(),
                "-p".to_string(),
            ],
            prompt_via_stdin: false,
            model_flag: Some("--model".to_string()),
        }
    }

    /// Spec dla Claude Code CLI (`claude -p "<prompt>"`).
    pub fn claude_code() -> Self {
        Self {
            display_name: "Claude Code CLI (Subprocess)",
            command: "claude".to_string(),
            pre_prompt_args: vec!["-p".to_string()],
            prompt_via_stdin: false,
            model_flag: Some("--model".to_string()),
        }
    }

    /// Spec dla Aider (`aider --message "<prompt>"`).
    /// Aider domyślnie czyta z stdin jeśli nie podano --message, ale --message jest bezpieczniejsze.
    pub fn aider() -> Self {
        Self {
            display_name: "Aider CLI (Subprocess)",
            command: "aider".to_string(),
            pre_prompt_args: vec!["--message".to_string()],
            prompt_via_stdin: false,
            model_flag: Some("--model".to_string()),
        }
    }

    /// Spec dla Gemini CLI (`gemini -p "<prompt>"`).
    pub fn gemini_cli() -> Self {
        Self {
            display_name: "Gemini CLI (Subprocess)",
            command: "gemini".to_string(),
            pre_prompt_args: vec!["-p".to_string()],
            prompt_via_stdin: false,
            model_flag: Some("--model".to_string()),
        }
    }

    /// Spec dla Codex CLI (`codex "<prompt>"`).
    pub fn codex_cli() -> Self {
        Self {
            display_name: "Codex CLI (Subprocess)",
            command: "codex".to_string(),
            pre_prompt_args: vec![],
            prompt_via_stdin: false,
            model_flag: None,
        }
    }

    /// Spec dla Kilo Code (`kilo run --format json -m <model> "<prompt>"`).
    /// Kilo to fork opencode z 302 modelami (17 darmowych: nvidia nemotron, minimax, ling, poolside, etc.).
    /// Używa `kilo run` (one-shot) bo `kilo acp` nie ma --model flag (issue #8016).
    /// `--format json` daje czyste JSON events zamiast TUI z ANSI codes.
    pub fn kilo_run() -> Self {
        Self {
            display_name: "Kilo Code (Subprocess, 302 modele)",
            command: "kilo".to_string(),
            pre_prompt_args: vec!["run".to_string(), "--format".to_string(), "json".to_string()],
            prompt_via_stdin: false,
            model_flag: Some("-m".to_string()),
        }
    }

    /// Spec dla Cline CLI (`cline --auto-approve true "<prompt>"`).
    /// Cline ma też `--acp` ale wymaga API key — używamy one-shot.
    /// Model przez `-m <model>`, provider przez `-P <id>`.
    pub fn cline() -> Self {
        Self {
            display_name: "Cline CLI (Subprocess)",
            command: "cline".to_string(),
            pre_prompt_args: vec!["--auto-approve".to_string(), "true".to_string()],
            prompt_via_stdin: false,
            model_flag: Some("-m".to_string()),
        }
    }
}

pub struct CliSubprocessProvider {
    spec: CliSpec,
}

impl CliSubprocessProvider {
    pub fn new(spec: CliSpec) -> Self {
        Self { spec }
    }
}

#[async_trait]
impl Provider for CliSubprocessProvider {
    fn name(&self) -> &str {
        self.spec.display_name
    }

    async fn stream_chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        token_tx: Sender<String>,
    ) -> Result<()> {
        // Użyj ostatniego user promptu jako zadania dla agenta-CLI.
        // Agenci-CLI są stateless z perspektywy opencode-rs — nie przekazujemy
        // pełnej historii, tylko aktualne zadanie (CLI ma własną sesję/pamięć).
        let last_prompt = messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.clone())
            .unwrap_or_default();

        if last_prompt.trim().is_empty() {
            anyhow::bail!("Brak promptu usera — agent-CLI potrzebuje zadania.");
        }

        // Na Windows, npm/Volta shims to pliki .cmd które wymagają `cmd /c` do uruchomienia.
        // `Command::new("kilo")` może znaleźć `kilo.cmd` ale z piped stdout bywa problematyczne.
        // Bezpieczniej zawsze używać `cmd /c` na Windows dla npm-installed CLI.
        // .exe (devin, opencode) też działa z cmd /c, więc nie ma straty.
        let mut cmd = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.arg("/c").arg(&self.spec.command);
            c
        } else {
            Command::new(&self.spec.command)
        };
        cmd.args(&self.spec.pre_prompt_args);

        // Jeśli CLI wspiera --model flag i model nie jest pusty, dodaj go.
        // Model z opencode-rs (np. "devin-cli-opus") mapuje na model CLI (np. "opus").
        if let Some(ref flag) = self.spec.model_flag {
            if !model.is_empty()
                && model != "devin-cli" && model != "claude-code-cli"
                && model != "aider-cli" && model != "gemini-cli" && model != "codex-cli"
                && model != "kilo-run" && model != "cline-cli" && model != "cline"
            {
                // Wyciągnij model po prefixie (np. "devin-cli-opus" → "opus")
                let cli_model = model
                    .strip_prefix("devin-cli-")
                    .or_else(|| model.strip_prefix("claude-code-cli-"))
                    .or_else(|| model.strip_prefix("aider-cli-"))
                    .or_else(|| model.strip_prefix("gemini-cli-"))
                    .or_else(|| model.strip_prefix("codex-cli-"))
                    .unwrap_or(model);
                cmd.arg(flag).arg(cli_model);
            }
        }

        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let prompt_via_stdin = self.spec.prompt_via_stdin;
        if prompt_via_stdin {
            cmd.stdin(Stdio::piped());
        } else {
            // Prompt jako ostatni argument (np. `devin -p "<prompt>"`, `kilo run <prompt>`).
            cmd.arg(&last_prompt);
        }

        let mut child = cmd.spawn().map_err(|e| {
            anyhow!(
                "Nie udało się uruchomić agenta-CLI '{}': {e}\n\
                 Sprawdź czy '{}' jest zainstalowany i w PATH.",
                self.spec.command,
                self.spec.command
            )
        })?;

        // Jeśli prompt przez stdin, wyślij go i zamknij stdin.
        if prompt_via_stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(last_prompt.as_bytes()).await?;
                stdin.shutdown().await.ok();
            }
        }

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Brak uchwytu stdout dla agenta-CLI '{}'", self.spec.command))?;

        let mut reader = BufReader::new(stdout).lines();
        let is_kilo = self.spec.command == "kilo";

        while let Some(line) = reader.next_line().await? {
            // Dla Kilo (`--format json`), wyciągnij text z JSON events.
            // Format: {"type":"text","part":{"type":"text","text":"4"}}
            if is_kilo && line.starts_with('{') {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                    if v.get("type").and_then(|t| t.as_str()) == Some("text") {
                        if let Some(text) = v.get("part").and_then(|p| p.get("text")).and_then(|t| t.as_str()) {
                            let _ = token_tx.send(format!("{text}\n")).await;
                            continue;
                        }
                    }
                    // Pomijaj step_start/step_finish/tool events (nie wyświetlaj surowego JSON)
                    if v.get("type").and_then(|t| t.as_str()).map(|s| s.contains("start") || s.contains("finish")).unwrap_or(false) {
                        continue;
                    }
                }
            }
            let _ = token_tx.send(format!("{}\n", line)).await;
        }

        let status = child.wait().await?;
        if !status.success() {
            // Nie błąd jeśli agent-CLI zwrócił non-zero ale coś wypisał — to może być
            // jego sposób sygnalizacji. Zwróć błąd tylko jeśli nic nie wypisał.
            return Err(anyhow!(
                "Agent-CLI '{}' zakończył się kodem błędu: {:?}",
                self.spec.command,
                status.code()
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_devin_spec_has_dangerous_permission() {
        let spec = CliSpec::devin();
        assert_eq!(spec.command, "devin");
        assert!(spec.pre_prompt_args.contains(&"--permission-mode".to_string()));
        assert!(spec.pre_prompt_args.contains(&"dangerous".to_string()));
        assert!(spec.pre_prompt_args.contains(&"-p".to_string()));
        assert!(!spec.prompt_via_stdin);
        assert_eq!(spec.model_flag.as_deref(), Some("--model"));
    }

    #[test]
    fn test_claude_code_spec_uses_p_flag() {
        let spec = CliSpec::claude_code();
        assert_eq!(spec.command, "claude");
        assert!(spec.pre_prompt_args.contains(&"-p".to_string()));
        assert_eq!(spec.model_flag.as_deref(), Some("--model"));
    }

    #[test]
    fn test_aider_spec_uses_message_flag() {
        let spec = CliSpec::aider();
        assert_eq!(spec.command, "aider");
        assert!(spec.pre_prompt_args.contains(&"--message".to_string()));
    }

    #[test]
    fn test_gemini_cli_spec() {
        let spec = CliSpec::gemini_cli();
        assert_eq!(spec.command, "gemini");
        assert!(spec.pre_prompt_args.contains(&"-p".to_string()));
    }

    #[test]
    fn test_codex_cli_spec_no_model_flag() {
        let spec = CliSpec::codex_cli();
        assert_eq!(spec.command, "codex");
        assert!(spec.pre_prompt_args.is_empty());
        assert!(spec.model_flag.is_none());
    }

    #[test]
    fn test_provider_name_uses_display_name() {
        let p = CliSubprocessProvider::new(CliSpec::devin());
        assert_eq!(p.name(), "Devin CLI (Subprocess)");
    }
}
