# AGENTS.md — OpenCode-RS

## Build & Test

```bash
cargo check                  # ~17s — szybki check
cargo build                  # pełny build, zero ostrzeżeń
cargo test                   # 163 testów, ~25s
cargo test memory::          # tylko memory (19 testów, w tym /palace)
cargo test skills::          # tylko skills (5 testów)
cargo test providers::       # tylko providers (cli_subprocess + devin_cloud + antigravity)
cargo test cost::            # tylko cost + TokenEstimator (6 testów)
cargo test tools::           # tylko ToolEngine metadata (4 testy)
cargo test app::             # tylko app.rs (15 testów: model tabs, filtered_models, App::new, cancel_streaming, timeout)
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
- `opencode-acp` / `opencode-acp-{free,go}` → `opencode acp` (oryginalny opencode v1.18.29, 127+ modeli w tym darmowe ling/mimo/nemotron — **nie wymaga wtyczki**, to CLI)
- `kilo-run` / `kilo-run-free` → `kilo run --format json -m <model>` (Kilo Code v7.5.9, fork opencode, 302 modele, 17 darmowych: nvidia nemotron, minimax, ling, poolside, stepfun, thinkingmachines — **nie wymaga wtyczki**, to CLI)
- `cline-cli` → `cline --auto-approve true -m <model>` (Cline CLI v3.0.2, ma też `--acp` ale wymaga API key)
- `gemini-acp` → `gemini --acp` (Gemini CLI v0.58.0, **darmowy tier** 60 req/min + 1000/day, wymaga `gemini` login Google account)
- `claude-code-acp` → `claude-code-acp` (Claude Code przez ACP adapter, wymaga ANTHROPIC_API_KEY lub Claude Pro/Max subscription)
- `codex-acp` → `codex-acp` (OpenAI Codex przez ACP adapter, wymaga OPENAI_API_KEY)

Routing w `providers/mod.rs` `execute_provider()` — sprawdzany przed innymi providerami.

## Wymagania — co wymaga wtyczki

Trzy kategorie modeli — **tylko pierwsza wymaga wtyczki VS Code**:

### 1. Wymaga wtyczki (Bridge Extension)

Modele z edytorów (Cursor, Windsurf, Trae, VS Code, GitHub Copilot, Amazon Q, Augment) — dostępne przez `vscode.lm` API, wymaga zainstalowania **OpenCode-RS Universal Bridge** (`bridge-extension/`).

**Instalacja wtyczki:**
1. Otwórz VS Code / Cursor / Windsurf / Trae
2. `Ctrl+Shift+P` → `Extensions: Install from VSIX...`
3. Wybierz `bridge-extension/out/extension.vsix` (lub skompiluj: `cd bridge-extension && npm install && npm run compile`)
4. Wtyczka startuje HTTP server na porcie 8765-8767 (auto-fallback)
5. Status: `$(radio-tower) OpenCode-RS: Active` w status barze edytora

**Weryfikacja:** `curl http://127.0.0.1:8765/health` → `{"status":"ok","editor":"Cursor","version":"1.18.25"}`

**Modele wymagające wtyczki:**
- `opencode-*` (OpenCode Native — Zen, Go, Flash, Pro, Claude, GPT, Gemini)
- `cursor-*` (Cursor Pro — Claude, GPT, DeepSeek)
- `windsurf-*` / `devin-cascade-*` (Windsurf Cascade)
- `trae-*` (Trae AI — Seed, Kimi, MiniMax, Gemini, GPT)
- `copilot-*` (GitHub Copilot)
- `amazon-q*` (Amazon Q Developer)
- `augment-*` (Augment Code)
- `commandcode-*` (Command Code — modele bridge, nie `commandcode-cli`)

**Wymaga:** edytor z wtyczką uruchomiony w tle (nie musi być aktywny, wtyczka działa po starcie).

### 2. Wymaga CLI zainstalowanego (bez wtyczki)

Modele z agentów-CLI — **nie wymagają wtyczki**, tylko binarka na PATH. Auto-detekcja: `where.exe`/`which` sprawdza dostępność, model pokazuje się tylko jeśli binarka istnieje.

| Model | CLI | Instalacja | Wymaga |
|---|---|---|---|
| `devin-cli*` | `devin` | `npm i -g @devin/cli` | login devin.ai |
| `devin-acp*` | `devin` | `npm i -g @devin/cli` | login devin.ai |
| `claude-code-cli*` | `claude` | `npm i -g @anthropic/claude-code` | ANTHROPIC_API_KEY lub Claude Pro/Max |
| `claude-code-acp` | `claude-code-acp` | `npm i -g @anthropic/claude-code-acp` | ANTHROPIC_API_KEY lub Claude Pro/Max |
| `aider-cli` | `aider` | `pip install aider-chat` | klucz API modelu |
| `gemini-cli` | `gemini` | `npm i -g @google/gemini-cli` | `gemini` login (Google account) |
| `gemini-acp` | `gemini` | `npm i -g @google/gemini-cli` | `gemini` login (darmowy tier 60 req/min) |
| `codex-cli` | `codex` | `npm i -g @openai/codex` | OPENAI_API_KEY |
| `codex-acp` | `codex-acp` | `npm i -g @openai/codex-acp` | OPENAI_API_KEY |
| `opencode-acp*` | `opencode` | `npm i -g opencode-ai@latest` | `opencode auth` (darmowe ling/mimo/nemotron) |
| `kilo-run*` | `kilo` | `npm i -g kilo-ai@latest` | `kilo auth` (17 darmowych modeli) |
| `cline-cli` | `cline` | `npm i -g @cline/cli` | `cline auth` (API key) |

**Wymaga:** tylko binarka na PATH. Lazy start — CLI uruchamia się przy pierwszym prompcie.

### 3. Wymaga API key lub lokalnego serwera (bez wtyczki, bez CLI)

| Model | Co | Wymaga |
|---|---|---|
| `gemini-3.7-*` | Direct Gemini API | `GEMINI_API_KEY` w `.env`/`auth.json` |
| `openai/*` | Direct OpenAI API | `OPENAI_API_KEY` |
| `anthropic/*` | Direct Anthropic API | `ANTHROPIC_API_KEY` |
| `deepseek/*` | Direct DeepSeek API | `DEEPSEEK_API_KEY` |
| `groq-*` | Direct Groq API | `GROQ_API_KEY` |
| `mistral/*` | Direct Mistral API | `MISTRAAL_API_KEY` |
| `openrouter/*` | OpenRouter aggregator | `OPENROUTER_API_KEY` |
| `devin-cloud*` | Devin Cloud (api.devin.ai v3) | `DEVIN_API_KEY` + `DEVIN_ORG_ID` |
| `ollama/*` | Ollama local | Ollama running na `localhost:11434` |
| `lmstudio/*` | LM Studio local | LM Studio running na `localhost:1234` |
| `llamacpp/*` | Llama.cpp server | server running na `localhost:8080` |
| `antigravity-*` | Antigravity IDE | Antigravity IDE uruchomione (gRPC-Web direct, **darmowe**) |

**Wymaga:** klucz w `.env` lub `~/.opencode/auth.json`, albo lokalny serwer running.

### Szybki test

```bash
opencode doctor    # sprawdza: bridge, klucze API, CLI binary, lokalne serwery
opencode models    # lista modeli + status (✅ OK / ⚪ NO_KEY / ⚪ BRIDGE / ❌ FAIL)
```

`opencode doctor` pokazuje dokładnie co jest dostępne i czego brakuje.

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
- [x] Fix UI freeze — Esc anuluje streaming, auto-recovery (timeout 90s idle / 5 min total), Ctrl+C zawsze działa, AbortHandle zabija task agenta
- [x] Auto-detekcja CLI — `where.exe`/`which` sprawdza binarki (opencode, devin, gemini, kilo, cline, claude-code-acp, codex-acp); modele CLI pokazują się tylko jeśli binarka zainstalowana; lazy start przy pierwszym prompcie
- [x] Dynamiczne odkrywanie modeli — `opencode models` (127 modeli) + `kilo models` (302 modele) uruchamiane w tle w `discover_models()`; modele dodawane jako `opencode-acp/<model>` i `kilo-run/<model>`
- [x] `src/opencode_compat.rs` — pełna kompatybilność opencode v1.18.21 + CommandCode v1.x: loader `opencode.json`/`opencode.jsonc` (merge global+project), `tui.json` (keybinds), `.opencode/agents/*.md` + `.commandcode/agents/*.md` (frontmatter: mode, model, prompt, temperature, permission, hidden), `.opencode/commands/*.md` + `.commandcode/commands/*.md` ($ARGUMENTS, $1-$9, !`cmd`, @file), `.opencode/plugins/*.js|ts` + `.commandcode/mods/*.ts` (loader + node subprocess bridge), formatters (built-in: rustfmt/gofmt/prettier/black/clang-format + custom z configa), LSP servers (config + extensions), permissions (ask/allow/deny per tool)
- [x] Komendy TUI: `/agents` `/agent <nazwa>` `/commands` `/mods` `/plugins` `/compat` `/keybinds` `/format <plik>` + custom komendy z `.opencode/commands/*.md` i `.commandcode/commands/*.md` (auto-wykrywanie w `handle_command _ =>`)
- [x] CommandCode Mods — `.commandcode/mods/*.ts` loader + `execute_mod()` (node subprocess z ModApi shim przez env vars: `COMMANDCODE_MOD`, `COMMANDCODE_MOD_EVENT`, `COMMANDCODE_MOD_PAYLOAD`, `COMMANDCODE_MOD_CWD`)
- [x] OpenCode Plugins — `.opencode/plugins/*.js|ts` loader + `execute_plugin()` (node subprocess z opencode API shim przez env vars: `OPENCODE_PLUGIN`, `OPENCODE_PLUGIN_HOOK`, `OPENCODE_PLUGIN_PAYLOAD`, `OPENCODE_PLUGIN_CWD`)
- [x] 16 nowych testów — `opencode_compat::tests::*` (strip_json_comments, split_frontmatter, parse_agent_markdown, parse_command_markdown, render_command_template, check_permission, opencode_config_parse, load_agents_from_markdown, load_commands_from_markdown, commandcode_mods_loading, load_empty_workdir) — łącznie 188 testów

### Do zrobienia

### 1. Push na origin
- Commity lokalne, nie pushowane
- **Koszt:** tryvialny (ale wymaga zgody — nie pushować bez pytania)
