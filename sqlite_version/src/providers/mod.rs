use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::Sender;

use crate::config::AppConfig;

pub mod acp;
pub mod antigravity;
pub mod bridge;
pub mod cli_subprocess;
pub mod devin_cloud;
pub mod direct;
pub mod subprocess;

use acp::AcpClientProvider;
use antigravity::AntigravityProvider;
use bridge::BridgeProvider;
use cli_subprocess::{CliSpec, CliSubprocessProvider};
use devin_cloud::DevinCloudProvider;
use direct::DirectApiProvider;
use subprocess::SubprocessProvider;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn name(&self) -> &str;
    async fn stream_chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        token_tx: Sender<String>,
    ) -> Result<()>;
}

pub struct ProviderRouter {
    config: AppConfig,
    work_dir: PathBuf,
    bridge: Arc<BridgeProvider>,
    direct_gemini: Option<Arc<DirectApiProvider>>,
    direct_groq: Option<Arc<DirectApiProvider>>,
    direct_openai: Option<Arc<DirectApiProvider>>,
    direct_anthropic: Option<Arc<DirectApiProvider>>,
    direct_deepseek: Option<Arc<DirectApiProvider>>,
    direct_mistral: Option<Arc<DirectApiProvider>>,
    direct_openrouter: Option<Arc<DirectApiProvider>>,
    direct_commandcode: Option<Arc<DirectApiProvider>>,
    // Devin Cloud (api.devin.ai v3) — sesje w chmurze. None jeśli brak DEVIN_API_KEY/DEVIN_ORG_ID.
    devin_cloud: Option<Arc<DevinCloudProvider>>,
    // ACP (Agent Client Protocol) — pełna integracja JSON-RPC over stdio z `devin acp`.
    // Bogatsza niż subprocess: streaming, plan updates, tool call visibility, thought streaming.
    devin_acp: Arc<AcpClientProvider>,
    devin_acp_opus: Arc<AcpClientProvider>,
    devin_acp_sonnet: Arc<AcpClientProvider>,
    devin_acp_codex: Arc<AcpClientProvider>,
    // OpenCode ACP (oryginalny opencode v1.18.29+, 127+ modeli w tym darmowe).
    // Nie wymaga wtyczki — to CLI (`opencode acp`), nie bridge.
    opencode_acp: Arc<AcpClientProvider>,
    // Gemini CLI ACP (darmowy tier 60 req/min, wymaga `gemini` login).
    gemini_acp: Arc<AcpClientProvider>,
    // Claude Code ACP (wymaga ANTHROPIC_API_KEY lub Claude Pro/Max).
    claude_code_acp: Arc<AcpClientProvider>,
    // Codex ACP (wymaga OPENAI_API_KEY).
    codex_acp: Arc<AcpClientProvider>,
    ollama: Arc<DirectApiProvider>,
    lmstudio: Arc<DirectApiProvider>,
    llamacpp: Arc<DirectApiProvider>,
    custom_endpoints: Vec<(String, Arc<DirectApiProvider>)>,
    commandcode: Arc<SubprocessProvider>,
    // Agenci-CLI uruchamiani jako subprocess (Devin, Claude Code, Aider, Gemini, Codex).
    // opencode-rs jako meta-agent deleguje zadania do tych agentów.
    devin_cli: Arc<CliSubprocessProvider>,
    claude_code_cli: Arc<CliSubprocessProvider>,
    aider_cli: Arc<CliSubprocessProvider>,
    gemini_cli: Arc<CliSubprocessProvider>,
    codex_cli: Arc<CliSubprocessProvider>,
    kilo_run: Arc<CliSubprocessProvider>,
    kilo_run_free: Arc<CliSubprocessProvider>,
    cline_cli: Arc<CliSubprocessProvider>,
    // Antigravity IDE — bezpośrednie połączenie gRPC-Web z language_server.exe.
    // 32 modele (Gemini 3.x, Claude 4.6, GPT-OSS) — wszystkie darmowe (free-tier).
    // Lazy init: provider tworzony przy pierwszym użyciu (wymaga uruchomionego IDE).
    antigravity: tokio::sync::OnceCell<Arc<AntigravityProvider>>,
}

impl ProviderRouter {
    pub fn new(config: AppConfig, work_dir: PathBuf) -> Self {
        let bridge = Arc::new(BridgeProvider::new(config.bridge_url.clone()));
        
        let direct_gemini = config.direct_gemini_api_key.as_ref().map(|k| {
            Arc::new(DirectApiProvider::new(
                "https://generativelanguage.googleapis.com/v1beta/openai".to_string(),
                Some(k.clone()),
            ))
        });

        let direct_groq = config.direct_groq_api_key.as_ref().map(|k| {
            Arc::new(DirectApiProvider::new(
                "https://api.groq.com/openai/v1".to_string(),
                Some(k.clone()),
            ))
        });

        let direct_openai = config.direct_openai_api_key.as_ref().map(|k| {
            Arc::new(DirectApiProvider::new(
                "https://api.openai.com/v1".to_string(),
                Some(k.clone()),
            ))
        });

        let direct_anthropic = config.direct_anthropic_api_key.as_ref().map(|k| {
            Arc::new(DirectApiProvider::new(
                "https://api.anthropic.com/v1".to_string(),
                Some(k.clone()),
            ))
        });

        let direct_deepseek = config.direct_deepseek_api_key.as_ref().map(|k| {
            Arc::new(DirectApiProvider::new(
                "https://api.deepseek.com/v1".to_string(),
                Some(k.clone()),
            ))
        });

        let direct_mistral = config.direct_mistral_api_key.as_ref().map(|k| {
            Arc::new(DirectApiProvider::new(
                "https://api.mistral.ai/v1".to_string(),
                Some(k.clone()),
            ))
        });

        let direct_openrouter = config.direct_openrouter_api_key.as_ref().map(|k| {
            Arc::new(DirectApiProvider::new(
                "https://openrouter.ai/api/v1".to_string(),
                Some(k.clone()),
            ))
        });

        let direct_commandcode = config.commandcode_api_key.as_ref().map(|k| {
            Arc::new(DirectApiProvider::new(
                config.commandcode_base_url.clone().unwrap_or_else(|| "https://api.commandcode.ai/v1".to_string()),
                Some(k.clone()),
            ))
        });

        // Devin Cloud (api.devin.ai v3) — wymaga DEVIN_API_KEY + DEVIN_ORG_ID.
        // Jak brak, provider jest None i modele devin-cloud-* nie są dostępne.
        let devin_cloud = match (&config.devin_api_key, &config.devin_org_id) {
            (Some(key), Some(org)) if !key.is_empty() && !org.is_empty() => {
                Some(Arc::new(DevinCloudProvider::new(key.clone(), org.clone(), work_dir.clone())))
            }
            _ => None,
        };

        // ACP (Agent Client Protocol) — pełna integracja JSON-RPC z `devin acp`.
        // work_dir potrzebne do wstrzykiwania planu + memory blocks w prompt delegata.
        let devin_acp = Arc::new(AcpClientProvider::devin(None, work_dir.clone()));
        let devin_acp_opus = Arc::new(AcpClientProvider::devin(Some("opus"), work_dir.clone()));
        let devin_acp_sonnet = Arc::new(AcpClientProvider::devin(Some("sonnet"), work_dir.clone()));
        let devin_acp_codex = Arc::new(AcpClientProvider::devin(Some("codex"), work_dir.clone()));

        // OpenCode ACP (oryginalny opencode) — 131 modeli wykrywanych dynamicznie.
        // Nie wymaga wtyczki — to CLI. Model przez env var OPENCODE_MODEL.
        // Dynamiczne modele (opencode-acp/<model>) tworzą nowy provider na żądanie.
        let opencode_acp = Arc::new(AcpClientProvider::opencode(None, work_dir.clone()));

        // Gemini CLI ACP — darmowy tier (60 req/min, 1000/day), wymaga `gemini` login.
        let gemini_acp = Arc::new(AcpClientProvider::gemini(work_dir.clone()));
        // Claude Code ACP — wymaga ANTHROPIC_API_KEY lub Claude Pro/Max subscription.
        let claude_code_acp = Arc::new(AcpClientProvider::claude_code(work_dir.clone()));
        // Codex ACP — wymaga OPENAI_API_KEY.
        let codex_acp = Arc::new(AcpClientProvider::codex(work_dir.clone()));

        let ollama = Arc::new(DirectApiProvider::new(
            config.ollama_url.clone().unwrap_or_else(|| "http://localhost:11434/v1".to_string()),
            None,
        ));

        let lmstudio = Arc::new(DirectApiProvider::new(
            config.lmstudio_url.clone().unwrap_or_else(|| "http://localhost:1234/v1".to_string()),
            None,
        ));

        let llamacpp = Arc::new(DirectApiProvider::new(
            config.llamacpp_url.clone().unwrap_or_else(|| "http://localhost:8080/v1".to_string()),
            None,
        ));

        let mut custom_endpoints = Vec::new();
        for ep in &config.custom_endpoints {
            custom_endpoints.push((
                ep.id.clone(),
                Arc::new(DirectApiProvider::new(ep.base_url.clone(), ep.api_key.clone())),
            ));
        }

        let commandcode = Arc::new(SubprocessProvider::new("commandcode".to_string()));

        // Agenci-CLI jako subprocess — opencode-rs jako meta-agent deleguje do nich.
        let devin_cli = Arc::new(CliSubprocessProvider::new(CliSpec::devin()));
        let claude_code_cli = Arc::new(CliSubprocessProvider::new(CliSpec::claude_code()));
        let aider_cli = Arc::new(CliSubprocessProvider::new(CliSpec::aider()));
        let gemini_cli = Arc::new(CliSubprocessProvider::new(CliSpec::gemini_cli()));
        let codex_cli = Arc::new(CliSubprocessProvider::new(CliSpec::codex_cli()));
        // Kilo Code (fork opencode, 302 modele, 17 darmowych) — `kilo run -m <model>`.
        let kilo_run = Arc::new(CliSubprocessProvider::new(CliSpec::kilo_run()));
        let kilo_run_free = Arc::new(CliSubprocessProvider::new(CliSpec::kilo_run()));
        // Cline CLI — `cline --auto-approve true -m <model>`.
        let cline_cli = Arc::new(CliSubprocessProvider::new(CliSpec::cline()));

        Self {
            config,
            work_dir,
            bridge,
            direct_gemini,
            direct_groq,
            direct_openai,
            direct_anthropic,
            direct_deepseek,
            direct_mistral,
            direct_openrouter,
            direct_commandcode,
            devin_cloud,
            devin_acp,
            devin_acp_opus,
            devin_acp_sonnet,
            devin_acp_codex,
            opencode_acp,
            gemini_acp,
            claude_code_acp,
            codex_acp,
            ollama,
            lmstudio,
            llamacpp,
            custom_endpoints,
            commandcode,
            devin_cli,
            claude_code_cli,
            aider_cli,
            gemini_cli,
            codex_cli,
            kilo_run,
            kilo_run_free,
            cline_cli,
            antigravity: tokio::sync::OnceCell::new(),
        }
    }

    pub fn get_available_models(&self) -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            // 🪐 Google Antigravity IDE — gRPC-Web direct, modele wykrywane dynamicznie
            // Wymaga uruchomionego Antigravity IDE. Statyczne modele usunięte — discovery używa language_server.exe.
            // Zobacz `opencode-rs models antigravity` dla aktualnej listy.

            // 🎯 Trae AI — realne modele z docs.trae.ai (Claude usunięty 11.2025, teraz Seed/Kimi/MiniMax/Gemini/GPT-5)
            // wildcard: każdy `trae-*` → Bridge (np. przyszły Seed-2.5 zadziała bez zmiany kodu)
            ("trae-seed-2.1-turbo", "Seed 2.1 Turbo (Trae/ByteDance)", "trae"),
            ("trae-seed-2.1-pro", "Seed 2.1 Pro (Trae/ByteDance)", "trae"),
            ("trae-kimi-k2.5", "Kimi K2.5 (Trae/Moonshot)", "trae"),
            ("trae-minimax-m3", "MiniMax M3 (Trae/MiniMax)", "trae"),
            ("trae-minimax-m2.7", "MiniMax M2.7 (Trae)", "trae"),
            ("trae-gemini-3.1-pro", "Gemini 3.1 Pro Preview (Trae)", "trae"),
            ("trae-gpt-5.4", "GPT-5.4 (Trae/OpenAI) *nie-US", "trae"),

            // 🌊 Windsurf Cascade (rebrand → Devin)
            ("windsurf-cascade-sonnet", "Claude 3.7 Sonnet (Cascade Flow)", "windsurf"),
            ("windsurf-cascade-gpt-4o", "GPT-4o (Cascade Flow)", "windsurf"),
            ("devin-cascade-sonnet", "Claude 3.7 Sonnet (Devin Cascade)", "windsurf"),
            ("devin-cascade-gpt-4o", "GPT-4o (Devin Cascade)", "windsurf"),

            // 🔮 Cursor Pro Bridge
            ("cursor-claude-3-7-sonnet", "Claude 3.7 Sonnet Thinking (Cursor Pro)", "cursor"),
            ("cursor-claude-3-5-sonnet", "Claude 3.5 Sonnet (Cursor Pro)", "cursor"),
            ("cursor-gpt-4o", "GPT-4o (Cursor Pro)", "cursor"),
            ("cursor-o3-mini", "o3-mini High (Cursor Pro)", "cursor"),
            ("cursor-deepseek-r1", "DeepSeek R1 (Cursor Pro)", "cursor"),

            // 📎 GitHub Copilot (via vscode.lm / Bridge)
            ("copilot-gpt-4o", "GPT-4o (GitHub Copilot)", "copilot"),
            ("copilot-claude-3-7-sonnet", "Claude 3.7 Sonnet (GitHub Copilot)", "copilot"),

            // ⌨️ Command Code
            ("commandcode-claude-3-7-sonnet", "Claude 3.7 Sonnet Thinking (Command Code)", "commandcode"),
            ("commandcode-claude-3-7-thinking", "Claude 3.7 Extended Thinking (Command Code)", "commandcode"),
            ("commandcode-claude-3-5-sonnet", "Claude 3.5 Sonnet v2 (Command Code)", "commandcode"),
            ("commandcode-claude-3-5-haiku", "Claude 3.5 Haiku Ultra Fast (Command Code)", "commandcode"),
            ("commandcode-opus-3", "Claude 3 Opus (Command Code)", "commandcode"),
            ("commandcode-gpt-4o", "GPT-4o Omnimodal (Command Code)", "commandcode"),
            ("commandcode-o3-mini", "o3-mini High Reasoning (Command Code)", "commandcode"),
            ("commandcode-deepseek-r1", "DeepSeek R1 (Command Code)", "commandcode"),
            ("commandcode-cli", "Command Code CLI (Direct Subprocess Native)", "commandcode"),

            // 🤝 Agenci-CLI jako subprocess (opencode-rs jako meta-agent deleguje zadania)
            // Każdy model = uruchomienie agenta-CLI w trybie non-interactive (-p / --message).
            // Modele z sufiksem (-opus, -sonnet) przekazują model do CLI przez --model flag.
            ("devin-cli", "Devin CLI (domyślny model)", "devin-cli"),
            // Dynamiczne modele: devin-cli/<model_uid> (np. devin-cli/claude-opus-5-medium)
            // Wykrywane z `devin models list` — 46 rodzin, 100+ modeli
            ("claude-code-cli", "Claude Code CLI (Subprocess, domyślny)", "claude-code-cli"),
            ("claude-code-cli-sonnet", "Claude Code CLI → Sonnet (Subprocess)", "claude-code-cli"),
            ("claude-code-cli-opus", "Claude Code CLI → Opus (Subprocess)", "claude-code-cli"),
            ("aider-cli", "Aider CLI (Subprocess, domyślny model)", "aider-cli"),
            ("gemini-cli", "Gemini CLI (Subprocess, domyślny)", "gemini-cli"),
            ("codex-cli", "Codex CLI (Subprocess)", "codex-cli"),

            // ☁️ Devin Cloud (api.devin.ai v3) — sesje w chmurze (pełny VM, shell, browser)
            // Wymaga DEVIN_API_KEY + DEVIN_ORG_ID. devin_mode = tryb agenta.
            ("devin-cloud", "Devin Cloud (Normal mode, api.devin.ai v3)", "devin-cloud"),
            ("devin-cloud-fast", "Devin Cloud Fast (2x szybszy, 4x droższy)", "devin-cloud"),
            ("devin-cloud-lite", "Devin Cloud Lite (lekki, tani)", "devin-cloud"),
            ("devin-cloud-ultra", "Devin Cloud Ultra (najpotężniejszy)", "devin-cloud"),
            ("devin-cloud-fusion", "Devin Cloud Fusion (hybrydowy)", "devin-cloud"),

            // 🔌 ACP (Agent Client Protocol) — pełna integracja JSON-RPC z `devin acp`
            // Bogatsza niż devin-cli (subprocess): streaming, plan, tool calls, thoughts.
            ("devin-acp", "Devin ACP (JSON-RPC, domyślny model)", "devin-acp"),
            ("devin-acp-opus", "Devin ACP → Opus (JSON-RPC streaming)", "devin-acp"),
            ("devin-acp-sonnet", "Devin ACP → Sonnet (JSON-RPC streaming)", "devin-acp"),
            ("devin-acp-codex", "Devin ACP → Codex (JSON-RPC streaming)", "devin-acp"),

            // 🔌 OpenCode ACP (oryginalny opencode v1.18.21, 131 modeli)
            // Nie wymaga wtyczki — to CLI (`opencode acp`). Modele wykrywane dynamicznie przez `opencode models`.
            // Wpisz /models lub Ctrl+M aby zobaczyć wszystkie 131 modeli (opencode/, opencode-go/, commandcode/, google/).
            ("opencode-acp", "OpenCode ACP (domyślny model)", "opencode-acp"),
            // Skróty do najlepszych modeli opencode:
            ("opencode-zen", "OpenCode Zen → opencode/big-pickle (najlepszy, darmowy)", "opencode-acp"),
            ("opencode-go", "OpenCode Go → opencode-go/glm-5.3 (flagship)", "opencode-acp"),

            // 🔌 Kilo Code (fork opencode, 302 modele, 17 darmowych) — `kilo run -m <model>`
            // Nie wymaga wtyczki — to CLI. Darmowe: nvidia nemotron, minimax, ling, poolside, etc.
            ("kilo-run", "Kilo Code (domyślny model)", "kilo-run"),
            ("kilo-run-free", "Kilo Code → Nemotron 3.5 Lightning (DARMOWY)", "kilo-run"),

            // 🔌 Cline CLI (`cline --auto-approve true -m <model>`)
            // Wymaga API key (cline auth). Ma też --acp ale one-shot jest prostsze.
            ("cline-cli", "Cline CLI (auto-approve, domyślny model)", "cline-cli"),

            // 🔌 Gemini CLI ACP (darmowy tier 60 req/min, wymaga `gemini` login)
            ("gemini-acp", "Gemini CLI ACP (DARMOWY tier, Google account)", "gemini-acp"),

            // 🔌 Claude Code ACP (wymaga ANTHROPIC_API_KEY lub Claude Pro/Max)
            ("claude-code-acp", "Claude Code ACP (Anthropic, Pro/Max lub API key)", "claude-code-acp"),

            // 🔌 Codex ACP (wymaga OPENAI_API_KEY)
            ("codex-acp", "Codex ACP (OpenAI, wymaga API key)", "codex-acp"),

            // 🌐 Direct Gemini API
            ("gemini-3.7-flash", "Gemini 3.7 Flash (Direct API)", "gemini"),
            ("gemini-3.7-pro", "Gemini 3.7 Pro (Direct API)", "gemini"),
            ("gemini-3.7-flash-thinking", "Gemini 3.7 Flash Thinking (Direct API)", "gemini"),
            ("gemini-3.1-pro", "Gemini 3.1 Pro Preview (Direct API)", "gemini"),

            // 🤖 Direct OpenAI API
            ("openai/gpt-4o", "GPT-4o Omnimodal (Direct OpenAI)", "openai"),
            ("openai/o3-mini", "o3-mini High Reasoning (Direct OpenAI)", "openai"),
            ("openai/o1", "o1 Full Reasoning (Direct OpenAI)", "openai"),
            ("openai/gpt-4o-mini", "GPT-4o Mini Fast (Direct OpenAI)", "openai"),

            // 🧠 Direct Anthropic API
            ("anthropic/claude-sonnet-4-5", "Claude Sonnet 4.5 (Direct Anthropic)", "anthropic"),
            ("anthropic/claude-opus-4-1", "Claude Opus 4.1 (Direct Anthropic)", "anthropic"),
            ("anthropic/claude-3-7-sonnet", "Claude 3.7 Sonnet (Direct Anthropic)", "anthropic"),
            ("anthropic/claude-3-5-haiku", "Claude 3.5 Haiku Fast (Direct Anthropic)", "anthropic"),

            // 🧠 Direct DeepSeek API
            ("deepseek/deepseek-chat", "DeepSeek V3 (Direct DeepSeek API)", "deepseek"),
            ("deepseek/deepseek-reasoner", "DeepSeek R1 Reasoning (DeepSeek API)", "deepseek"),

            // ⚡ Direct Groq High-Speed API
            ("groq-llama-3.3-70b", "Llama 3.3 70B Versatile (Groq 1000 tok/s)", "groq"),
            ("groq-deepseek-r1", "DeepSeek R1 Distill 70B (Groq)", "groq"),
            ("groq-qwen-coder", "Qwen 2.5 Coder 32B (Groq Fast)", "groq"),

            // 🇫🇷 Mistral & Codestral
            ("mistral/codestral-2501", "Codestral 2501 (Mistral AI Code)", "mistral"),
            ("mistral/mistral-large-2", "Mistral Large 2 (Mistral AI)", "mistral"),

            // 🔀 OpenRouter Aggregator
            ("openrouter/auto", "OpenRouter Best Router (200+ Models)", "openrouter"),
            ("openrouter/claude-3.7-sonnet", "Claude 3.7 Sonnet (OpenRouter)", "openrouter"),
            ("openrouter/deepseek-r1", "DeepSeek R1 (OpenRouter)", "openrouter"),

            // 🤝 Amazon Q & Augment (Bridge)
            ("amazon-q", "Amazon Q Developer (Bridge)", "amazon-q"),
            ("amazon-q-claude", "Claude via Amazon Q (Bridge)", "amazon-q"),
            ("augment-code", "Augment Code Agent (Bridge)", "augment"),

            // 🖥️ LM Studio (Local Port 1234)
            ("lmstudio/local-model", "LM Studio Active Model (Port 1234)", "lmstudio"),
            ("lmstudio/qwen2.5-coder", "Qwen 2.5 Coder (LM Studio)", "lmstudio"),
            ("lmstudio/deepseek-r1", "DeepSeek R1 (LM Studio)", "lmstudio"),

            // 🦙 Llama.cpp Server (Local Port 8080)
            ("llamacpp/default", "Llama.cpp Default Server (Port 8080)", "llamacpp"),
            ("llamacpp/llama-3.3", "Llama 3.3 Instruct (Llama.cpp)", "llamacpp"),

            // 🦙 Ollama Local
            ("ollama/llama3.2", "Llama 3.2 (Ollama Local)", "ollama"),
            ("ollama/deepseek-r1", "DeepSeek R1 8B/14B (Ollama Local)", "ollama"),
            ("ollama/qwen2.5-coder", "Qwen 2.5 Coder (Ollama Local)", "ollama"),
        ]
    }

    pub async fn stream_with_failover(
        &self,
        requested_model: &str,
        messages: &[ChatMessage],
        token_tx: Sender<String>,
    ) -> Result<String> {
        let mut models_to_try = vec![requested_model.to_string()];

        if self.config.auto_failover {
            for fallback in &self.config.fallback_chain {
                if fallback != requested_model && !models_to_try.contains(fallback) {
                    models_to_try.push(fallback.clone());
                }
            }
        }

        let mut last_err = anyhow!("Brak dostępnych operatorów");

        for model in &models_to_try {
            let res = self.execute_provider(model, messages, token_tx.clone()).await;
            match res {
                Ok(_) => return Ok(model.clone()),
                Err(err) => {
                    let _ = token_tx.send(format!("\n⚠️ [Failover] Błąd operatora {model}: {err}. Przełączanie na kolejny...\n")).await;
                    last_err = err;
                }
            }
        }

        Err(last_err)
    }

    async fn execute_provider(
        &self,
        model: &str,
        messages: &[ChatMessage],
        token_tx: Sender<String>,
    ) -> Result<()> {
        // ─── Antigravity IDE — gRPC-Web direct do language_server.exe ────
        // 32 modele (Gemini 3.x, Claude 4.6, GPT-OSS), wszystkie darmowe.
        // Lazy init: provider tworzony przy pierwszym użyciu.
        if model.starts_with("antigravity-") {
            let provider = self
                .antigravity
                .get_or_try_init(|| async {
                    AntigravityProvider::discover().await.map(Arc::new)
                })
                .await
                .map_err(|e: anyhow::Error| {
                    anyhow!(
                        "Antigravity IDE nie dostępne: {}.\n\
                         Uruchom Antigravity IDE i spróbuj ponownie.",
                        e
                    )
                })?;

            // Wyciągnij model ID (bez prefixu "antigravity-")
            let ag_model = model.strip_prefix("antigravity-").unwrap_or(model);
            return provider.stream_chat(ag_model, messages, token_tx).await;
        }

        // ─── Agenci-CLI jako subprocess (meta-agent delegation) ───────────
        // Każdy agent-CLI uruchamiany w trybie non-interactive (-p / --message).
        // Modele z sufiksem (np. "devin-cli-opus" lub "devin-cli/claude-opus-5-medium") przekazują model do CLI.
        if model == "devin-cli" || model.starts_with("devin-cli-") || model.starts_with("devin-cli/") {
            return self.devin_cli.stream_chat(model, messages, token_tx).await;
        }
        if model == "claude-code-cli" || model.starts_with("claude-code-cli-") {
            return self.claude_code_cli.stream_chat(model, messages, token_tx).await;
        }
        if model == "aider-cli" || model.starts_with("aider-cli-") {
            return self.aider_cli.stream_chat(model, messages, token_tx).await;
        }
        if model == "gemini-cli" || model.starts_with("gemini-cli-") {
            return self.gemini_cli.stream_chat(model, messages, token_tx).await;
        }
        if model == "codex-cli" || model.starts_with("codex-cli-") {
            return self.codex_cli.stream_chat(model, messages, token_tx).await;
        }

        // ─── Devin Cloud (api.devin.ai v3) — sesje w chmurze ──────────────
        // Wymaga DEVIN_API_KEY + DEVIN_ORG_ID. Jak brak, błąd z instrukcją.
        if model == "devin-cloud" || model.starts_with("devin-cloud-") {
            if let Some(ref cloud) = self.devin_cloud {
                return cloud.stream_chat(model, messages, token_tx).await;
            } else {
                anyhow::bail!(
                    "Devin Cloud wymaga DEVIN_API_KEY + DEVIN_ORG_ID.\n\
                     Ustaw zmienne środowiskowe lub dodaj do config.json:\n\
                     - DEVIN_API_KEY: service user key (prefix 'cog_') z app.devin.ai → Settings → Service Users\n\
                     - DEVIN_ORG_ID: organization ID (prefix 'org-') z tej samej strony\n\
                     Alternatywa: użyj modelu 'devin-cli' (lokalny Devin CLI, nie wymaga API key)."
                );
            }
        }

        // ─── ACP (Agent Client Protocol) — pełny JSON-RPC over stdio ──────
        // Bogatsza integracja niż devin-cli: streaming, plan, tool calls, thoughts.
        if model == "devin-acp" {
            return self.devin_acp.stream_chat(model, messages, token_tx).await;
        }
        if model == "devin-acp-opus" {
            return self.devin_acp_opus.stream_chat(model, messages, token_tx).await;
        }
        if model == "devin-acp-sonnet" {
            return self.devin_acp_sonnet.stream_chat(model, messages, token_tx).await;
        }
        if model == "devin-acp-codex" {
            return self.devin_acp_codex.stream_chat(model, messages, token_tx).await;
        }

        // ─── OpenCode ACP (oryginalny opencode, 131 modeli) ─────────────
        // Nie wymaga wtyczki — to CLI. Model przez env var OPENCODE_MODEL.
        if model == "opencode-acp" {
            return self.opencode_acp.stream_chat(model, messages, token_tx).await;
        }
        // Skróty opencode-zen / opencode-go → realne modele przez ACP
        if model == "opencode-zen" {
            let provider = AcpClientProvider::opencode(Some("opencode/big-pickle"), self.work_dir.clone());
            return provider.stream_chat(model, messages, token_tx).await;
        }
        if model == "opencode-go" {
            let provider = AcpClientProvider::opencode(Some("opencode-go/glm-5.3"), self.work_dir.clone());
            return provider.stream_chat(model, messages, token_tx).await;
        }
        // Dynamiczne modele z `opencode models` — format "opencode-acp/<provider>/<model>"
        // Np. "opencode-acp/opencode/ling-3.0-flash-fin-free", "opencode-acp/opencode-go/glm-5.2"
        // Np. "opencode-acp/commandcode/claude-sonnet-5", "opencode-acp/google/gemini-3.7-flash"
        if let Some(opencode_model) = model.strip_prefix("opencode-acp/") {
            let provider = AcpClientProvider::opencode(Some(opencode_model), self.work_dir.clone());
            return provider.stream_chat(model, messages, token_tx).await;
        }

        // ─── Kilo Code (fork opencode, 302 modele, 17 darmowych) ─────────
        // `kilo run -m <model>` — one-shot, bo `kilo acp` nie ma --model flag.
        if model == "kilo-run" {
            return self.kilo_run.stream_chat(model, messages, token_tx).await;
        }
        if model == "kilo-run-free" {
            // Darmowy model: nvidia/nemotron-3.5-lightning:free
            return self.kilo_run_free.stream_chat("kilo/nvidia/nemotron-3.5-lightning:free", messages, token_tx).await;
        }
        // Dynamiczne modele z `kilo models` — format "kilo-run/<model>"
        // Np. "kilo-run/kilo/anthropic/claude-sonnet-latest"
        if let Some(kilo_model) = model.strip_prefix("kilo-run/") {
            // Użyj CliSubprocessProvider z dynamicznym modelem
            let spec = crate::providers::cli_subprocess::CliSpec::kilo_with_model(kilo_model);
            let provider = crate::providers::cli_subprocess::CliSubprocessProvider::new(spec);
            return provider.stream_chat(model, messages, token_tx).await;
        }

        // ─── Cline CLI ────────────────────────────────────────────────────
        if model == "cline-cli" || model == "cline" {
            return self.cline_cli.stream_chat(model, messages, token_tx).await;
        }

        // ─── Gemini CLI ACP (darmowy tier, Google account) ────────────────
        if model == "gemini-acp" {
            return self.gemini_acp.stream_chat(model, messages, token_tx).await;
        }

        // ─── Claude Code ACP (Anthropic, Pro/Max lub API key) ─────────────
        if model == "claude-code-acp" {
            return self.claude_code_acp.stream_chat(model, messages, token_tx).await;
        }

        // ─── Codex ACP (OpenAI, wymaga API key) ───────────────────────────
        if model == "codex-acp" {
            return self.codex_acp.stream_chat(model, messages, token_tx).await;
        }

        if model == "commandcode-cli" || model == "commandcode" || model == "cmd" || model == "cmd-cli" {
            return self.commandcode.stream_chat(model, messages, token_tx).await;
        }

        if model.starts_with("commandcode-") || model.starts_with("cmd-") {
            if let Some(ref cmdcode) = self.direct_commandcode {
                let target_model = model
                    .strip_prefix("commandcode-")
                    .or_else(|| model.strip_prefix("cmd-"))
                    .unwrap_or(model);
                return cmdcode.stream_chat(target_model, messages, token_tx).await;
            }
        }

        if model.starts_with("gemini") {
            if let Some(ref gemini) = self.direct_gemini {
                return gemini.stream_chat(model, messages, token_tx).await;
            }
        }

        if model.starts_with("groq") {
            if let Some(ref groq) = self.direct_groq {
                let target_model = match model {
                    "groq-llama-3.3-70b" => "llama-3.3-70b-versatile",
                    "groq-deepseek-r1" => "deepseek-r1-distill-llama-70b",
                    "groq-qwen-coder" => "qwen-2.5-coder-32b",
                    _ => "llama-3.3-70b-versatile",
                };
                return groq.stream_chat(target_model, messages, token_tx).await;
            }
        }

        if model.starts_with("openai") {
            if let Some(ref openai) = self.direct_openai {
                let target_model = model.strip_prefix("openai/").or_else(|| model.strip_prefix("openai-")).unwrap_or(model);
                return openai.stream_chat(target_model, messages, token_tx).await;
            }
        }

        if model.starts_with("anthropic") {
            if let Some(ref anthropic) = self.direct_anthropic {
                let target_model = model.strip_prefix("anthropic/").unwrap_or(model);
                return anthropic.stream_chat(target_model, messages, token_tx).await;
            }
        }

        if model.starts_with("deepseek") {
            if let Some(ref deepseek) = self.direct_deepseek {
                let target_model = model.strip_prefix("deepseek/").unwrap_or(model);
                return deepseek.stream_chat(target_model, messages, token_tx).await;
            }
        }

        if model.starts_with("mistral") {
            if let Some(ref mistral) = self.direct_mistral {
                let target_model = model.strip_prefix("mistral/").unwrap_or(model);
                return mistral.stream_chat(target_model, messages, token_tx).await;
            }
        }

        if model.starts_with("openrouter") {
            if let Some(ref openrouter) = self.direct_openrouter {
                let target_model = model.strip_prefix("openrouter/").unwrap_or(model);
                return openrouter.stream_chat(target_model, messages, token_tx).await;
            }
        }

        // Trae alias: legacy `trae-sonnet` / `trae-claude-*` → Seed (Claude usunięty z Trae 11.2025)
        if model == "trae-sonnet" || model == "trae-claude-3-7-sonnet" || model == "trae-claude-3-5-sonnet" {
            return self.bridge.stream_chat("trae-seed-2.1-turbo", messages, token_tx).await;
        }

        // Devin to alias Windsurf (rebrand)
        if model.starts_with("devin-") {
            let aliased = model.replacen("devin-", "windsurf-", 1);
            return self.bridge.stream_chat(&aliased, messages, token_tx).await;
        }

        // Copilot / Amazon Q / Augment - zawsze Bridge (vscode.lm)
        if model.starts_with("copilot-") || model.starts_with("amazon-q") || model.starts_with("augment") {
            return self.bridge.stream_chat(model, messages, token_tx).await;
        }

        if model.starts_with("lmstudio") {
            let target_model = model.strip_prefix("lmstudio/").unwrap_or(model);
            return self.lmstudio.stream_chat(target_model, messages, token_tx).await;
        }

        if model.starts_with("llamacpp") {
            let target_model = model.strip_prefix("llamacpp/").unwrap_or(model);
            return self.llamacpp.stream_chat(target_model, messages, token_tx).await;
        }

        if model.starts_with("ollama") {
            let target_model = model.strip_prefix("ollama/").unwrap_or(model);
            return self.ollama.stream_chat(target_model, messages, token_tx).await;
        }

        // Sprawdź dynamiczne endpointy użytkownika (custom_endpoints)
        for (prefix, provider) in &self.custom_endpoints {
            if model.starts_with(prefix) {
                let target_model = model.strip_prefix(&format!("{}/", prefix)).unwrap_or(model);
                return provider.stream_chat(target_model, messages, token_tx).await;
            }
        }

        // Domyślnie używaj zunifikowanego Bridge (Cursor, Antigravity, Trae, Windsurf, OpenCode)
        self.bridge.stream_chat(model, messages, token_tx).await
    }

    /// Asynchroniczne dynamiczne wykrywanie modeli ze wszystkich aktywnych źródeł (Bridge, Ollama, LM Studio, Custom)
    pub async fn discover_models(&self) -> Vec<(String, String, String)> {
        let mut results: Vec<(String, String, String)> = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        // 0. Auto-detekcja CLI binary (opencode, devin, gemini, kilo, cline, ...)
        // Modele CLI pokazują się tylko jeśli binarka jest zainstalowana.
        let cli_map = Self::detect_cli_providers();

        // 1. Dodaj modele bazowe (filtruj CLI jeśli binarka niedostępna)
        for (id, name, prov) in self.get_available_models() {
            // Sprawdź czy to model CLI — jeśli tak, czy binarka jest dostępna
            if let Some(binary) = Self::cli_binary_for_provider(prov) {
                if !*cli_map.get(binary).unwrap_or(&false) {
                    continue; // CLI nie zainstalowane — pomiń model
                }
            }
            if seen_ids.insert(id.to_string()) {
                results.push((id.to_string(), name.to_string(), prov.to_string()));
            }
        }

        // 2. Dodaj modele z custom_endpoints
        for ep in &self.config.custom_endpoints {
            let model_id = format!("{}/{}", ep.id, ep.default_model.as_deref().unwrap_or("default"));
            if seen_ids.insert(model_id.clone()) {
                results.push((model_id, ep.name.clone(), ep.id.clone()));
            }
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(800))
            .build()
            .unwrap_or_default();

        // 3. Odpytaj mostek Bridge (/v1/models)
        let bridge_url = format!("{}/models", self.config.bridge_url.trim_end_matches('/'));
        if let Ok(resp) = client.get(&bridge_url).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
                    for item in data {
                        if let (Some(id), Some(name)) = (
                            item.get("id").and_then(|v| v.as_str()),
                            item.get("name").and_then(|v| v.as_str()),
                        ) {
                            let prov = item.get("provider").and_then(|v| v.as_str()).unwrap_or("bridge");
                            if seen_ids.insert(id.to_string()) {
                                results.push((id.to_string(), name.to_string(), prov.to_string()));
                            }
                        }
                    }
                }
            }
        }

        // 4. Odpytaj Ollama (/api/tags)
        let ollama_url = self.config.ollama_url.as_deref().unwrap_or("http://localhost:11434/v1");
        let ollama_tags_url = format!("{}/api/tags", ollama_url.trim_end_matches("/v1").trim_end_matches('/'));
        if let Ok(resp) = client.get(&ollama_tags_url).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(models) = json.get("models").and_then(|m| m.as_array()) {
                    for m in models {
                        if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
                            let id = format!("ollama/{}", name);
                            if seen_ids.insert(id.clone()) {
                                results.push((id, format!("{} (Ollama Local)", name), "ollama".to_string()));
                            }
                        }
                    }
                }
            }
        }

        // 5. Odpytaj LM Studio (/v1/models)
        let lm_url = self.config.lmstudio_url.as_deref().unwrap_or("http://localhost:1234/v1");
        let lm_models_url = format!("{}/models", lm_url.trim_end_matches('/'));
        if let Ok(resp) = client.get(&lm_models_url).send().await {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(data) = json.get("data").and_then(|d| d.as_array()) {
                    for item in data {
                        if let Some(id_str) = item.get("id").and_then(|v| v.as_str()) {
                            let id = format!("lmstudio/{}", id_str);
                            if seen_ids.insert(id.clone()) {
                                results.push((id, format!("{} (LM Studio)", id_str), "lmstudio".to_string()));
                            }
                        }
                    }
                }
            }
        }

        // 6. Odpytaj Antigravity IDE (gRPC-Web, dynamiczne wykrywanie modeli)
        // Antigravity musi być uruchomione. Modele są dodawane z prefixem "antigravity-".
        if let Ok(ag) = AntigravityProvider::discover().await {
            if let Ok(models) = ag.get_available_models().await {
                for m in models {
                    let id = format!("antigravity-{}", m.id);
                    if seen_ids.insert(id.clone()) {
                        let name = m.display_name.unwrap_or_else(|| m.id.clone());
                        let provider = m.model_provider.as_deref().unwrap_or("antigravity");
                        let provider_label = match provider {
                            "MODEL_PROVIDER_GOOGLE" => "Google",
                            "MODEL_PROVIDER_ANTHROPIC" => "Anthropic",
                            "MODEL_PROVIDER_OPENAI" => "OpenAI",
                            _ => "Antigravity",
                        };
                        results.push((
                            id,
                            format!("{} (Antigravity, {})", name, provider_label),
                            "antigravity".to_string(),
                        ));
                    }
                }
            }
        }

        // 7. Dynamiczne odkrywanie modeli z `opencode models` (127 modeli, w tym darmowe)
        // Tylko jeśli `opencode` jest na PATH. Modele dodawane jako "opencode-acp/<model>".
        if *cli_map.get("opencode").unwrap_or(&false) {
            if let Ok(output) = Self::run_cli_with_timeout("opencode", &["models"]).await {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let model_id = line.trim();
                    if model_id.is_empty() || model_id.starts_with('#') { continue; }
                    // Format: opencode/<model> lub opencode-go/<model>
                    let full_id = format!("opencode-acp/{model_id}");
                    if seen_ids.insert(full_id.clone()) {
                        // Skrócona nazwa wyświetlana — ostatnia część po /
                        let short_name = model_id.rsplit('/').next().unwrap_or(model_id);
                        let display = format!("OpenCode ACP → {short_name}");
                        results.push((full_id, display, "opencode-acp".to_string()));
                    }
                }
            }
        }

        // 8. Dynamiczne odkrywanie modeli z `kilo models` (302 modele, 17 darmowych)
        // Tylko jeśli `kilo` jest na PATH. Modele dodawane jako "kilo-run/<model>".
        if *cli_map.get("kilo").unwrap_or(&false) {
            if let Ok(output) = Self::run_cli_with_timeout("kilo", &["models"]).await {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let model_id = line.trim();
                    if model_id.is_empty() || model_id.starts_with('#') { continue; }
                    // Format: kilo/<model> — usuń prefix "kilo/" jeśli istnieje
                    let clean_model = model_id.strip_prefix("kilo/").unwrap_or(model_id);
                    let full_id = format!("kilo-run/{clean_model}");
                    if seen_ids.insert(full_id.clone()) {
                        let short_name = clean_model.rsplit('/').next().unwrap_or(clean_model);
                        let display = format!("Kilo Code → {short_name}");
                        results.push((full_id, display, "kilo-run".to_string()));
                    }
                }
            }
        }

        // 9. Dynamiczne odkrywanie modeli z `devin models list` (46 rodzin, 100+ modeli)
        // Tylko jeśli `devin` jest na PATH. Modele dodawane jako "devin-cli/<model_uid>".
        if *cli_map.get("devin").unwrap_or(&false) {
            if let Ok(output) = Self::run_cli_with_timeout("devin", &["models", "list"]).await {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                // `devin models list` może wypisywać na stdout lub stderr
                let combined = format!("{stdout}\n{stderr}");
                for line in combined.lines() {
                    let line = line.trim();
                    // Linie z modelami mają format: "  model-uid                        Display Name  [context, pricing]"
                    // Pierwsze słowo (bez spacji) to model UID
                    if line.starts_with("MODEL_") || line.starts_with("claude-") || line.starts_with("gpt-")
                        || line.starts_with("gemini-") || line.starts_with("grok-") || line.starts_with("kimi-")
                        || line.starts_with("deepseek-") || line.starts_with("glm-") || line.starts_with("inkling-")
                        || line.starts_with("penguin-") || line.starts_with("nemotron-") || line.starts_with("swe-")
                        || line.starts_with("opus") || line.starts_with("sonnet") || line.starts_with("codex")
                        || line.starts_with("haiku") || line.starts_with("gemini")
                    {
                        let model_uid = line.split_whitespace().next().unwrap_or(line);
                        if !model_uid.is_empty() {
                            let full_id = format!("devin-cli/{model_uid}");
                            if seen_ids.insert(full_id.clone()) {
                                // Wyciągnij display name (druga kolumna do "[")
                                let display_name = line.split("[")
                                    .next()
                                    .unwrap_or(line)
                                    .split_whitespace()
                                    .skip(1)
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                let display = if display_name.is_empty() {
                                    format!("Devin CLI → {model_uid}")
                                } else {
                                    format!("Devin CLI → {display_name}")
                                };
                                results.push((full_id, display, "devin-cli".to_string()));
                            }
                        }
                    }
                }
            }
        }

        results
    }

    /// Uruchamia CLI z twardym timeoutem — `opencode models` (Winget) potrafi wisieć
    /// w nieskończoność i zablokować discover_models / cargo test.
    async fn run_cli_with_timeout(bin: &str, args: &[&str]) -> Result<std::process::Output> {
        let mut cmd = tokio::process::Command::new(bin);
        cmd.args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        tokio::time::timeout(Duration::from_secs(4), cmd.output())
            .await
            .map_err(|_| anyhow!("{bin} timeout"))?
            .map_err(|e| anyhow!("{bin}: {e}"))
    }

    /// Sprawdza czy binarka CLI jest dostępna na PATH.
    /// Szybkie: używa `where.exe` (Windows) / `which` (Unix) zamiast uruchamiać proces.
    pub fn is_cli_available(binary: &str) -> bool {
        if cfg!(windows) {
            // `where.exe` — uwaga: w PowerShell `where` to alias dla Where-Object!
            std::process::Command::new("where.exe")
                .arg(binary)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .stdin(std::process::Stdio::null())
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        } else {
            std::process::Command::new("which")
                .arg(binary)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .stdin(std::process::Stdio::null())
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
    }

    /// Mapuje provider tag → binarka CLI do sprawdzenia.
    /// Zwraca None dla providerów które nie są CLI (API, bridge, etc).
    pub fn cli_binary_for_provider(provider: &str) -> Option<&'static str> {
        match provider {
            // ACP — binarka CLI
            "opencode-acp" => Some("opencode"),
            "devin-acp" => Some("devin"),
            "gemini-acp" => Some("gemini"),
            "claude-code-acp" => Some("claude-code-acp"),
            "codex-acp" => Some("codex-acp"),
            // Subprocess CLI — binarka CLI
            "kilo-run" => Some("kilo"),
            "cline-cli" => Some("cline"),
            "devin-cli" => Some("devin"),
            "claude-code-cli" => Some("claude"),
            "gemini-cli" => Some("gemini"),
            "codex-cli" => Some("codex"),
            "aider-cli" => Some("aider"),
            _ => None,
        }
    }

    /// Sprawdza dostępność wszystkich CLI binary i zwraca mapę provider→available.
    /// Wywoływane raz przy starcie (w discover_models).
    pub fn detect_cli_providers() -> std::collections::HashMap<&'static str, bool> {
        let binaries = [
            "opencode", "devin", "gemini", "kilo", "cline",
            "claude-code-acp", "codex-acp", "claude", "codex", "aider",
        ];
        let mut map = std::collections::HashMap::new();
        for bin in binaries {
            map.insert(bin, Self::is_cli_available(bin));
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    #[test]
    fn test_get_available_models_count_and_aliases() {
        let cfg = AppConfig::default();
        let router = ProviderRouter::new(cfg, std::env::temp_dir());
        let models = router.get_available_models();
        assert!(models.len() >= 80, "should have 80+ models, got {}", models.len());
        assert!(models.iter().any(|(id,_,_)| *id=="cursor-claude-3-7-sonnet"));
        assert!(models.iter().any(|(id,_,_)| *id=="trae-kimi-k2.5"));
        assert!(models.iter().any(|(id,_,_)| *id=="groq-llama-3.3-70b"));
        assert!(models.iter().any(|(id,_,_)| *id=="openrouter/auto"));
    }
    #[test]
    fn test_provider_tabs_count() {
        let tabs = crate::app::App::model_provider_tabs();
        assert!(tabs.len() >= 15);
        assert_eq!(tabs[0].1, "fav");
    }
    #[test]
    fn test_filtered_models_all() {
        let dir = std::env::temp_dir().join(format!(
            "opencode_filtered_all_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).ok();
        let mut cfg = AppConfig::for_tests();
        cfg.favorite_models = vec!["cursor-claude-3-7-sonnet".to_string()];
        let _router = ProviderRouter::new(cfg.clone(), dir.clone());
        let app = crate::app::App::new(dir.clone(), cfg);
        assert!(!app.filtered_models().is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_cli_binary_for_provider_mapping() {
        // CLI providers → binarka
        assert_eq!(ProviderRouter::cli_binary_for_provider("opencode-acp"), Some("opencode"));
        assert_eq!(ProviderRouter::cli_binary_for_provider("devin-acp"), Some("devin"));
        assert_eq!(ProviderRouter::cli_binary_for_provider("gemini-acp"), Some("gemini"));
        assert_eq!(ProviderRouter::cli_binary_for_provider("kilo-run"), Some("kilo"));
        assert_eq!(ProviderRouter::cli_binary_for_provider("cline-cli"), Some("cline"));
        // Nie-CLI providers → None
        assert_eq!(ProviderRouter::cli_binary_for_provider("gemini"), None);
        assert_eq!(ProviderRouter::cli_binary_for_provider("openai"), None);
        assert_eq!(ProviderRouter::cli_binary_for_provider("bridge"), None);
    }

    #[test]
    fn test_is_cli_available_cargo() {
        // cargo powinien być dostępny (to projekt Rust)
        assert!(ProviderRouter::is_cli_available("cargo"));
        // nieistniejąca binarka → false
        assert!(!ProviderRouter::is_cli_available("nonexistent_binary_xyz_123"));
    }
}
