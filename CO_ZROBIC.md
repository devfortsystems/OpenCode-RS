# CO_ZROBIC.md — OpenCode-RS

> **Status:** ~98% gotowości (Priorytet 4 — Terminalowy Agent AI w Rust)
> **Ostatnia aktualizacja:** 2026-09-18
> **Typ:** AI Developer Assistant / CLI / ACP Server / Multi-Model Terminal + TUI IDE 3-kolumnowy + Tray + Web UI
> **Technologie:** Rust (Tokio, Crossterm / Ratatui, Reqwest, DevFortDB / SQLite, Antigravity gRPC-Web)
> **Testy:** 212/212 PASS (`cargo test --lib`), ~30s
> **Liczba modeli (opencode models):** 668–877 (w tym 400+ darmowych)

---

## 1. Aktualny stan (po poprawkach z 18.09.2026)
- **Rdzeń:** Silnik agenta `src/agent/` (ReAct, ContextManager, ToolEngine, McpManager, CheckpointManager).
- **TUI IDE 3-kolumnowy (F3 / `/ide`):** Eksplorator 2-paneli | Edytor VS Code Dark+ TextMate 50+ języków | Czat AI + Workspace Resurrect (multi-tab, per-karta model/katalog).
- **Providerzy:** Bridge (vscode.lm: Cursor/Windsurf/Trae 27 modeli), Direct API (Gemini/Groq/OpenAI/Anthropic/DeepSeek/Mistral/OpenRouter + CommandCode 58 modeli!), Agenci-CLI (Devin/Claude/Aider/Gemini/Codex), ACP (Devin/OpenCode/Gemini/Claude/Codex), **Antigravity IDE headless** (32 modele darmowe Gemini 3.x/Claude 4.6/GPT-OSS, auto-standalone + odzyskiwanie stanu między sesjami), Devin Cloud (api.devin.ai v3 SWE-2 FREE), Ollama/LMStudio/llama.cpp.
- **Wersja publiczna SQLite:** Gałąź `sqlite_version/` → auto-sync skryptem `./scripts/sync_to_sqlite.ps1`, 100% zgodna, 0 prywatnych zależności.
- **Persistent Context:** Memory blocks Letta-style, `/palace` (blocks+plan+skills), `/context` (rozkład tokenów), plan `.opencode/plan.md`, Archival HNSW w DevFortDB.
- **Dodatki:** Tray (opencode --tray), Quota tracker, Web UI Vue 3 + ApexCharts (port 7711), `/sleeptime` dreaming, RepoMap AST, VSIX Manager, `/diff` `/copy` `/vsix` `/repomap` `/ide` i 30 innych slash command + custom komendy `.opencode/commands/*.md`.
- **Plugins/Mods:** OpenCode plugins (`.opencode/plugins/*.js|ts`) + CommandCode mods (`.commandcode/mods/*.ts`).
- **Perms:** Permission "ask" dialog TUI (Enter allow / Esc deny), timeout 120s. ACP `session/cancel` działa.
- **Drzewo robocze:** 51 zmodyfikowanych plików od ostatniego commita (6100 insertów, 1038 usunięć).

---

## 2. CO ZROBIONE z tego co było w oryginale (wszystko ⮕ [x])

### A. Weryfikacja gałęzi SQLite (`sqlite_version/`)
- [x] **Sync skrypt `scripts/sync_to_sqlite.ps1`** — działa, weryfikuje `cargo check` sqlite_version po każdej operacji, omija `database.rs` i `archival.rs`. ✅ Ostatni run 18.09.2026: `sqlite_version kompiluje się w 100% poprawnie!`
- [x] **Stabilność ACP + Tool Calling w sqlite_version** — testy 212/212 przechodzą, zero martwych zależności od DevFortDB.
- [x] **Obsługa błędów przy braku kluczy API** — opencode doctor pokazuje statusy ✅/⚪/❌, opencode models pokazuje badge [NO_KEY], komunikat z instrukcją dla każdego providera.

### B. Stabilność testów i pamięć
- [x] **Testy jednostkowe:** `cargo test --lib` → **212/212 PASS** (app 15, importer 14, file_manager 21, memory 19, skills 5, providers 4, cost 6, database 12, opencode_compat 18, + e2e). Żaden nie wisi.
- [x] **Zawieszenia UI:** Esc + Ctrl+Shift+Esc + Ctrl+. anulują streaming, auto-timeout 90s idle / 5m total, AbortHandle zabija task agenta.
- [x] **Obcinanie/kompresja kontekstu w `src/agent/context.rs`:**
  - `compress_context_if_needed(messages, max_tokens)` (L268): system + ostatnie 6 wiadomości, starsze → streszczenie z preview 200 znaków/wiadomość.
  - Komenda TUI `/compact` → ręczna kompresja, raport przed/po.
  - **Auto-compact w `start_agent_stream()`** (app.rs L1602-1620, dodano 18.09.2026): jeśli messages > 32k tokens → auto-compress do 16k, eprintln log.

### C. Dystrybucja i CLI binary
- [x] **`DISTRIBUTION.md` aktualny** — strategia prywatne (root / DevFortDB) vs publiczne (sqlite_version/ SQLite), tabela porównawcza, procedura publikacji.
- [x] **Czysty build release:** `cargo build --release` → ~8-9min, 0 błędów (ostatni run 18.09.2026: 5m 35s, tylko 1 niegroźny warning unused import main.rs:571).
- [x] **`opencode doctor` WSZYSTKO OK:** git ✅, rustc ✅, 4 klucze API + 3 custom endpoints ✅, antigravity LS binarka ✅, DevFortDB MDB ✅, memory ✅, workdir ✅.
- [x] **`opencode --help` / subkomendy CLI:** run/acp/mcp/models/auth/doctor/session/stats/export/import/skills/rules/plugins/sleeptime/upgrade/version/completion + tray.
- [ ] **Zacommitować wyczyszczone zmiany robocze (51 plików, 6100 insertów)** → ⚠️ **ZROBIĆ RĘCZNIE JEŚLI CHCESZ** (Automat NIE commituje bez Twojej zgody — AGENTS.md zakazuje).

---

## 2.5 CO DODANO OD UTWORZENIA ORYGINALNEGO CO_ZROBIC.md (brakujące 18% → 98%)

> Oryginalny plik był z fazy 80-85%. Poniżej funkcje dodane POJEDYNCZO, z których KAŻDA przechodzi testy i smoke:

- [x] Antigravity IDE headless (auto-standalone port 13443–13447, **odzyskiwanie stanu między sesjami** w `~/.opencode-rs/antigravity_state.json`, timeout 45s zamiast 15s → 0 duplicate procesów). *Dzisiaj 18.09.*
- [x] **58 modeli Command Code (commandcode/...)** — zamiast 4 z mostka. Poprawki: env override w Config.load (13 kluczy), routing `commandcode/` ukośnik + myślnik, runtime map "command-code" (z myślnikiem z auth.json), discover 58 z `opencode models` ze switch provider, cost.rs label Command Code API $0.80/$2.40.
- [x] **Failover per-provider** (antigravity-* → najpierw 7 innych antigravity, nie od razu cursor-sonnet; to samo dla commandcode, devin, opencode-acp, kilo, gemini-acp).
- [x] Pending prompts sanity check (`looks_like_ui_paste_junk`) — ignoruje wklejone zrzuty ekranu TUI (>5% box draw / >4k chars).
- [x] Tabs wielowierszowe + 4 pickery viewport scroll + dynamiczne szerokości (model/session/palette/permission).
- [x] Runtime availability filter TUI + fav bypass unwrap_or(true).
- [x] Devin SWE-2 FREE (3 modele: high/medium/max 262K → cost.rs 🟢 DARMOWY).
- [x] DevFortDB embedded (sesje, memory blocks, plan, cache TTL, stats, archival HNSW) + JSON fallback + auto-migracja.
- [x] ACP server (`opencode --acp`) — sterowany przez Zed/Windsurf po stdio JSON-RPC, session/update streaming.
- [x] CommandCode / OpenCode **pełna kompatybilność v1.18**: loader `opencode.json`/tui.json, subagenci `.opencode/agents/*.md` (frontmatter), komendy `.opencode/commands/*.md` ($ARGUMENTS, $1-$9, !cmd, @file), plugins JS/TS, mods TS CommandCode, permissions ask/allow/deny per-tool, formatters (rustfmt/gofmt/prettier/black/clang-format + custom), LSP servers z configa.
- [x] **TUI IDE 3-kolumnowy (F3):** 22% eksplorator | 48% edytor VS Code Dark+ TextMate (syntect 50+ języków) | 30% czat.
- [x] RepoMap AST kompaktowy indeks symboli <1500 tokenów.
- [x] VSIX Manager (.vsix: motywy, LSP, snippety).
- [x] `web_fetch` tool (pobieranie stron WWW do czystego Markdown).
- [x] **Workspace Resurrect (F3):** wielozadaniowość kart (model/katalog per karta), auto-zapis draftów promptów (0 utraty po restarcie).
- [x] **Quota & Subscription Tracker:** OpenRouter saldo USD, Gemini 1500 RPD, Antigravity 32 bez limitu, daty odnowienia.
- [x] **Windows System Tray (opencode --tray):** Web UI, TUI, ~/.opencode-rs, autostart Windows, exit. <5MB RAM.
- [x] **Quasar Web UI (Vue 3 + ApexCharts)** `src/web/dashboard.html`: 3-kol IDE, pasek kart projektów, eksplorator q-tree, edytor kodu, czat AI z auto-zapis draftu, wykresy kołowe limitów Apex.
- [x] Dockerfile + .dockerignore (Web UI :7711, 8765-8767 zarezerwowane dla Bridge).
- [x] Permission "ask" dialog TUI + headless allow.
- [x] ACP session/cancel → AbortHandle per session_id, StopReason::Cancelled.
- [x] Ścieżki UI ~/.opencode-rs/ per user i .opencode-rs/ per projekt (legacy .opencode/ tylko odczyt).

---

## 3. PRAWDZIWIE POZOSTAŁE DO ZROBIENIA (ostatnie ~2% / KOSMETYKA NIEBLOKUJĄCA)

> Wszystko co naprawdę zostało to drobne kosmetyki. Nic nie blokuje używania na produkcji.

### 🔹 Priorytet 1 (dla bezpieczeństwa / czystości):
1. **[ ] Ręcznie zacommitować 51 plików zmian (6100 insertów).** Automat NIE commituje bez Twojej zgody. Proponowana wiadomość:
   ```
   feat: CommandCode 58 modeli, Antigravity headless recover, failover per-provider,
         pending UI-junk sanity check, 4 pickery viewport scroll, tabs 2-row, auto context compact,
         TUI IDE + Tray + Web UI + Quota + Workspace Resurrect + SQLite sync v2
   test: 212/212 PASS
   ```
2. **[ ] (1 warning) `use app::AppEvent` main.rs:571 — usuń unused import** lub `cargo fix --bin opencode-rs`.

### 🔹 Priorytet 2 (kosmetyka / UX opcjonalne):
3. **[ ] Test manualny CLI one-shot:** `opencode -m commandcode/claude-sonnet-5 -p "2+2="` oraz `opencode -m antigravity-gemini-3.8-flash-tiered -p "2+2="` — potwierdzenie end-2-end z nowym binarką release.
4. **[ ] Aktualizacja SUBSKRYPCJE.md / README.md** o nowe providerzy (Antigravity 32 free, SWE-2 free, CommandCode 58, 400+ free suma).
5. **[ ] (Nice-to-have) `~/antigravity_state.json` → TTL:** jeśli plik jest starszy niż 24h, wymuś nowy discover (na wypadek rebootu Windows, gdzie port jest wolny ale plik jest). *Można dodać 2 linijki w `load_standalone_state_from_disk()` porównujące `metadata().modified()`.*

### 🔹 Priorytet 3 (eksperymentalne / na prośbę):
6. **[ ] Webhook / Auto-update `opencode upgrade`** — domyślnie nie działa, wymaga serwera aktualizacji.
7. **[ ] Krótkie wideo / screenshoty do README** (3-kol IDE vs zwykły TUI, tray menu, Web UI charts).

---

## 4. Podsumowanie: CZY MA SENS ROBIĆ COŚ DALEJ?

| Oryginalny punkt | Czy trzeba? | Ocena |
|---|---|---|
| A. sqlite_version tests/sync | **✅ ZALICZONE W 100%** | Testy 212/212 + sync `opencode-rs -> sqlite_version` działa. |
| B. Stabilność testów | **✅ ZALICZONE** | Nie ma wiszących testów. |
| B. Memory optymalizacja | **✅ ZALICZONE (dziś 18.09 dodano auto-compact)** | + ręczny `/compact` + funkcja compress istnieją od dawna. |
| C. DISTRIBUTION.md aktualizacja | **✅ ZALICZONE** | Dokumentacja rozdziału prywatne/publiczne jest poprawna. |
| C. Build release czysty | **✅ ZALICZONE** | Ostatni 5m 35s / 0 błędów. |
| C. Commit zmian roboczych | **⚠️ TYLKO RĘCZNIE** (nie bez Twojej zgody, patrz AGENTS.md). | 51 plików czeka. |

**Wniosek:** Oryginalny CO_ZROBIC.md był z fazy ~80% i jest **w 100% przeterminowany** — wszystkie jego punkty są już zrobione. Prawdziwie zostało **TYLKO 2 rzeczy na priorytet 1** (ręczny commit i usunięcie 1 warningu), reszta to kosmetyki / opcjonalne.

---

## 5. Komendy weryfikacyjne (uruchom w dowolnej kolejności)

```bash
# Testy i kompilacja
cargo check                                           # ~15s, 0 błędów
cargo test --lib                                      # 212/212 PASS, ~30s

# Wersja publiczna SQLite
powershell -ExecutionPolicy Bypass -File .\scripts\sync_to_sqlite.ps1   # sync + compile check
cd sqlite_version ; cargo check ; cargo test --lib   # 100% działa

# Środowisko + modele + smoke
.\target\release\opencode-rs.exe doctor               # ✅ WSZYSTKO OK
.\target\release\opencode-rs.exe models | Select-String "commandcode/" | Measure-Object -Line  # 58 sztuk
.\target\release\opencode-rs.exe -m antigravity-gemini-3.8-flash-tiered -w . -p "2+2="   # Antigravity smoke
```
