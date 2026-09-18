# Minimalny obraz Linux — OpenCode-RS Web UI
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    && rm -rf /var/lib/apt/lists/*

# Binarka zbudowana na hoście: cargo build --release
# (wersja publiczna: cd sqlite_version && cargo build --release)
COPY target/release/opencode-rs /usr/local/bin/opencode
RUN chmod +x /usr/local/bin/opencode

WORKDIR /workspace
EXPOSE 7711

# Web Companion nasłuchuje na 0.0.0.0 (port 7711, fallback 7712-7715)
# Uwaga: porty 8765-8767 są zarezerwowane dla Bridge Extension (Cursor/Trae/Windsurf)
ENTRYPOINT ["opencode", "web", "--port", "7711"]
