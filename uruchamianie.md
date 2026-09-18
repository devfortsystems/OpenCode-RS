# 🚀 OpenCode-RS — Przewodnik Uruchamiania i Wdrożeń

Kompletny podręcznik uruchamiania **OpenCode-RS**: w kontenerach Docker, środowisku WSL2, na zdalnych serwerach (VPS / Dedicated / Cloud przez SSH) oraz jako usługa w tle na Windowsie i Linuksie.

---

## 📌 Spis Treści
1. [Wymagania i Architektura (Dlaczego to takie proste?)](#1-wymagania-i-architektura)
2. [Scenariusz 1: Docker & Dev Containers](#scenariusz-1-docker--dev-containers)
3. [Scenariusz 2: WSL2 (Windows + Natywny Linux)](#scenariusz-2-wsl2-windows--natywny-linux)
4. [Scenariusz 3: Zdalny Serwer Linux (VPS / Cloud / Dedyk przez SSH)](#scenariusz-3-zdalny-serwer-linux-vps--cloud--dedyk-przez-ssh)
5. [Scenariusz 4: Działanie jako Usługa Systemowa (Service / Daemon)](#scenariusz-4-działanie-jako-usługa-systemowa-service--daemon)
6. [Scenariusz 5: Hybryda — UI na Windows, Kompilacja na Linuksie](#scenariusz-5-hybryda--ui-na-windows-kompilacja-na-linuksie)
7. [Dostęp Zdalny z Telefonu / Tabletu / Laptopa (LAN / Tailscale)](#dostęp-zdalny-z-telefonu--tabletu--laptopa)
8. [Podgląd Działających Modeli & Cennik](#podgląd-działających-modeli--cennik)

---

## 1. Wymagania i Architektura

W przeciwieństwie do Cursor, VS Code czy Devin (które ważą 400–800 MB i wymagają Electrona oraz Node.js):
- **Waga:** Jedna samodzielna binarka `opencode` (~8 MB).
- **RAM:** Zaledwie 15–25 MB (poniżej 5 MB w trybie zasobnika Windows Tray).
- **Zależności:** **ZERO** zewnętrznych zależności (brak Node.js, brak Pythona, brak Chromium). Baza danych (SQLite / DevFortDB) oraz serwer Web są wbudowane bezpośrednio w plik `.exe` / binarkę ELF.

---

## Scenariusz 1: Docker & Dev Containers

Uruchomienie OpenCode-RS w odizolowanym kontenerze z dostępem do Twojego projektu przez przeglądarkę.

### A. Gotowy `Dockerfile` z repozytorium

Repozytorium **zawiera już gotowy plik `Dockerfile`** w korzeniu projektu (razem z `.dockerignore`). Skorzystaj z niego bezpośrednio:

```bash
# 1. Skompiluj binarkę release
cargo build --release

# 2. Zbuduj obraz Docker
docker build -t opencode-rs .

# 3. Uruchom kontener
docker run -d -p 8765:8765 -v ./twoj-projekt:/workspace -v opencode-data:/root/.opencode-rs opencode-rs
```

**Zawartość gotowego Dockerfile:** (debian:bookworm-slim, kopiuje `target/release/opencode-rs`, wystawia port 8765, uruchamia `opencode web`)

---

### B. Własny `Dockerfile` (opcjonalnie, gdy potrzebujesz dodatkowych narzędzi)
Jeśli chcesz dostosować obraz (dodać kompilatory, Python, Node.js itp.), możesz rozszerzyć szablon:

```dockerfile
# Bazowy obraz Linux z dodatkowymi narzędziami developerskimi
FROM debian:bookworm-slim

# Zainstaluj narzędzia (dostosuj do swojego języka: build-essential, python3, rust, itp.)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    git \
    build-essential \
    python3 \
    && rm -rf /var/lib/apt/lists/*

# Skopiuj binarkę opencode (lub pobierz z wydania GitHub)
COPY target/release/opencode-rs /usr/local/bin/opencode
RUN chmod +x /usr/local/bin/opencode

# Katalog roboczy dla Twoich projektów
WORKDIR /workspace

# Port Web UI & API
EXPOSE 8765

# Uruchomienie serwera Web w trybie sieciowym
ENTRYPOINT ["opencode", "web", "--port", "8765"]
```

### C. `docker-compose.yml` (Zalecane)
```yaml
version: '3.8'

services:
  opencode:
    build: .
    container_name: opencode-ide
    restart: unless-stopped
    ports:
      - "8765:8765"
    volumes:
      # Montowanie Twojego kodu z hosta do kontenera
      - ./twoj-projekt:/workspace
      # Zachowanie bazy, sesji i pamięci AI pomiędzy restartami kontenera
      - opencode-data:/root/.opencode-rs
    environment:
      - OPENAI_API_KEY=${OPENAI_API_KEY}
      - ANTHROPIC_API_KEY=${ANTHROPIC_API_KEY}
      - GEMINI_API_KEY=${GEMINI_API_KEY}

volumes:
  opencode-data:
```

### D. Uruchomienie:
```bash
docker compose up -d
```
Otwórz w przeglądarce: **`http://localhost:8765`**  
Masz pełne IDE z edytorem, drzewem plików i agentem kompilującym kod wewnątrz Dockera!

---

## Scenariusz 2: WSL2 (Windows + Natywny Linux)

Idealne rozwiązanie, gdy pracujesz na Windowsie, ale kod wymaga Linuksa (np. linuksowe pakiety C++, Docker, skrypty Bash).

### Krok 1: Instalacja w WSL
Otwórz terminal WSL (Ubuntu):
```bash
# Sklonuj lub skompiluj projekt w WSL
cd ~
git clone https://github.com/devfortsystems/OpenCode-RS.git
cd OpenCode-RS
cargo build --release

# Skopiuj binarkę do PATH
sudo cp target/release/opencode-rs /usr/local/bin/opencode
```

### Krok 2: Uruchomienie serwera Web w WSL
W terminalu WSL:
```bash
opencode web --host 0.0.0.0 --port 8765
```

### Krok 3: Dostęp z Windowsa
WSL2 automatycznie przekazuje porty na Windowsa!  
Otwórz w Chrome / Edge na Windowsie: **`http://localhost:8765`**.  
- Wszystkie pliki, kompilacje i testy wykonują się w natywnym jądrze Linuksa WSL2 z pełną prędkością ext4.
- Ty sterujesz wszystkim z Windowsa.

---

## Scenariusz 3: Zdalny Serwer Linux (VPS / Cloud / Dedyk przez SSH)

Praca na potężnej maszynie zdalnej (np. Hetzner, OVH, AWS EC2, maszyna z kartami GPU).

### Krok 1: Prześlij binarkę na serwer
Z Windowsa (PowerShell):
```powershell
scp target/release/opencode user@twoj-serwer.pl:/usr/local/bin/opencode
ssh user@twoj-serwer.pl "chmod +x /usr/local/bin/opencode"
```

### Krok 2: Uruchomienie na serwerze
Zaloguj się na serwer przez SSH:
```bash
ssh user@twoj-serwer.pl

# Przejdź do katalogu z projektem
cd /var/www/moj-projekt

# Odpal serwer z tokenem autoryzacyjnym
opencode web --host 0.0.0.0 --port 8765 --token SuperTajneHaslo123
```

### Krok 3: Połączenie z Twojego komputera

**Opcja A: Bezpieczny tunel SSH (Zalecana — brak konieczności otwierania portu na firewallu):**
```bash
ssh -L 8765:localhost:8765 user@twoj-serwer.pl
```
Wtedy na swoim laptopie wchodzisz na: **`http://localhost:8765`**.

**Opcja B: Bezpośrednio przez IP serwera:**
Otwórz port 8765 w firewallu (`ufw allow 8765/tcp`) i wejdź na:
```
http://twoj-serwer.pl:8765/?token=SuperTajneHaslo123
```

---

## Scenariusz 4: Działanie jako Usługa Systemowa (Service / Daemon)

Serwer ma działać non-stop w tle i wstawać automatycznie po restarcie maszyny.

### A. Na Linuksie (systemd):
Utwórz plik usługi `/etc/systemd/system/opencode.service`:
```ini
[Unit]
Description=OpenCode-RS Web Companion & AI Agent Daemon
After=network.target

[Service]
Type=simple
User=root
WorkingDirectory=/root/projekty
ExecStart=/usr/local/bin/opencode web --host 0.0.0.0 --port 8765 --token MojKlucz2026
Restart=always
RestartSec=5
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
```

Włącz i uruchom usługę:
```bash
sudo systemctl daemon-reload
sudo systemctl enable --now opencode
sudo systemctl status opencode
```

### B. Na Windowsie (Zasobnik Systemowy Tray / Autostart):
Mamy wbudowaną natywną obsługę daemona Windows Tray:
```powershell
opencode --tray
```
- Aplikacja chowa się do zasobnika systemowego obok zegara Windows (<5 MB RAM).
- Kliknij prawym przyciskiem myszy na ikonę i wybierz **„Uruchamiaj przy starcie Windows”** — wpis zostanie dodany do rejestru `HKCU\...\Run`.
- Dwuklik na ikonę natychmiast otwiera Web UI w Twojej domyślnej przeglądarce.

---

## Scenariusz 5: Hybryda — UI na Windows, Kompilacja na Linuksie

Chcesz mieć interfejs OpenCode-RS na Windowsie, ale nie chcesz instalować kompilatorów ani baz danych na Windowsie.

### Jak to skonfigurować:
1. **WSL Bridge:**
   W promptach do agenta w OpenCode-RS możesz po prostu pisać:
   > *"Zbuduj projekt i odpal testy w WSL"*
   
   Agent AI automatycznie użyje narzędzia terminala z prefiksem `wsl`:
   ```bash
   wsl bash -c "cd /home/user/app && cargo test"
   ```
2. **Kompilacja w kontenerze Docker:**
   Możesz polecić agentowi:
   > *"Skompiluj binarkę w kontenerze linuksowym"*
   
   Agent wykona:
   ```bash
   docker run --rm -v ${PWD}:/src -w /src rust:latest cargo build --release
   ```

---

## Dostęp Zdalny z Telefonu / Tabletu / Laptopa

Jeśli OpenCode-RS działa na Twoim komputerze stacjonarnym lub serwerze:
1. Upewnij się, że serwer nasłuchuje na `0.0.0.0`:
   ```bash
   opencode web --host 0.0.0.0
   ```
2. **W sieci domowej / firmowej Wi-Fi:**
   Sprawdź lokalne IP komputera (`ipconfig` na Windows lub `ip a` na Linuksie, np. `192.168.1.50`).  
   Na telefonie/tablecie w przeglądarce wpisz:
   ```
   http://192.168.1.50:8765
   ```
3. **Poza domem (bez publicznego IP):**
   Użyj darmowego **Tailscale**:
   - Zainstaluj Tailscale na PC i telefonie.
   - Wejdź na adres Tailscale swojego PC (np. `http://100.85.12.34:8765`).
   - Masz bezpieczne, szyfrowane połączenie z dowolnego miejsca na świecie bez przekierowywania portów na routerze.

---

## Podgląd Działających Modeli & Cennik

Aby sprawdzić, które modele i dostawcy AI są w danej chwili aktywne i ile kosztują:

### W terminalu (CLI):
```bash
opencode models           # Pełna lista działających modeli wraz z cenami za 1M tokenów
opencode models --free    # Tylko w 100% darmowe modele ($0.00)
opencode models gemini    # Filtrowanie po nazwie lub dostawcy
opencode quota            # Wykorzystanie limitów zapytań (RPD), salda USD i daty odnowienia
```

### W Web UI:
1. Otwórz Web UI w przeglądarce (`http://localhost:8765`).
2. Kliknij w górnym pasku przycisk **„Cennik & Modele”** (ikona dolara).
3. Pojawi się interaktywna tabela ze statusem dostępności, ceną promptu/odpowiedzi oraz przyciskiem *„Wybierz dla karty”*.
