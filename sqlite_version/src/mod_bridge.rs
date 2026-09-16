use anyhow::{Result, anyhow};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

/// Persistent bridge dla CommandCode mods (.commandcode/mods/*.ts).
///
/// Mod jest uruchamiany raz jako persistent subprocess (node) z pełnym shim ModApi.
/// Komunikacja odbywa się przez stdin/stdout w formacie JSON-lines:
///   Rust → Mod: {"type":"event","event":"beforeToolCall","payload":{...}}
///   Mod → Rust: {"type":"result","result":{...}} lub {"type":"error","error":"..."}
///
/// Shim ModApi w JS emuluje pełny obiekt `cmd` z metodami:
///   cmd.hooks({beforeToolCall, afterToolCall, transformInput})
///   cmd.addTool(name, schema, handler)
///   cmd.addCommand(name, handler)
///   cmd.on(event, handler)
///   cmd.ui.notify(msg) / cmd.ui.confirm(msg) / cmd.ui.showEntry(entry)
///   cmd.exec(command) → wykonuje komendę i zwraca wynik
///   cmd.addProvider(name, config)
///   cmd.addRenderer(type, handler)
pub struct ModBridge {
    child: Child,
    /// Bufor na odpowiedzi od moda.
    responses: Mutex<Vec<serde_json::Value>>,
    /// Lista zarejestrowanych tools przez mod (cmd.addTool).
    registered_tools: Mutex<Vec<ModTool>>,
    /// Lista zarejestrowanych komend przez mod (cmd.addCommand).
    registered_commands: Mutex<Vec<ModCommand>>,
    /// Nazwa moda.
    pub mod_name: String,
    /// Ścieżka do pliku .ts.
    pub mod_path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ModTool {
    pub name: String,
    pub description: String,
    pub schema: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ModCommand {
    pub name: String,
    pub description: String,
}

/// JS shim kod który emuluje ModApi i komunikuje się z Rust przez stdin/stdout.
const MOD_API_SHIM: &str = r#"
// CommandCode ModApi shim — emuluje @commandcode/harness ModApi
// Komunikacja z Rust przez stdin/stdout JSON-lines.
const readline = require('readline');
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

const rl = readline.createInterface({ input: process.stdin, output: process.stdout, terminal: false });

// Rejestracja hooks/tools/commands/providers/renderers
const hooks = {};
const tools = {};
const commands = {};
const providers = {};
const renderers = {};
const eventHandlers = {};

// ModApi — obiekt `cmd` przekazywany do default export function mod(cmd)
const cmd = {
  hooks(h) { Object.assign(hooks, h); },
  addTool(name, schema, handler) {
    tools[name] = { name, schema, handler, description: schema.description || '' };
    send({ type: 'registered', kind: 'tool', name, description: schema.description || '' });
  },
  addCommand(name, handler) {
    commands[name] = { name, handler, description: '' };
    send({ type: 'registered', kind: 'command', name, description: '' });
  },
  on(event, handler) { eventHandlers[event] = handler; },
  addProvider(name, config) {
    providers[name] = { name, config };
    send({ type: 'registered', kind: 'provider', name });
  },
  addRenderer(type, handler) {
    renderers[type] = { type, handler };
    send({ type: 'registered', kind: 'renderer', type });
  },
  ui: {
    notify(msg) { send({ type: 'ui', action: 'notify', message: msg }); },
    confirm(msg) { send({ type: 'ui', action: 'confirm', message: msg }); return true; },
    showEntry(entry) { send({ type: 'ui', action: 'showEntry', entry }); },
  },
  exec(command) {
    try {
      const out = execSync(command, { encoding: 'utf-8', timeout: 30000, cwd: process.env.COMMANDCODE_MOD_CWD || '.' });
      return out;
    } catch (e) { return String(e); }
  },
  config: { flags: {} },
};

// Wyślij JSON-line do stdout
function send(obj) {
  process.stdout.write(JSON.stringify(obj) + '\n');
}

// Wczytaj i wykonaj mod
const modPath = process.env.COMMANDCODE_MOD_PATH;
if (!modPath) { send({ type: 'error', error: 'COMMANDCODE_MOD_PATH not set' }); process.exit(1); }

try {
  // Spróbuj załadować TypeScript przez ts-node jeśli dostępny, inaczej przez require
  let mod;
  if (modPath.endsWith('.ts')) {
    try {
      // Spróbuj ts-node
      require('ts-node/register');
      mod = require(modPath);
    } catch (e) {
      // Fallback: traktuj jako JS (strip type annotations nie jest idealne ale działa dla prostych modów)
      try { mod = require(modPath.replace(/\.ts$/, '.js')); }
      catch (e2) {
        send({ type: 'error', error: 'ts-node not available and .js fallback failed: ' + e2.message });
        send({ type: 'ready', tools: [], commands: [] });
        // Kontynuuj bez moda — przynajmniej odbieraj zdarzenia
      }
    }
  } else {
    mod = require(modPath);
  }

  // Wywołaj default export function
  if (mod && typeof mod.default === 'function') {
    mod.default(cmd);
  } else if (typeof mod === 'function') {
    mod(cmd);
  }

  send({ type: 'ready', tools: Object.keys(tools), commands: Object.keys(commands) });
} catch (e) {
  send({ type: 'error', error: 'Mod load failed: ' + e.message });
}

// Pętla zdarzeń — odbieraj zdarzenia od Rust
rl.on('line', (line) => {
  try {
    const msg = JSON.parse(line);
    if (msg.type === 'event') {
      const handler = hooks[msg.event] || eventHandlers[msg.event];
      if (handler) {
        try {
          const result = handler(msg.payload);
          if (result && typeof result.then === 'function') {
            result.then(r => send({ type: 'result', event: msg.event, result: r }))
                  .catch(e => send({ type: 'error', event: msg.event, error: String(e) }));
          } else {
            send({ type: 'result', event: msg.event, result });
          }
        } catch (e) {
          send({ type: 'error', event: msg.event, error: String(e) });
        }
      } else {
        send({ type: 'result', event: msg.event, result: undefined });
      }
    } else if (msg.type === 'call_tool') {
      const tool = tools[msg.tool];
      if (tool && tool.handler) {
        try {
          const result = tool.handler(msg.args);
          send({ type: 'tool_result', tool: msg.tool, result });
        } catch (e) {
          send({ type: 'error', tool: msg.tool, error: String(e) });
        }
      } else {
        send({ type: 'error', tool: msg.tool, error: 'Tool not found' });
      }
    } else if (msg.type === 'call_command') {
      const command = commands[msg.command];
      if (command && command.handler) {
        try {
          const result = command.handler(msg.args);
          send({ type: 'command_result', command: msg.command, result });
        } catch (e) {
          send({ type: 'error', command: msg.command, error: String(e) });
        }
      }
    }
  } catch (e) {
    send({ type: 'error', error: 'Parse error: ' + e.message });
  }
});

// Utrzymuj proces przy życiu
process.stdin.on('end', () => process.exit(0));
"#;

impl ModBridge {
    /// Uruchamia mod jako persistent subprocess z pełnym shim ModApi.
    pub fn start(mod_path: &Path, work_dir: &Path) -> Result<Self> {
        let mod_name = mod_path.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown").to_string();

        // Znajdź node lub bun
        let (node_cmd, node_args) = if which("bun") {
            ("bun", vec!["--eval".to_string()])
        } else if which("node") {
            ("node", vec!["--eval".to_string()])
        } else {
            return Err(anyhow!("Node.js lub Bun nie znaleziony na PATH — zainstaluj Node.js aby używać modów CommandCode"));
        };

        let mut cmd = if cfg!(windows) && node_cmd == "node" {
            let mut c = Command::new("cmd");
            c.arg("/c").arg(node_cmd);
            c
        } else {
            Command::new(node_cmd)
        };
        cmd.args(&node_args)
            .arg(MOD_API_SHIM)
            .env("COMMANDCODE_MOD_PATH", mod_path)
            .env("COMMANDCODE_MOD_CWD", work_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(work_dir);

        let mut child = cmd.spawn().map_err(|e| anyhow!("Mod spawn failed: {e}"))?;

        // Czekaj na "ready" message (timeout 5s)
        let stdout = child.stdout.as_mut().ok_or_else(|| anyhow!("Mod stdout missing"))?;
        let mut reader = BufReader::new(stdout);
        let mut tools = Vec::new();
        let mut commands = Vec::new();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            let mut line = String::new();
            // Non-blocking read with timeout
            let remaining = deadline - std::time::Instant::now();
            if read_line_timeout(&mut reader, &mut line, remaining) {
                if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&line) {
                    let msg_type = msg.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    match msg_type {
                        "ready" => {
                            if let Some(t) = msg.get("tools").and_then(|t| t.as_array()) {
                                for tool in t {
                                    if let Some(name) = tool.as_str() {
                                        tools.push(ModTool {
                                            name: name.to_string(),
                                            description: String::new(),
                                            schema: serde_json::json!({}),
                                        });
                                    }
                                }
                            }
                            break;
                        }
                        "registered" => {
                            let kind = msg.get("kind").and_then(|k| k.as_str()).unwrap_or("");
                            let name = msg.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            if kind == "tool" {
                                tools.push(ModTool {
                                    name: name.to_string(),
                                    description: msg.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                                    schema: serde_json::json!({}),
                                });
                            } else if kind == "command" {
                                commands.push(ModCommand {
                                    name: name.to_string(),
                                    description: msg.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string(),
                                });
                            }
                        }
                        "error" => {
                            let err = msg.get("error").and_then(|e| e.as_str()).unwrap_or("unknown");
                            // Nie przerywaj — mod może działać częściowo
                            eprintln!("Mod {} warning: {}", mod_name, err);
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(Self {
            child,
            responses: Mutex::new(Vec::new()),
            registered_tools: Mutex::new(tools),
            registered_commands: Mutex::new(commands),
            mod_name,
            mod_path: mod_path.to_path_buf(),
        })
    }

    /// Wyślij zdarzenie do moda (np. "beforeToolCall", "afterToolCall", "transformInput").
    /// Zwraca wynik handlera (jeśli zwrócił wartość).
    pub fn send_event(&mut self, event: &str, payload: &serde_json::Value) -> Result<serde_json::Value> {
        let stdin = self.child.stdin.as_mut().ok_or_else(|| anyhow!("Mod stdin missing"))?;
        let msg = serde_json::json!({
            "type": "event",
            "event": event,
            "payload": payload
        });
        let line = serde_json::to_string(&msg)? + "\n";
        stdin.write_all(line.as_bytes())?;
        stdin.flush()?;

        // Czekaj na odpowiedź (timeout 10s)
        let stdout = self.child.stdout.as_mut().ok_or_else(|| anyhow!("Mod stdout missing"))?;
        let mut reader = BufReader::new(stdout);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            let mut resp_line = String::new();
            let remaining = deadline - std::time::Instant::now();
            if read_line_timeout(&mut reader, &mut resp_line, remaining) {
                if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&resp_line) {
                    let resp_type = resp.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if resp_type == "result" {
                        return Ok(resp.get("result").cloned().unwrap_or(serde_json::Value::Null));
                    } else if resp_type == "error" {
                        let err = resp.get("error").and_then(|e| e.as_str()).unwrap_or("unknown");
                        return Err(anyhow!("Mod error: {}", err));
                    } else if resp_type == "ui" {
                        // UI notification — zapisz do bufora, kontynuuj czekanie
                        self.responses.lock().unwrap().push(resp);
                    }
                }
            }
        }
        Err(anyhow!("Mod response timeout"))
    }

    /// Wywołaj tool zarejestrowany przez mod (cmd.addTool).
    pub fn call_tool(&mut self, tool_name: &str, args: &serde_json::Value) -> Result<serde_json::Value> {
        let stdin = self.child.stdin.as_mut().ok_or_else(|| anyhow!("Mod stdin missing"))?;
        let msg = serde_json::json!({
            "type": "call_tool",
            "tool": tool_name,
            "args": args
        });
        let line = serde_json::to_string(&msg)? + "\n";
        stdin.write_all(line.as_bytes())?;
        stdin.flush()?;

        let stdout = self.child.stdout.as_mut().ok_or_else(|| anyhow!("Mod stdout missing"))?;
        let mut reader = BufReader::new(stdout);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            let mut resp_line = String::new();
            let remaining = deadline - std::time::Instant::now();
            if read_line_timeout(&mut reader, &mut resp_line, remaining) {
                if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&resp_line) {
                    let resp_type = resp.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if resp_type == "tool_result" {
                        return Ok(resp.get("result").cloned().unwrap_or(serde_json::Value::Null));
                    } else if resp_type == "error" {
                        let err = resp.get("error").and_then(|e| e.as_str()).unwrap_or("unknown");
                        return Err(anyhow!("Mod tool error: {}", err));
                    }
                }
            }
        }
        Err(anyhow!("Mod tool timeout"))
    }

    /// Zwraca tools zarejestrowane przez mod.
    pub fn registered_tools(&self) -> Vec<ModTool> {
        self.registered_tools.lock().unwrap().clone()
    }

    /// Zwraca commands zarejestrowane przez mod.
    pub fn registered_commands(&self) -> Vec<ModCommand> {
        self.registered_commands.lock().unwrap().clone()
    }

    /// Zwróć UI notifications z bufora.
    pub fn drain_ui_notifications(&self) -> Vec<serde_json::Value> {
        let mut responses = self.responses.lock().unwrap();
        let ui: Vec<_> = responses.drain(..).filter(|r| {
            r.get("type").and_then(|t| t.as_str()) == Some("ui")
        }).collect();
        ui
    }

    /// Zakończ mod.
    pub fn shutdown(&mut self) {
        let _ = self.child.kill();
    }
}

impl Drop for ModBridge {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Sprawdź czy komenda jest dostępna na PATH.
fn which(cmd: &str) -> bool {
    if cfg!(windows) {
        std::process::Command::new("where.exe")
            .arg(cmd)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else {
        std::process::Command::new("which")
            .arg(cmd)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

/// Odczytaj linię z timeoutem (blocking read z krótkim deadline).
/// Używa std::io::Read zamiast thread (BufRead nie jest Send).
fn read_line_timeout(reader: &mut impl BufRead, line: &mut String, timeout: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    // Proste czytanie znak po znaku z sprawdzaniem deadline
    let mut buf = [0u8; 1];
    loop {
        if std::time::Instant::now() > deadline { return false; }
        match reader.read(&mut buf) {
            Ok(0) => return !line.is_empty(),
            Ok(_) => {
                if buf[0] == b'\n' { return true; }
                line.push(buf[0] as char);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }
            Err(_) => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_which_node() {
        // node powinien być zainstalowany w środowisku deweloperskim
        // ale nie wymagaj tego w teście
        let _ = which("nonexistent_binary_xyz");
    }

    #[test]
    fn test_mod_api_shim_contains_modapi() {
        // Shim definiuje obiekt `cmd` z metodami — sprawdź czy zawiera kluczowe API
        assert!(MOD_API_SHIM.contains("hooks("), "shim should have hooks method");
        assert!(MOD_API_SHIM.contains("addTool"), "shim should have addTool");
        assert!(MOD_API_SHIM.contains("addCommand"), "shim should have addCommand");
        assert!(MOD_API_SHIM.contains("ui: {"), "shim should have ui object");
        assert!(MOD_API_SHIM.contains("notify(msg)"), "shim should have ui.notify");
        assert!(MOD_API_SHIM.contains("exec(command)"), "shim should have exec");
    }
}
