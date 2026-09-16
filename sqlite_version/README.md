# ⚡ OpenCode-RS

<p align="center">
  <b>Blazing-Fast, Native Terminal AI Coding Assistant written in 100% Pure Rust 🦀</b><br>
  <i>Ultra-low memory footprint (< 15MB RAM), sub-5ms instant startup, Multi-Runtime (Host, WSL, Docker, SSH), Dual-Pane File Explorer & i18n support.</i>
</p>

<p align="center">
  <a href="https://github.com/devfortsystems/OpenCode-RS"><img src="https://img.shields.io/badge/Language-Rust-orange.svg" alt="Rust" /></a>
  <a href="https://github.com/devfortsystems/OpenCode-RS/blob/main/LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License" /></a>
  <a href="https://github.com/devfortsystems/OpenCode-RS"><img src="https://img.shields.io/badge/Platforms-Windows%20%7C%20Linux%20%7C%20macOS-green.svg" alt="Platforms" /></a>
  <a href="https://github.com/devfortsystems/OpenCode-RS/actions"><img src="https://img.shields.io/badge/Tests-31%2F31%20Passed-brightgreen.svg" alt="Tests" /></a>
  <a href="https://github.com/devfortsystems/OpenCode-RS"><img src="https://img.shields.io/github/stars/devfortsystems/OpenCode-RS?style=social" alt="Stars" /></a>
</p>

---

## ✨ Features

- **🚀 Ultra-Fast & Lightweight:** Built in pure Rust on top of Tokio and Ratatui. Uses `< 15 MB RAM` and boots in `4 ms` (compared to 300MB+ for Node/Electron CLI tools).
- **🌐 Multi-Runtime Architecture:**
  - **Host:** Run commands on your local machine.
  - **WSL:** Execute builds inside Windows Subsystem for Linux (`/env wsl`).
  - **Docker:** Execute commands inside any Docker container (`/env docker <name>`).
  - **SSH Remote Linux:** Control remote Linux servers / VPS in real-time (`/ssh user@host`).
- **📁 Dual-Pane File Explorer (`[Ctrl+E]` or `/files`):**
  - Classic dual-pane layout (Left / Right).
  - Tree View mode (`t`) with collapsible hierarchical directory trees.
  - Quick `@file` path pasting (`Space`), Copy (`F5`/`c`), Move (`F6`/`m`), Delete (`F8`/`d`).
  - Full mouse wheel and click support.
- **🌍 Full Localization & Custom Locales (i18n):**
  - Built-in languages: English (Default), Polish (`/lang pl`), Simplified Chinese (`/lang zh`), German (`/lang de`), Spanish (`/lang es`), French (`/lang fr`), Ukrainian (`/lang uk`).
  - Export & load your own JSON translations via `/lang export` and `/lang load <file.json>`.
- **🔑 Zero-Config Key Import:**
  - Automatically loads API keys and credentials from OpenCode's `auth.json`, `%APPDATA%`, `~/.config`, and `.env` files (Claude 3.7, GPT-4o, Gemini 3.7, DeepSeek V3, Groq, Ollama).
- **⏪ Checkpoints & Instant Rollback:**
  - Create in-memory project snapshots (`/checkpoint [name]`).
  - Revert multi-file modifications in one second (`/rollback`).
- **🎨 42 Modern TUI Themes (`[Ctrl+K]`):**
  - OpenCode Dark, Graphite, OLED, Catppuccin (Mocha/Frappe/Macchiato), TokyoNight, Dracula, Nord, Cyberpunk, Gruvbox, Monokai, ClassicBlue + 29 importowanych z `default-themes.ts` (OC-2, AMOLED, Aura, Ayu, Carbonfox, Cobalt2, Cursor, Everforest, Flexoki, GitHub, Kanagawa, Material, Matrix, OneDark, Rosé Pine, Solarized, Synthwave '84, Vercel, Zenburn i inne) + System.
- **📊 Real-time Cost & Token Tracker:** Live dollar cost ($) and context token usage bar.
- **📜 Automatic Architecture Rules:** Automatically reads and respects `AGENTS.md`, `CLAUDE.md`, `.opencode/rules.md`, and `.cursorrules`.

---

## ⌨️ Keyboard Shortcuts & Controls

| Shortcut | Description |
| :--- | :--- |
| **`Ctrl + P`** | **Command Palette** (Raycast / VS Code style launcher) |
| **`Ctrl + K`** | **Theme Picker** (42 visual color themes) |
| **`Ctrl + E`** | **Dual-Pane File Explorer** (File manager & tree view) |
| **`Ctrl + B`** | **Toggle Info Sidebar** (Context, tokens, cost, runtime, branch) |
| **`Ctrl + M`** | **AI Model Picker** (Switch models & providers) |
| **`Ctrl + H`** | **Session History** (Switch between chat sessions) |
| **`Ctrl + T`** | **Toggle Agent Mode** (Coder ⇄ Architect ⇄ Ask ⇄ Interactive ⇄ Auto) |
| **`PageUp / PageDown`** | Scroll chat history (with mouse wheel support) |
| **`Ctrl + L`** | Clear active chat screen |
| **`Ctrl + C`** | Save session and exit |

---

## 🚀 Quick Start

### Installation from Source
```bash
# Clone repository
git clone https://github.com/devfortsystems/OpenCode-RS.git
cd OpenCode-RS

# Build release binary
cargo build --release

# Run OpenCode-RS
./target/release/opencode-rs
```

### Installation via Cargo
```bash
cargo install --path .
opencode-rs
```

---

## 🛠️ Configuration

OpenCode-RS looks for configuration in:
- Windows: `%APPDATA%\opencode\config.json`
- Linux/macOS: `~/.config/opencode/config.json`
- Local `.env` file in your workspace

### Example `.env`:
```env
ANTHROPIC_API_KEY=sk-ant-...
OPENAI_API_KEY=sk-...
GEMINI_API_KEY=AIzaSy...
GROQ_API_KEY=gsk_...
DEEPSEEK_API_KEY=sk-...
```

---

## 📋 Zamknięte Subskrypcje AI

OpenCode-RS odblokowuje Twoje płatne subskrypcje bez dodatkowych kosztów API — przez mostek `bridge-extension` lub sterownik `subprocess`.

Pełna lista 8 operatorów, 75+ modeli i łańcuch auto-failover → **[SUBSKRYPCJE.md](SUBSKRYPCJE.md)**

| Operator | Identyfikatory (`/model`) | Połączenie |
| :--- | :--- | :--- |
| **Cursor Pro** | `cursor-claude-3-7-sonnet`, `cursor-gpt-4o`, `cursor-o3-mini` | Bridge `8765` |
| **Windsurf / Devin** | `windsurf-cascade-sonnet`, `devin-cascade-sonnet` | Bridge `8765` |
| **Trae AI** | `trae-seed-2.1-turbo`, `trae-kimi-k2.5`, `trae-minimax-m3` | Bridge `8765` |
| **Antigravity** | `antigravity-gemini-3-7-pro`, `antigravity-claude-3-7` | Bridge `8765` |
| **GitHub Copilot** | `copilot-gpt-4o`, `copilot-claude-3-7-sonnet` | Bridge `8765` |
| **CommandCode** | `commandcode`, `commandcode-claude-3-7-sonnet` | Subprocess / Direct |
| **Amazon Q / Augment** | `amazon-q`, `augment-code` | Bridge `8765` |

> Więcej: Direct API (`openai/*`, `deepseek/*`, `mistral/*`, `openrouter/*`, `groq-*`), lokale (`ollama/*`, `lmstudio/*`, `llamacpp/*`) — pełna lista `src/providers/mod.rs:156`.

---

## 🧠 Taste-1 + Skills + Interactive Modes (Cline/Roo/Trae)

Cross-client łączy OpenCode + CommandCode:

- **Taste-1** `src/taste.rs` — `taste-1` meta neuro-symbolic z `commandcode.ai/docs/taste`: czyta `.commandcode/taste/**/taste.md` + `~/.commandcode/taste` i wstrzykuje do promptu `src/agent/context.rs:111`. Komendy: `/taste`, `/taste enable|disable [--user]`, `/taste push --all`, `/taste pull <ns/pkg>`, `/taste list|lint|open` (wrapper `npx taste`).
- **Skills** `src/skills.rs` — uniwersalny loader Roo/Cline: `.roo/skills/*/SKILL.md`, `.cline/skills`, `.commandcode/skills`, `.opencode/skills`, `.clinerules/.roorules`. Podgląd `/skills`, iniekcja do system prompt `src/agent/context.rs:124`.
- **Interactive (Cline-like)** `src/app.rs:324` — `Ctrl+T` → `interactive` — inline diff + potwierdzenie przed `edit_file`/`bash_exec`. **Auto (Unattended)** → `auto` — pełna autonomia `trust_mode=ON`, 6 iteracji ReAct.
- **@terminal / @problems** `src/agent/context.rs:53` — `@terminal` wstrzykuje `.opencode/terminal.log`, `@problems` diagnostykę `cargo check` / `.opencode/problems.log` (VS Code/Cline).
- **Memories** — auto-wczyt `.windsurf/memories.md`, `.trae/memories.md`, `.roo/memories.md` `src/agent/context.rs:128`.
- **Trae BUILD** — `/build <opis>` przełącza na `auto` i buduje projekt od 0. **Preview** — `/preview` podpowiada `vite`/`next` dev server.

Wszystko w 1 TUI — testy `31 passed` `src/e2e.rs:1` + `src/taste.rs:260`.

---

## 📄 License

This project is licensed under the **MIT License** - see the [LICENSE](LICENSE) file for details.
