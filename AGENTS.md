# AGENTS.md — OpenCode-RS

## Build & Test

```bash
cargo check                  # ~17s — szybki check
cargo build                  # pełny build, zero ostrzeżeń
cargo test                   # 164 testów, ~80s
cargo test memory::          # tylko memory (19 testów, w tym /palace)
cargo test skills::          # tylko skills (5 testów)
cargo test providers::       # tylko providers (cli_subprocess + devin_cloud + antigravity)
cargo test cost::            # tylko cost + TokenEstimator (6 testów)
cargo test tools::           # tylko ToolEngine metadata (4 testy)
cargo test app::             # tylko app.rs (11 testów: model tabs, filtered_models, App::new)
cargo test importer::        # tylko importer.rs (14 testów: strip_json_comments, apply_env_key)
cargo test file_manager::    # tylko file_manager.rs (21 testów: file_color, PaneState, navigation)
cargo test database::        # tylko DevFortDB embedded (12 testów: JSON, TTL, scan, increment, sessions)
cargo build --release        # release binary
```

Platforma: Windows + PowerShell (uwaga: `&&` nie działa, używaj `;`).

## Architektura (krótko)

- `src/providers/` — `ProviderRouter` + `BridgeProvider` (HTTP proxy do vscode.lm edytorów) + `DirectApiProvider` (OpenAI-compat) + `SubprocessProvider` (commandcode-cli) + `CliSubprocessProvider` (Devin/Claude Code/Aider/Gemini/Codex CLI) + `DevinCloudProvider` (api.devin.ai v3, sesje w chmurze) + `AcpClientProvider` (Agent Client Protocol, JSON-RPC over stdio z `devin acp` / `gemini --acp` / `claude-code-acp` / `codex-acp` / `opencode acp`)
- `src/acp_server.rs` — **ACP server** (odwrotna rola: opencode-rs jako agent sterowany przez edytory Zed/Windsurf przez `opencode --acp`, stdio JSON-RPC, streaming przez `session/update` notifications)
- `src/database.rs` — **DevFortDB embedded** (wbudowana baza MDBX: sesje, memory blocks, plan, cache z TTL, stats — namespace izolacja, JSON API, 256 MiB, ACID)
- `vendor/signet-mdbx-sys/` — vendored MDBX sys bindings (Windows fix — puste bindings_windows.rs w upstream)
- `src/agent/` — `Agent` (ReAct loop), `ContextManager` (system prompt), `ToolEngine` (read/edit/write/bash/grep — z metadanymi rozmiaru/tokenów), `McpManager`, `CheckpointManager`
- `src/app.rs` (120KB) — TUI, komendy slash, event loop, `AppEvent::ContextUpdate(chars, tokens)` — realny tracking contextu
- `src/memory.rs` — Letta-style memory blocks (persona/human/project) + `ProjectPlan` (persistentny plan per-projekt `.opencode/plan.md`) + `/palace` (drzewiasty podgląd: blocks + plan + skills) + `/doctor` (audyt)
- `src/skills.rs` — loader skilli (roo/cline/opencode/commandcode/learned)
- `src/cost.rs` — `CostEstimator` (koszt USD per model) + `TokenEstimator` (BPE heurystyka: kod 3.2, proza 4.0, JSON 3.5 chars/token; file-size estimation per-extension)
- `bridge-extension/` — VS Code extension (HTTP server na 8765-8767, proxy vscode.lm)

## Estymacja tokenów — jak działa

`TokenEstimator` w `src/cost.rs` — lepsza niż `chars / 4`:

- **Heurystyka kod vs tekst** (`estimate(content)`): liczy ratio symboli `{}()[];=<>` — jeśli >25% to kod (3.2 chars/token), <8% to proza (4.0), middle (3.6)
- **File-size estimation** (`estimate_from_file_size(path, bytes)`): per-extension — `.rs/.ts/.py` → 3.2, `.json/.yaml` → 3.5, `.md/.txt` → 4.0; `bytes * 0.85 / chars_per_token` (pokrywa UTF-8)
- **Realny context tracking**: `AppEvent::ContextUpdate(chars, tokens)` wysyłany po każdej iteracji ReAct — UI pokazuje pełne tool outputs (nie ucięte do 4000 znaków) + system prompt
- **Metadane w tool results**: `read_file` zwraca header `📄 path (bytes, linie) | ~tokeny`, `edit_file` pokazuje `Δ tokenów`, `write_file` pokazuje rozmiar+tokeny

Stawki kosztów (`estimate_cost_from_tokens`): Claude Opus/Sonnet $3, Haiku $0.80, GPT-4o $2.50, GPT-mini $0.15, Gemini $0.10, DeepSeek $0.55, Qwen/Llama $0.20, Kilo/OpenCode/Devin $0 (darmowe/subskrypcja), Ollama/LMStudio/llama.cpp $0 (lokalne).

## Modele — integracja agentów-CLI i chmury

opencode-rs jako **meta-agent** — deleguje zadania do innych agentów-CLI i chmury:

- `devin-cli` / `devin-cli-{opus,sonnet,codex}` → `devin -p` (lokalny Devin CLI, tryb non-interactive)
- `claude-code-cli` / `claude-code-cli-{sonnet,opus}` → `claude -p` (Claude Code CLI)
- `aider-cli` → `aider --message` (Aider)
- `gemini-cli` → `gemini -p` (Gemini CLI)
- `codex-cli` → `codex` (Codex CLI)
- `devin-cloud` / `devin-cloud-{fast,lite,ultra,fusion}` → api.devin.ai v3 (sesje w chmurze, wymaga `DEVIN_API_KEY` + `DEVIN_ORG_ID`)
- `devin-acp` / `devin-acp-{opus,sonnet,codex}` → `devin acp` (Agent Client Protocol, JSON-RPC over stdio — streaming, plan, tool calls, thoughts, auto-approve permissions)
- `opencode-acp` / `opencode-acp-{free,go}` → `opencode acp` (oryginalny opencode v1.18.21, 127 modeli w tym darmowe ling/mimo/nemotron — **nie wymaga wtyczki**, to CLI)
- `kilo-run` / `kilo-run-free` → `kilo run --format json -m <model>` (Kilo Code v7.5.9, fork opencode, 302 modele, 17 darmowych: nvidia nemotron, minimax, ling, poolside, stepfun, thinkingmachines — **nie wymaga wtyczki**, to CLI)
- `cline-cli` → `cline --auto-approve true -m <model>` (Cline CLI v3.0.2, ma też `--acp` ale wymaga API key)
- `gemini-acp` → `gemini --acp` (Gemini CLI v0.58.0, **darmowy tier** 60 req/min + 1000/day, wymaga `gemini` login Google account)
- `claude-code-acp` → `claude-code-acp` (Claude Code przez ACP adapter, wymaga ANTHROPIC_API_KEY lub Claude Pro/Max subscription)
- `codex-acp` → `codex-acp` (OpenAI Codex przez ACP adapter, wymaga OPENAI_API_KEY)

Routing w `providers/mod.rs` `execute_provider()` — sprawdzany przed innymi providerami.

### 3 sposoby integracji z Devinem

1. **Devin CLI (subprocess)** — `devin -p "prompt"` — one-shot, non-interactive, lokalnie
2. **Devin Cloud (REST API)** — `api.devin.ai v3` — sesje w chmurze, streaming przez polling
3. **Devin ACP (JSON-RPC over stdio)** — `devin acp` — pełny protokół: streaming, plan, tool calls, thoughts, auto-approve permissions

Wszystkie 3 sposoby wstrzykiwane są planem projektu + memory blocks przez `build_context_aware_prompt()` — dzięki temu "wspólny czat" jest wspólny dla każdego AI, nie tylko modeli "własnego" agenta.

## Persistentny kontekst (wspólny dla wszystkich modeli)

| Co | Gdzie | Widoczne dla |
|---|---|---|
| Memory blocks (persona/human/project) | `~/.opencode/memory/*.md` + `.opencode/memory/project.md` | "własne" modele (system prompt) + delegaci (wstrzykiwane w prompt) |
| Plan projektu | `.opencode/plan.md` | "własne" modele (system prompt) + delegaci (wstrzykiwane w prompt) |
| Ostatnia sesja | `SessionManager` JSON | auto-ładowana przy starcie |
| Skills | `.opencode/skills/` | "własne" modele (system prompt) |

Plan i memory blocks są wstrzykiwane w prompt delegatów (Devin ACP/Cloud) przez `build_context_aware_prompt()` — dzięki temu "wspólny czat" jest wspólny dla każdego AI, nie tylko modeli "własnego" agenta.

## Konwencje kodu

- Komentarze po polsku (jak w reszcie kodu)
- Testy inline w modułach (`#[cfg(test)] mod tests`)
- Temp dir w testach: `std::env::temp_dir().join(format!("opencode_{}_{}", uuid::Uuid::new_v4()))`
- Komendy slash w `app.rs` `handle_command()` — match na `parts[0]`
- Tools agenta w `agent/mod.rs` `execute_tool_call()` — match na `tool_name`
- System prompt w `agent/context.rs` `build_system_prompt()` — format! z sekcjami

## Backlog

### Zrobione

- [x] Usunąć śmieć `cargo` — pusty plik 0B usunięty z repo root
- [x] `/palace` — podgląd pełnego stanu pamięci (blocks + plan + skills)
- [x] Agenci-CLI jako subprocess (Devin/Claude Code/Aider/Gemini/Codex) — CliSubprocessProvider + routing
- [x] Devin Cloud REST API — sesje w chmurze przez api.devin.ai v3
- [x] Pełny ACP client — JSON-RPC over stdio dla `devin acp` (streaming + plan + tool calls + thoughts + auto-approve)
- [x] Persistent plan per projekt — `.opencode/plan.md` + tools plan_set/plan_add_step/... + `/plan` komenda + wstrzykiwanie w system prompt
- [x] Wstrzykiwanie planu + memory w prompt delegatów (Devin ACP/Cloud) — build_context_aware_prompt
- [x] opencode-acp — oryginalny opencode jako ACP (127 modeli, darmowe, bez wtyczki)
- [x] kilo-run — Kilo Code jako subprocess (302 modele, 17 darmowych, bez wtyczki)
- [x] cline-cli — Cline CLI jako subprocess (auto-approve)
- [x] gemini-acp — Gemini CLI ACP (darmowy tier 60 req/min, Google login)
- [x] claude-code-acp — Claude Code ACP adapter (Anthropic)
- [x] codex-acp — OpenAI Codex ACP adapter
- [x] Bug fix: prompt nie był przekazywany jako arg w CliSubprocessProvider (prompt_via_stdin=false)
- [x] Bug fix: Windows .cmd shims wymagają cmd /c w CliSubprocessProvider
- [x] Estymacja tokenów — TokenEstimator (BPE heurystyka kod vs tekst + file-size per-extension) + realny context tracking (AppEvent::ContextUpdate(chars, tokens)) + metadane w tool results (read_file/edit_file/write_file)
- [x] Nowy operator bridge — Gemini CLI, Claude Code, Codex przez ACP (nie wymaga wtyczki)
- [x] Antigravity IDE reverse engineering — gRPC-Web protocol cracked, 32 models discovered, API documented in `ANTIGRAVITY_API.md`
- [x] Antigravity provider — `src/providers/antigravity.rs` (gRPC-Web direct do language_server.exe, 32 modele: Gemini 3.x, Claude 4.6, GPT-OSS — wszystkie darmowe)
- [x] `/sleeptime` dreaming — `opencode --sleeptime` (jednorazowo) lub `opencode --sleeptime 300` (cyklicznie co 5 min) — refleksja nad pamięcią + planem, log do `.opencode/sleeptime_log.md`
- [x] Testy coverage — app.rs (11 testów), importer.rs (14 testów), file_manager.rs (21 testów) — łącznie 46 nowych testów
- [x] ACP jako agent — `opencode --acp` (stdio JSON-RPC server, opencode-rs sterowany przez Zed/Windsurf)
- [x] DevFortDB embedded — `src/database.rs` (wbudowana baza MDBX, ACID, TTL, JSON, HNSW — sesje/memory/plan/cache/stats w jednej bazie)
- [x] Subkomendy CLI — `opencode run/acp/mcp/models/auth/doctor/session/stats/export/import/skills/rules/plugins/sleeptime/upgrade/version/completion` (łącząc opencode + devin + commandcode)
- [x] Sesje w DevFortDB — `SessionManager` używa DB primary, JSON fallback + auto-migracja
- [x] Archival memory (HNSW) — `src/archival.rs` (Grafowektor + hash/Ollama/OpenAI embedding, tools `archival_search`/`archival_add`/`archival_list`)
- [x] `/context` — podgląd zużycia context window (rozkład tokenów: system prompt, memory, plan, skills, archival, historia)

### Do zrobienia

### 1. Push na origin
- Commity lokalne, nie pushowane
- **Koszt:** tryvialny (ale wymaga zgody — nie pushować bez pytania)
