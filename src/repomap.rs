//! RepoMap — inteligentny, kompaktowy indeks symboli (AST/signatures) całego projektu.
//!
//! Generuje zwięzłe drzewo definicji (struktury, funkcje, klasy, traity, interfejsy)
//! mieszczące się w zadanym limicie tokenów (~1500-2000 tokenów).
//! Wstrzykiwane do system promptu agenta, dając pełną orientację w architekturze projektu
//! bez konieczności czytania setek plików.

use std::fs;
use std::path::{Path, PathBuf};

/// Symbol wyekstrahowany z kodu źródłowego
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolDef {
    pub kind: SymbolKind,
    pub name: String,
    pub signature: String,
    pub line_number: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Struct,
    Enum,
    Trait,
    Class,
    Interface,
    TypeAlias,
    Impl,
}

impl SymbolKind {
    pub fn badge(&self) -> &'static str {
        match self {
            SymbolKind::Function => "fn",
            SymbolKind::Struct => "struct",
            SymbolKind::Enum => "enum",
            SymbolKind::Trait => "trait",
            SymbolKind::Class => "class",
            SymbolKind::Interface => "interface",
            SymbolKind::TypeAlias => "type",
            SymbolKind::Impl => "impl",
        }
    }
}

/// Wpis pliku w mapie repozytorium
#[derive(Debug, Clone)]
pub struct FileSymbols {
    pub rel_path: String,
    pub symbols: Vec<SymbolDef>,
}

pub struct RepoMap {
    root_dir: PathBuf,
    max_tokens: usize,
}

impl RepoMap {
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            root_dir,
            max_tokens: 1800,
        }
    }

    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Buduje pełną mapę repozytorium jako sformatowany tekst
    pub fn build_map(&self) -> String {
        let files = self.collect_source_files();
        let mut file_symbols = Vec::new();

        for file_path in files {
            if let Ok(content) = fs::read_to_string(&file_path) {
                let symbols = Self::extract_symbols(&file_path, &content);
                if !symbols.is_empty() {
                    let rel = file_path
                        .strip_prefix(&self.root_dir)
                        .unwrap_or(&file_path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    file_symbols.push(FileSymbols {
                        rel_path: rel,
                        symbols,
                    });
                }
            }
        }

        self.format_and_budget(&file_symbols)
    }

    /// Zbiera ścieżki plików źródłowych projektu
    fn collect_source_files(&self) -> Vec<PathBuf> {
        let mut results = Vec::new();
        Self::walk_dir(&self.root_dir, &self.root_dir, &mut results, 6);
        // Sortuj alfabetycznie dla determinizmu
        results.sort();
        results
    }

    fn walk_dir(dir: &Path, root: &Path, results: &mut Vec<PathBuf>, max_depth: usize) {
        if max_depth == 0 || !dir.exists() {
            return;
        }

        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Ignoruj katalogi tymczasowe, budowania i ukryte
            if name.starts_with('.')
                || name == "target"
                || name == "node_modules"
                || name == "vendor"
                || name == "dist"
                || name == "build"
                || name == ".git"
            {
                continue;
            }

            if path.is_dir() {
                Self::walk_dir(&path, root, results, max_depth - 1);
            } else if path.is_file() {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if matches!(
                    ext.as_str(),
                    "rs" | "ts" | "js" | "py" | "go" | "java" | "c" | "cpp" | "h" | "hpp" | "cs"
                ) {
                    // Pomiń minified lub gigantyczne pliki (>300KB)
                    if let Ok(meta) = entry.metadata() {
                        if meta.len() < 300 * 1024 {
                            results.push(path);
                        }
                    }
                }
            }
        }
    }

    /// Ekstrahuje symbole z pliku na podstawie rozszerzenia
    pub fn extract_symbols(path: &Path, content: &str) -> Vec<SymbolDef> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        let mut symbols = Vec::new();

        for (line_idx, line) in content.lines().enumerate() {
            let line_num = line_idx + 1;
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
                continue;
            }

            match ext.as_str() {
                "rs" => {
                    Self::parse_rust_line(trimmed, line_num, &mut symbols);
                }
                "ts" | "js" => {
                    Self::parse_js_ts_line(trimmed, line_num, &mut symbols);
                }
                "py" => {
                    Self::parse_python_line(trimmed, line_num, &mut symbols);
                }
                "go" => {
                    Self::parse_go_line(trimmed, line_num, &mut symbols);
                }
                _ => {
                    // Domyślny parser bazowy
                    Self::parse_generic_line(trimmed, line_num, &mut symbols);
                }
            }
        }

        symbols
    }

    fn parse_rust_line(line: &str, line_num: usize, symbols: &mut Vec<SymbolDef>) {
        if line.starts_with("pub struct ") || line.starts_with("struct ") {
            let name = Self::extract_word(line, if line.starts_with("pub ") { 2 } else { 1 });
            symbols.push(SymbolDef {
                kind: SymbolKind::Struct,
                name: name.clone(),
                signature: format!("struct {name}"),
                line_number: line_num,
            });
        } else if line.starts_with("pub enum ") || line.starts_with("enum ") {
            let name = Self::extract_word(line, if line.starts_with("pub ") { 2 } else { 1 });
            symbols.push(SymbolDef {
                kind: SymbolKind::Enum,
                name: name.clone(),
                signature: format!("enum {name}"),
                line_number: line_num,
            });
        } else if line.starts_with("pub trait ") || line.starts_with("trait ") {
            let name = Self::extract_word(line, if line.starts_with("pub ") { 2 } else { 1 });
            symbols.push(SymbolDef {
                kind: SymbolKind::Trait,
                name: name.clone(),
                signature: format!("trait {name}"),
                line_number: line_num,
            });
        } else if line.starts_with("pub fn ") || line.starts_with("pub async fn ") {
            let sig = line.trim_end_matches('{').trim().to_string();
            let name = sig
                .split("fn ")
                .nth(1)
                .and_then(|s| s.split('(').next())
                .unwrap_or("")
                .trim()
                .to_string();
            if !name.is_empty() {
                symbols.push(SymbolDef {
                    kind: SymbolKind::Function,
                    name,
                    signature: sig,
                    line_number: line_num,
                });
            }
        } else if line.starts_with("impl ") {
            let sig = line.trim_end_matches('{').trim().to_string();
            symbols.push(SymbolDef {
                kind: SymbolKind::Impl,
                name: sig.clone(),
                signature: sig,
                line_number: line_num,
            });
        }
    }

    fn parse_js_ts_line(line: &str, line_num: usize, symbols: &mut Vec<SymbolDef>) {
        if line.starts_with("export function ") || line.starts_with("export async function ") {
            let sig = line.trim_end_matches('{').trim().to_string();
            let name = sig
                .split("function ")
                .nth(1)
                .and_then(|s| s.split('(').next())
                .unwrap_or("")
                .trim()
                .to_string();
            if !name.is_empty() {
                symbols.push(SymbolDef {
                    kind: SymbolKind::Function,
                    name,
                    signature: sig,
                    line_number: line_num,
                });
            }
        } else if line.starts_with("export class ") || line.starts_with("class ") {
            let name = Self::extract_word(line, if line.starts_with("export ") { 2 } else { 1 });
            symbols.push(SymbolDef {
                kind: SymbolKind::Class,
                name: name.clone(),
                signature: format!("class {name}"),
                line_number: line_num,
            });
        } else if line.starts_with("export interface ") || line.starts_with("interface ") {
            let name = Self::extract_word(line, if line.starts_with("export ") { 2 } else { 1 });
            symbols.push(SymbolDef {
                kind: SymbolKind::Interface,
                name: name.clone(),
                signature: format!("interface {name}"),
                line_number: line_num,
            });
        } else if line.starts_with("export type ") || line.starts_with("type ") {
            let name = Self::extract_word(line, if line.starts_with("export ") { 2 } else { 1 });
            symbols.push(SymbolDef {
                kind: SymbolKind::TypeAlias,
                name: name.clone(),
                signature: format!("type {name}"),
                line_number: line_num,
            });
        }
    }

    fn parse_python_line(line: &str, line_num: usize, symbols: &mut Vec<SymbolDef>) {
        if line.starts_with("class ") {
            let name = line
                .trim_start_matches("class ")
                .split('(')
                .next()
                .unwrap_or("")
                .trim_end_matches(':')
                .trim()
                .to_string();
            if !name.is_empty() {
                symbols.push(SymbolDef {
                    kind: SymbolKind::Class,
                    name: name.clone(),
                    signature: format!("class {name}"),
                    line_number: line_num,
                });
            }
        } else if line.starts_with("def ") || line.starts_with("async def ") {
            let sig = line.trim_end_matches(':').trim().to_string();
            let name = sig
                .split("def ")
                .nth(1)
                .and_then(|s| s.split('(').next())
                .unwrap_or("")
                .trim()
                .to_string();
            if !name.is_empty() && !name.starts_with('_') {
                symbols.push(SymbolDef {
                    kind: SymbolKind::Function,
                    name,
                    signature: sig,
                    line_number: line_num,
                });
            }
        }
    }

    fn parse_go_line(line: &str, line_num: usize, symbols: &mut Vec<SymbolDef>) {
        if line.starts_with("func ") {
            let sig = line.trim_end_matches('{').trim().to_string();
            let name = sig
                .trim_start_matches("func ")
                .split('(')
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            symbols.push(SymbolDef {
                kind: SymbolKind::Function,
                name,
                signature: sig,
                line_number: line_num,
            });
        } else if line.starts_with("type ") && (line.contains("struct") || line.contains("interface")) {
            let name = Self::extract_word(line, 1);
            let kind = if line.contains("interface") {
                SymbolKind::Interface
            } else {
                SymbolKind::Struct
            };
            symbols.push(SymbolDef {
                kind,
                name: name.clone(),
                signature: format!("type {name}"),
                line_number: line_num,
            });
        }
    }

    fn parse_generic_line(line: &str, line_num: usize, symbols: &mut Vec<SymbolDef>) {
        if line.starts_with("class ") {
            let name = Self::extract_word(line, 1);
            symbols.push(SymbolDef {
                kind: SymbolKind::Class,
                name: name.clone(),
                signature: format!("class {name}"),
                line_number: line_num,
            });
        }
    }

    fn extract_word(line: &str, index: usize) -> String {
        line.split_whitespace()
            .nth(index)
            .unwrap_or("")
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
            .to_string()
    }

    /// Formatuje zebrane symbole w estetyczne drzewo tekstowe mieszczące się w limicie tokenów
    fn format_and_budget(&self, files: &[FileSymbols]) -> String {
        if files.is_empty() {
            return "Brak plików źródłowych do zaindeksowania.".to_string();
        }

        let mut out = String::from("🧭 **RepoMap (Symbol Index)**:\n");
        let mut current_chars = out.len();
        let max_chars = self.max_tokens * 4; // ~4 chars per token heurystyka

        for file in files {
            let mut file_block = format!("📄 `{}`:\n", file.rel_path);
            for sym in &file.symbols {
                file_block.push_str(&format!("   • [{}] `{}`\n", sym.kind.badge(), sym.signature));
            }

            if current_chars + file_block.len() > max_chars {
                out.push_str("\n... [przycięto RepoMap ze względu na limit tokenów]\n");
                break;
            }

            out.push_str(&file_block);
            current_chars += file_block.len();
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rust_symbols() {
        let code = r#"
pub struct User {
    pub id: u64,
}

pub enum Status {
    Active,
    Inactive,
}

pub trait Authenticable {
    fn auth(&self) -> bool;
}

pub fn login(user: &User) -> bool {
    true
}

impl User {
    pub fn new() -> Self { User { id: 1 } }
}
"#;
        let p = Path::new("test.rs");
        let syms = RepoMap::extract_symbols(p, code);
        assert!(syms.iter().any(|s| s.kind == SymbolKind::Struct && s.name == "User"));
        assert!(syms.iter().any(|s| s.kind == SymbolKind::Enum && s.name == "Status"));
        assert!(syms.iter().any(|s| s.kind == SymbolKind::Trait && s.name == "Authenticable"));
        assert!(syms.iter().any(|s| s.kind == SymbolKind::Function && s.name == "login"));
    }

    #[test]
    fn test_extract_ts_symbols() {
        let code = r#"
export interface Config {
    port: number;
}

export class Server {
    start() {}
}

export function createServer(): Server {
    return new Server();
}
"#;
        let p = Path::new("server.ts");
        let syms = RepoMap::extract_symbols(p, code);
        assert!(syms.iter().any(|s| s.kind == SymbolKind::Interface && s.name == "Config"));
        assert!(syms.iter().any(|s| s.kind == SymbolKind::Class && s.name == "Server"));
        assert!(syms.iter().any(|s| s.kind == SymbolKind::Function && s.name == "createServer"));
    }

    #[test]
    fn test_repomap_budgeting() {
        let dir = std::env::temp_dir().join(format!("opencode_repomap_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let src_dir = dir.join("src");
        std::fs::create_dir_all(&src_dir).unwrap();

        std::fs::write(src_dir.join("main.rs"), "pub fn main() {}\npub struct App;\n").unwrap();
        std::fs::write(src_dir.join("lib.rs"), "pub enum Mode { A, B }\n").unwrap();

        let map = RepoMap::new(dir.clone()).with_max_tokens(500);
        let output = map.build_map();

        assert!(output.contains("RepoMap"));
        assert!(output.contains("main.rs"));
        assert!(output.contains("lib.rs"));
        assert!(output.contains("struct App"));

        let _ = std::fs::remove_dir_all(dir);
    }
}
