# Naprawy Jakości OpenCode-RS — Plan Implementacji

## Badania Repozytorium — Ustalenia

### 1. Permission "ask" = allow (brak dialogu)
**Lokalizacja:** [agent/mod.rs:60-77](file:///c:/projekt/opencode-rs/src/agent/mod.rs#L60-L77)

**Bug:** Gdy `self.permissions` jest `None` (brak pliku `opencode.json`/`commandcode.json` z sekcją `permission`), funkcja `check_tool_permission()` zwraca `"allow"` dla WSZYSTKICH narzędzi (linia 75: `None => "allow"`). Oznacza to, że domyślnie dialog uprawnień NIGDY się nie pojawia, nawet dla niebezpiecznych narzędzi (edit, bash, webfetch).

**Poprawne zachowanie:** Gdy brak konfiguracji, niebezpieczne narzędzia (edit, bash, webfetch) powinny domyślnie dostać `"ask"`, a tylko read-only `"allow"` — tak samo jak w gałęzi `Some(p)` z `.unwrap_or("ask")`.

**Kod do zmiany:** Linia 75 w `agent/mod.rs` — zamiast twardego `"allow"`, matchować po `tool_name` tak samo jak w gałęzi `Some(p)`.

---

### 2. ACP `session/cancel` — anulowanie z edytora nie działa
**Lokalizacja:** [acp_server.rs:166-227](file:///c:/projekt/opencode-rs/src/acp_server.rs#L166-L227)

**Ustalenia:**
- HashMap `prompt_aborts` jest poprawnie współdzielona przez `aborts_prompt` i `aborts_cancel` (oba klonują ten sam `Arc`).
- `session_id_str` w handlerze `session/prompt` oraz `session_id` w `session/cancel` używają tego samego formatu `.0.to_string()`.

**Potencjalne problemy zidentyfikowane:**
- Handler `session/cancel` używa `.remove()` — jeśli anulowanie przyjdzie po zakończeniu taska (linia 186 już usunęła wpis), nic się nie stanie. To poprawne.
- **Prawdziwy problem:** AbortHandle abortuje `prompt_task`, ale NIE abortuje tasku streamera (`streamer` linia 150) oraz NIE czyści kanału `token_tx`. Streamer może wisieć w tle. Dodatkowo, jeśli `process_user_prompt()` wykonuje długą synchroniczną operację bez `.await`, tokio nie przerwie jej natychmiast.

**Poprawka:**
- W handlerze `session/cancel`, po `handle.abort()`, dodać abort również streamera (potrzebny drugi HashMap dla AbortHandle streamera lub lepsza struktura).
- Dodać log/eprintln! gdy cancel dotrze, żeby było widać że handler działa.
- Zapewnić że `StopReason::Cancelled` jest poprawnie propagowane również do streamera (wysłać ostatni chunk `stop_reason: cancelled`).

---

### 3. Niespójność ścieżek `.opencode` vs `.opencode-rs` w UI/komunikatach
**Zasada z AGENTS.md:** Zapis zawsze do `.opencode-rs/`, `.opencode/` tylko do ODCZYTU (legacy migration).

**Pliki do poprawki — komunikaty użytkownika, w których pojawia się `.opencode` i sugerują ZAPIS (nie odczyt legacy):**

| Plik | Linie | Komunikat | Poprawka |
|------|-------|-----------|----------|
| [app.rs:3384](file:///c:/projekt/opencode-rs/src/app.rs#L3384) | 3384, 3386, 3388 | `/agents` help mówi `.opencode/agents` | Wymienić na `.opencode-rs/agents` (lub dodaj info o obu: odczyt z .opencode, zapis do .opencode-rs) |
| [app.rs:3400](file:///c:/projekt/opencode-rs/src/app.rs#L3400) | 3400 | "Utwórz pliki .md w: .opencode/agents/*.md" | `.opencode-rs/agents/*.md` + wspomnij o legacy odczycie |
| [app.rs:3459](file:///c:/projekt/opencode-rs/src/app.rs#L3459) | 3459 | `.opencode/commands/*.md` | `.opencode-rs/commands/*.md` |
| [app.rs:3491](file:///c:/projekt/opencode-rs/src/app.rs#L3491) | 3491 | `.opencode/plugins/*.js|ts` | `.opencode-rs/plugins/*.js|ts` |
| [memory.rs:357](file:///c:/projekt/opencode-rs/src/memory.rs#L357) | 357, 359 | "zapisał procedurę do .opencode/skills/learned/" | `.opencode-rs/skills/learned/` |
| [memory.rs:468](file:///c:/projekt/opencode-rs/src/memory.rs#L468) | 468 | "dodaj .opencode/skills/*.md" | `.opencode-rs/skills/*.md` |
| [palette.rs:214](file:///c:/projekt/opencode-rs/src/palette.rs#L214) | 214, 220, 232 | descriptions mówią `.opencode/agents`/commands/plugins | `.opencode-rs/...` |
| [skills.rs:115](file:///c:/projekt/opencode-rs/src/skills.rs#L115) | 115 | "dodaj .opencode/skills/*.md" | `.opencode-rs/skills/*.md` |
| [agent/context.rs:374](file:///c:/projekt/opencode-rs/src/agent/context.rs#L374) | 374, 391, 393 | System prompt mówi `.opencode/plan.md` / `.opencode/skills/` | `.opencode-rs/plan.md` / `.opencode-rs/skills/` |

**Uwaga:** Odwołania do `.opencode/` jako *legacy read-only* lub *fallback import* zostają BEZ ZMIAN (np. `importer.rs`, `auth.rs` sekcje fallback, `~/.config/opencode/` — to są poprawne odwołania do legacy formatów innych narzędzi). Poprawiamy TYLKO komunikaty sugerujące użytkownikowi tworzenie/zapis w `.opencode/`.

---

### 4. AGENTS.md — liczba testów i commitów
**Lokalizacja:** [AGENTS.md:8](file:///c:/projekt/opencode-rs/AGENTS.md#L8) oraz sekcja Backlog

**Niespójności:**
- Linia 8: `cargo test # 163 testów, ~25s` → faktycznie 208 testów `--lib` (wg linii 252).
- Sekcja Build & Test: `cargo test app::` mówi o 15 testach — weryfikować czy aktualne.
- Dokument: "216 testów / 3 commity" → faktycznie "208 testów lib, 5 commitów".

**Poprawka:** Zaktualizować AGENTS.md:
- Linię 8 zmienić na: `cargo test # 208 testów --lib`
- Upewnić się że wszystkie sub-test counts (memory, skills itd.) sumują się poprawnie.
- Poprawić fragment o committach jeśli występuje.

---

### 5. Docker + uruchomianie.md
**Ustalenia:**
- Dockerfile ISTNIEJE: [Dockerfile](file:///c:/projekt/opencode-rs/Dockerfile) ✓
- `.dockerignore` ISTNIEJE: [.dockerignore](file:///c:/projekt/opencode-rs/.dockerignore) ✓
- [uruchamianie.md:32-59](file:///c:/projekt/opencode-rs/uruchamianie.md#L32-L59) mówi użytkownikowi "Utwórz plik Dockerfile w projekcie:" i pokazuje szablon — ZAMIAST odwołać się do istniejącego Dockerfile w repo.

**Poprawka dla uruchamianie.md:**
- Sekcję "Scenariusz 1A: Gotowy `Dockerfile`" przerobić: zamiast szablonu do samodzielnego wklejenia, napisać że repo zawiera gotowy `Dockerfile` + `.dockerignore`.
- Szablon Dockerfile można przenieść do osobnej podsekcji "Własny Dockerfile (opcjonalnie)" lub usunąć.
- Upewnić się że `docker-compose.yml` przykład w punkcie 1B montuje `/root/.opencode-rs` (jest już poprawnie na linii 76).

---

### 6. Testy integracyjne ACP/Kilo — `#[ignore]`
**Lokalizacja:**
- [tests/test_opencode_acp.rs:50](file:///c:/projekt/opencode-rs/tests/test_opencode_acp.rs#L50) i [78](file:///c:/projekt/opencode-rs/tests/test_opencode_acp.rs#L78)
- [tests/test_kilo_run.rs:55](file:///c:/projekt/opencode-rs/tests/test_kilo_run.rs#L55)

**Problem:** Każdy z tych testów MA JUŻ runtime check:
- `if std::process::Command::new("opencode").arg("--version").output().is_err() { return "SKIP" }`
- analogicznie dla `kilo`

Więc `#[ignore]` jest NADMIAROWY i powoduje że testy NIGDY nie są uruchamiane — nawet jeśli binarka jest dostępna. Bez flagi `--ignored` cargo test je pomija całkowicie.

**Poprawka:** Usunąć atrybut `#[ignore]` z tych 3 funkcji testowych. Runtime skip logic (`SKIP: not available`) wystarczy, bo zwraca wiadomość i kończy bez `panic!`/`assert!`. Test przejdzie jako "pass" (lub pojawi się w outputcie jako skip) jeśli binarki nie ma.

---

## Pliki i Moduły do Zmiany

| Plik | Zakres zmiany |
|------|---------------|
| `src/agent/mod.rs` | `check_tool_permission()` — poprawka domyślnych uprawnień gdy `permissions` jest `None` |
| `src/acp_server.rs` | `session/cancel` handler — dodanie abortu streamera, logowania, propagacji StopReason |
| `src/app.rs` ~7 miejsc | Komunikaty `/agents`, `/commands`, `/plugins` — ścieżki `.opencode-rs/` zamiast `.opencode/` |
| `src/memory.rs` ~4 miejsca | Komunikaty o skills/learned — ścieżki `.opencode-rs/` |
| `src/palette.rs` 3 miejsca | Descriptions palety — `.opencode-rs/` |
| `src/skills.rs` 1 miejsce | Skill empty state message — `.opencode-rs/` |
| `src/agent/context.rs` 3 miejsca | System prompt strings — `.opencode-rs/` |
| `AGENTS.md` | Liczby testów w `Build & Test` i Backlog |
| `uruchamianie.md` | Scenariusz 1 — odwołanie do istniejącego Dockerfile |
| `tests/test_opencode_acp.rs` | Usunąć `#[ignore]` z 2 funkcji |
| `tests/test_kilo_run.rs` | Usunąć `#[ignore]` z 1 funkcji |
| `sqlite_version/` — wszystkie powyższe | Synchronizacja zmian po zatwierdzeniu |

---

## Kroki Implementacji (w kolejności zależności)

1. **Poprawka Permission "ask"** (`src/agent/mod.rs`)
   - Linia 75: `None => "allow"` → zamienić na match po `tool_name` identyczny jak w gałęzi `Some(p)` (edit/bash/webfetch → "ask", reszta → "allow").

2. **Poprawka ACP session/cancel** (`src/acp_server.rs`)
   - Wprowadzić strukturę `(AbortHandle, Option<AbortHandle>)` w HashMapie zamiast samego AbortHandle (prompt_task + streamer_task).
   - W handlerze `session/cancel`: abortować OBA taski jeśli istnieją, dodać `eprintln!("🛑 ACP session/cancel received for {}", session_id)`.
   - Przed abortem streamera wysłać ostatni chunk `session/update` z `stop_reason: cancelled`.
   - Upewnić się że po obu aborta HashMap jest czyszczony.

3. **Spójność ścieżek w UI komunikatach** (~8 plików)
   - Przeiterować tabelę z sekcji 3 i poprawić każdy wymieniony komunikat.
   - Konwencja: komunikaty do użytkownika → `.opencode-rs/` z dopiskiem "(lub .opencode/ dla wersji legacy/importu)" jeśli to odpowiednie miejsce.

4. **Aktualizacja AGENTS.md**
   - Linia 8: `163 testów` → `208 testów --lib`
   - Sekcja `Łącznie 208 testów` jest poprawna (linia 252), ale sprawdzić czy fragment "216 testów / 3 commity" nadal występuje i poprawić.
   - Sprawdzić sub-counts: `memory::` 19, `skills::` 5, `cost::` 6, `app::` 15, `importer::` 14, `file_manager::` 21, `database::` 12 — suma to 92, reszta to moduły agent/providers/opencode_compat itp. (116 więcej = 208).

5. **uruchamianie.md — Scenariusz 1**
   - Linia 32-59: zamiast "Utwórz plik Dockerfile" napisać "Repozytorium zawiera gotowy `Dockerfile` w korzeniu projektu".
   - Przykład docker-compose (1B) pozostać BEZ ZMIAN.
   - Szablon Dockerfile: albo usunąć, albo przenieść na koniec sekcji jako "Własny wariant Dockerfile (opcjonalnie)".

6. **Usunięcie nadmiarowych `#[ignore]`**
   - `tests/test_opencode_acp.rs`: usunąć `#[ignore]` z linii 50 i 78.
   - `tests/test_kilo_run.rs`: usunąć `#[ignore]` z linii 55.

7. **Synchronizacja do `sqlite_version/`**
   - Uruchomić skrypt: `powershell .\scripts\sync_to_sqlite.ps1`
   - Jeśli zmiany w plikach nie zsynchronizowanych przez skrypt (testy/AGENTS.md/uruchamianie.md), skopiować ręcznie analogiczne zmiany.

---

## Zależności i Uwagi

- **Kolejność:** Kroki 1-6 są niezależne od siebie (różne pliki), można robić równolegle. Krok 7 (sync) MUSI być ostatni.
- **`sqlite_version/tests/`:** Zawiera kopie testów ACP/Kilo — po usunięciu `#[ignore]` w głównym repo, skrypt sync powinien je skopiować. Weryfikować ręcznie.
- **Permission "ask" + TUI:** Po poprawce agent domyślnie będzie PYTAĆ o każdą edycję pliku i każdy bash. To zmienia UX więc warto ją opisać w komunikacie — "Aby wyłączyć pytania, ustaw w opencode.json: permission: { edit: allow, bash: allow }".
- **Runtime skip w testach:** Usunięcie `#[ignore]` nie zepsuje builda — testy bez binarki zwrócą "SKIP" i zakończą się sukcesem (bez assertów). Jeśli jednak binarka jest obecna a test się zawiesi, timeout 30s + `handle.abort()` ochroni (już zaimplementowane).

---

## Walidacja po Implementacji

1. **Permission "ask":**
   - Usunąć lub przemianować lokalne `opencode.json`/`commandcode.json`.
   - Uruchomić TUI, poprosić agenta o `echo hello` przez bash.
   - **OCZEKIWANE:** Pojawi się dialog uprawnień `[Enter] Zezwól / [Esc] Odmów`.
   - Esc powinien zwrócić błąd "odrzucone przez użytkownika", Enter pozwoli wykonać polecenie.
   - Dodatkowo: ustawić `permission.bash = "deny"` w opencode.json → bash ma być zablokowany bez dialogu.
   - Dodatkowo: ustawić `permission.edit = "allow"` → edycja bez dialogu.

2. **ACP session/cancel:**
   - Uruchomić `opencode --acp`, podłączyć testowy klient (lub curl symulujący notification).
   - Wysłać długi `session/prompt` (np. "Napisz esej na 1000 słów"), a po 1s wysłać `session/cancel` z tym samym session_id.
   - **OCZEKIWANE:** W stderr pojawi się log `🛑 ACP session/cancel received for <id>`, prompt jest przerwany, PromptResponse wraca z `StopReason::Cancelled`, proces nie wiszi.

3. **Spójność ścieżek:**
   - Uruchomić TUI, wykonać `/agents`, `/commands`, `/plugins`.
   - **OCZEKIWANE:** Komunikaty podpowiadają `.opencode-rs/agents/*.md` itp. jako ścieżkę DO STWORZENIA.
   - `/palace` → sprawdzić czy wiadomości o skills mają `.opencode-rs/`.
   - `/doctor` → to samo.

4. **AGENTS.md:**
   - `cargo test --lib` → policzyć wynik. Powinno być 208 testów pass.
   - Porównać z liczbami w AGENTS.md.

5. **Docker + uruchamianie:**
   - `docker build -t opencode-rs .` → build przechodzi (jeśli środowisko ma Docker).
   - uruchamianie.md: przeklik linków do własnego Dockerfile zamiast szablonu.

6. **Testy integracyjne (bez #[ignore]):**
   - `cargo test --test test_opencode_acp` → jeśli brak `opencode` CLI → przejdzie jako SKIP (paniczne). Jeśli jest → przejdzie z assertionem Warszawy/4.
   - `cargo test --test test_kilo_run` → analogicznie.
   - `cargo test --lib` → 208 testów dalej przechodzi w 100%.

7. **`cargo check` + `cargo build`:**
   - Zero błędów kompilacji, zero warningów (jak w regule projektu).

8. **Sync sqlite_version:**
   - `cd sqlite_version ; cargo check` → zero błędów.
   - `cd sqlite_version ; cargo test --lib` → wszystkie testy przechodzą.

---

## Ryzyka i Obsługa

| Ryzyko | Sposób obsługi |
|--------|---------------|
| Po poprawce permission "ask" dialog przy każdej edycji jest zbyt irytujący dla użytkowników bez opencode.json | Domyślne "ask" to poprawne i bezpieczne zachowanie. W komunikacie TUI przy pierwszym uruchomieniu dodajemy wskazówkę jak ustawić allow. Jeśli użytkownik chce starego zachowania, dodaje plik z permission.allow. |
| Abort streamera w ACP powoduje podwójne free lub panic przy dropie token_tx | Użyć `Option::take()` i bezpiecznego dropu; w najgorszym przypadku wyłączamy tylko prompt task, streamer sam się zamknie po zamknięciu rx (obecne zachowanie). |
| Usunięcie `#[ignore]` → CI/GitHub Actions ma `kilo`/`opencode` CLI i testy wiszą 30s w timeout | Runtime check *przed* spawnem (`Command::new("kilo").output().is_err()`) gwarantuje że bez binarki test zwraca SKIP natychmiast. Jeśli binarka JEST dostępna to test powinien działać; timeout 30s i tak uciśnie wiszący task. |
| Sync do sqlite_version psuje zmiany w sqlite_version (np. własny database.rs) | Skrypt sync jawnie OMIJA database.rs i archival.rs (zgodnie z AGENTS.md). Wszystkie pozostałe pliki są bezpieczne do nadpisania. Przed syncem nie ma potrzeby backupu, a po sync uruchamiamy `cargo check`. |
