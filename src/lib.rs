//! lib.rs — punkt wejścia biblioteki dla testów integracyjnych.
//!
//! Główny binary jest w main.rs. Ten plik re-eksportuje wszystkie moduły
//! żeby testy integracyjne (tests/*.rs) mogły ich używać przez `opencode_rs::`.
//!
//! Wszystkie moduły są `pub mod` w main.rs — to standardowa technika dla
//! projektów binary+lib w Rust.

pub mod agent;
pub mod app;
pub mod auth;
pub mod config;
pub mod cost;
pub mod environment;
pub mod diff;
pub mod file_manager;
pub mod pty;
pub mod git;
pub mod hermes;
pub mod live_grep;
pub mod lsp;
pub mod i18n;
pub mod importer;
pub mod palette;
pub mod plugins;
pub mod providers;
pub mod runtime;
pub mod search;
pub mod session;
pub mod sync;
pub mod syntax;
pub mod templates;
pub mod theme;
pub mod ui;
pub mod update;
pub mod web;
pub mod taste;
pub mod skills;
pub mod memory;
pub mod archival;
pub mod e2e;
pub mod acp_server;
pub mod database;
