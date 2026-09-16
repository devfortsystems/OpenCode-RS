# DISTRIBUTION.md — Architektura Wersji i Dystrybucji

Niniejszy dokument definiuje oficjalną strategię podziału repozytorium OpenCode-RS na wersję prywatną (wewnętrzną) oraz wersję publiczną (Open Source).

---

## 1. Dwie Wersje Projektu

### 🔒 Wersja Prywatna (Główny katalog repozytorium — dla autora / DevFort Systems)
- **Silnik bazy danych:** Wbudowana baza **DevFortDB** (`devfortdb-core`, `devfortdb-embedded-api` zoptymalizowana pod kątem MDBX, grafowektora HNSW, transakcji ACID, pamięci RamCache i szyfrowania).
- **Zależności:** Lokalne crates z `../baza/crates/*`.
- **Dostęp:** Ściśle prywatne, lokalne repozytorium autora (Daniel / DevFort Systems).
- **Zasada bezwzględna:** Kod, architektura wewnętrzna oraz pliki binarne bazy danych DevFortDB **NIE MOGĄ** być udostępniane publicznie ani wypychane do otwartych repozytoriów.

---

### 🌐 Wersja Publiczna (Katalog `sqlite_version/` — do publicznego repozytorium)
- **Silnik bazy danych:** Otwarty silnik **SQLite** (`rusqlite = { version = "0.32", features = ["bundled"] }`).
- **Zależności:** 100% publiczne, open-source'owe biblioteki z crates.io. Kompilacja bez zewnętrznych zależności systemowych (bundled C source).
- **Archival Memory / Wektory:** Wbudowany, lekki indeks wektorowy z metryką podobieństwa cosinusowego (`SimpleVectorIndex`), w pełni kompatybilny z embeddingami lokalnymi (Hash), Ollama oraz OpenAI.
- **Dostęp:** Oficjalne publiczne repozytorium Open Source (GitHub: `devfortsystems/OpenCode-RS`).
- **Funkcjonalności:** W 100% identyczne możliwości jak wersja prywatna:
  - Pełny ReAct loop agenta,
  - Narzędzia (`read_file`, `edit_file`, `write_file`, `bash`, `grep`, `web_fetch`),
  - 3-kolumnowy tryb IDE (`F3` / `/ide`),
  - Kolorowanie składni w palecie VS Code Dark+ (TextMate / syntect) dla 50+ języków,
  - Menadżer wtyczek `.vsix` (`/vsix`),
  - AST RepoMap (<1500 tokenów),
  - Wszyscy providerzy (OpenAI, Claude, Gemini, DeepSeek, Devin ACP, Kilo, Ollama, Antigravity IDE),
  - Pełna kompatybilność z konfiguracjami OpenCode i CommandCode.

---

## 2. Tabela Porównawcza

| Cecha | Wersja Prywatna (Root) | Wersja Publiczna (`sqlite_version/`) |
|---|---|---|
| **Przeznaczenie** | Użytek własny autora / DevFort Systems | Publiczne repozytorium GitHub / crates.io |
| **Baza danych** | `DevFortDB` (MDBX, transakcyjna pamięć embedded) | `SQLite` (`rusqlite` bundled) |
| **Zależności prywatne** | Tak (`../baza/crates/*`) | **ZERO** — tylko oficjalne crates.io |
| **Repozytorium Git** | Prywatne | Publiczne (`origin`) |
| **Wymagania kompilacji** | Dostęp do `../baza` | Czysty Rust (`cargo build`), zero konfiguracji |
| **Plik bazy danych** | `.opencode/devfort.db` | `.opencode/opencode.db` |
| **Funkcje TUI / IDE** | Identyczne | Identyczne |

---

## 3. Procedura Publikacji do Publicznego Repo

Podczas publikacji do otwartego repozytorium (`git push origin main` na publicznym remote):

1. **Źródło dla publicznego repo:** Publikowana jest wyłącznie zawartość katalogu `sqlite_version/` (lub gałąź utworzona z jego zawartości).
2. **Weryfikacja zależności:**
   ```bash
   cd sqlite_version
   cargo check
   cargo test
   ```
3. **Prywatne repozytorium:** Główny root projektu z `DevFortDB` pozostaje w prywatnym repozytorium autora.
