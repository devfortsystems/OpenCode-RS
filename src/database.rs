//! DevFortDB — wbudowana baza danych w opencode-rs.
//!
//! DevFortDB to high-performance embedded DB (MDBX, ACID, TTL, JSON, HNSW).
//! Używana w opencode-rs do:
//! - **Sesje** — zapis/odczyt historii czatu (zamiast JSON files)
//! - **Memory blocks** — persistentna pamięć (persona/human/project)
//! - **Plan projektu** — zapis/aktualizacja planu
//! - **Archival memory** — wektorowa pamięć długoterminowa (HNSW)
//! - **Cache** — cache modeli, providerów, wyników
//! - **Token usage stats** — per-model statystyki użycia tokenów
//!
//! API: `Database::open(work_dir)` → `Database` z namespace'ami.
//! Każdy namespace ma izolowane klucze (prefiks `{ns}:`).
//!
//! Przykład:
//! ```ignore
//! let db = Database::open(&work_dir)?;
//! db.put_session("session-uuid", &session_data)?;
//! let session: Option<SessionData> = db.get_session("session-uuid")?;
//! ```

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde::de::DeserializeOwned;
use serde::Serialize;

use devfortdb_core::{Config, Engine};
use devfortdb_embedded_api::ns::ApiError;
use devfortdb_embedded_api::Namespace;

/// Wbudowana baza danych opencode-rs — wrapper na DevFortDB Engine.
///
/// Namespace'y izolują dane:
/// - `sessions` — historie czatów
/// - `memory` — bloki pamięci (persona/human/project)
/// - `plan` — plany projektów
/// - `archival` — pamięć wektorowa (HNSW)
/// - `cache` — cache modeli/providerów
/// - `stats` — statystyki użycia tokenów
#[derive(Clone)]
pub struct Database {
    engine: Arc<Engine>,
}

impl Database {
    /// Otwiera bazę w katalogu `.opencode/db/` (tworzy jeśli nie istnieje).
    pub fn open(work_dir: &Path) -> Result<Self> {
        let db_path = work_dir.join(".opencode").join("db");
        std::fs::create_dir_all(&db_path)?;

        let cfg = Config {
            path: db_path.join("opencode.mdb"),
            max_size: 256 * 1024 * 1024, // 256 MiB — wystarczająco dla sesji + memory
            hot_cache_capacity: 1024,    // 1024 hot keys w LRU
            max_dbs: 16,
            backup_log: false,
            single_writer: true, // opencode-rs = jeden writer (TUI)
            durable_writes: true, // ACID — sesje muszą być trwałe
            ..Default::default()
        };

        let engine = Engine::open(&cfg)?;
        Ok(Self {
            engine: Arc::new(engine),
        })
    }

    /// Otwiera bazę w trybie fast (nondurable) — dla cache/statystyk.
    /// 10-30x szybsze zapisy, utrata max ~10ms danych przy crashu.
    pub fn open_fast(work_dir: &Path) -> Result<Self> {
        let db_path = work_dir.join(".opencode").join("db");
        std::fs::create_dir_all(&db_path)?;

        let cfg = Config {
            path: db_path.join("opencode.mdb"),
            max_size: 256 * 1024 * 1024,
            hot_cache_capacity: 2048,
            max_dbs: 16,
            backup_log: false,
            single_writer: true,
            durable_writes: false, // fast mode — dla cache
            ..Default::default()
        };

        let engine = Engine::open(&cfg)?;
        Ok(Self {
            engine: Arc::new(engine),
        })
    }

    // ── Generic JSON API ────────────────────────────────────────────────

    /// Zapis JSON do namespace.
    pub fn put<T: Serialize>(&self, ns: &str, key: &[u8], value: &T) -> Result<()> {
        let ns = Namespace::new(&self.engine, ns);
        ns.put_json(key, value).map_err(map_err)
    }

    /// Zapis JSON z TTL.
    pub fn put_ttl<T: Serialize>(
        &self,
        ns: &str,
        key: &[u8],
        value: &T,
        ttl: Duration,
    ) -> Result<()> {
        let ns = Namespace::new(&self.engine, ns);
        ns.put_json_ttl(key, value, ttl).map_err(map_err)
    }

    /// Odczyt JSON z namespace.
    pub fn get<T: DeserializeOwned>(&self, ns: &str, key: &[u8]) -> Result<Option<T>> {
        let ns = Namespace::new(&self.engine, ns);
        ns.get_json(key).map_err(map_err)
    }

    /// Usunięcie klucza z namespace.
    pub fn delete(&self, ns: &str, key: &[u8]) -> Result<bool> {
        let ns = Namespace::new(&self.engine, ns);
        ns.delete(key).map_err(map_err)
    }

    /// Scan wszystkich kluczy w namespace.
    pub fn scan(&self, ns: &str) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let ns = Namespace::new(&self.engine, ns);
        ns.scan().map_err(map_err)
    }

    /// Atomiczny licznik w namespace.
    pub fn increment(&self, ns: &str, key: &[u8], delta: i64) -> Result<i64> {
        let ns = Namespace::new(&self.engine, ns);
        ns.increment(key, delta).map_err(map_err)
    }

    // ── Convenience: string keys ────────────────────────────────────────

    /// Zapis JSON z kluczem string.
    pub fn put_str<T: Serialize>(&self, ns: &str, key: &str, value: &T) -> Result<()> {
        self.put(ns, key.as_bytes(), value)
    }

    /// Odczyt JSON z kluczem string.
    pub fn get_str<T: DeserializeOwned>(&self, ns: &str, key: &str) -> Result<Option<T>> {
        self.get(ns, key.as_bytes())
    }

    /// Usunięcie z kluczem string.
    pub fn delete_str(&self, ns: &str, key: &str) -> Result<bool> {
        self.delete(ns, key.as_bytes())
    }

    // ── High-level API: sesje ───────────────────────────────────────────

    /// Zapis sesji czatu (JSON).
    pub fn put_session<T: Serialize>(&self, session_id: &str, session: &T) -> Result<()> {
        self.put_str("sessions", session_id, session)
    }

    /// Odczyt sesji czatu.
    pub fn get_session<T: DeserializeOwned>(&self, session_id: &str) -> Result<Option<T>> {
        self.get_str("sessions", session_id)
    }

    /// Usunięcie sesji.
    pub fn delete_session(&self, session_id: &str) -> Result<bool> {
        self.delete_str("sessions", session_id)
    }

    /// Lista wszystkich sesji (klucze jako string).
    pub fn list_sessions(&self) -> Result<Vec<String>> {
        let entries = self.scan("sessions")?;
        Ok(entries
            .into_iter()
            .filter_map(|(k, _)| String::from_utf8(k).ok())
            .collect())
    }

    // ── High-level API: memory blocks ───────────────────────────────────

    /// Zapis bloku pamięci (persona/human/project).
    pub fn put_memory<T: Serialize>(&self, block_name: &str, data: &T) -> Result<()> {
        self.put_str("memory", block_name, data)
    }

    /// Odczyt bloku pamięci.
    pub fn get_memory<T: DeserializeOwned>(&self, block_name: &str) -> Result<Option<T>> {
        self.get_str("memory", block_name)
    }

    // ── High-level API: plan ────────────────────────────────────────────

    /// Zapis planu projektu.
    pub fn put_plan<T: Serialize>(&self, project_id: &str, plan: &T) -> Result<()> {
        self.put_str("plan", project_id, plan)
    }

    /// Odczyt planu projektu.
    pub fn get_plan<T: DeserializeOwned>(&self, project_id: &str) -> Result<Option<T>> {
        self.get_str("plan", project_id)
    }

    // ── High-level API: cache (z TTL) ───────────────────────────────────

    /// Zapis do cache z TTL.
    pub fn cache_put<T: Serialize>(&self, key: &str, value: &T, ttl: Duration) -> Result<()> {
        self.put_ttl("cache", key.as_bytes(), value, ttl)
    }

    /// Odczyt z cache.
    pub fn cache_get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        self.get_str("cache", key)
    }

    // ── High-level API: stats ───────────────────────────────────────────

    /// Inkrementacja licznika statystyk (np. token usage per model).
    pub fn stats_increment(&self, key: &str, delta: i64) -> Result<i64> {
        self.increment("stats", key.as_bytes(), delta)
    }

    /// Odczyt licznika statystyk (raw bytes → i64).
    pub fn stats_get(&self, key: &str) -> Result<Option<i64>> {
        let ns = Namespace::new(&self.engine, "stats");
        let bytes = ns.get_bytes(key.as_bytes()).map_err(map_err)?;
        match bytes {
            Some(b) if b.len() == 8 => {
                let arr: [u8; 8] = b.as_slice().try_into().unwrap();
                Ok(Some(i64::from_be_bytes(arr)))
            }
            Some(b) if !b.is_empty() => {
                // Fallback — może być zapisane jako JSON string
                let s = String::from_utf8_lossy(&b);
                Ok(s.parse::<i64>().ok())
            }
            _ => Ok(None),
        }
    }

    // ── Direct engine access (zaawansowane) ─────────────────────────────

    /// Bezpośredni dostęp do Engine (dla HNSW, backup, etc.).
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

/// Mapuje błędy DevFortDB na anyhow::Error.
fn map_err(e: ApiError) -> anyhow::Error {
    anyhow::anyhow!("database: {}", e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    fn test_db() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(dir.path()).unwrap();
        (dir, db)
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestUser {
        name: String,
        level: u32,
    }

    #[test]
    fn test_put_get_json() {
        let (_d, db) = test_db();
        let user = TestUser {
            name: "daniel".into(),
            level: 7,
        };
        db.put_str("test", "u1", &user).unwrap();
        let back: Option<TestUser> = db.get_str("test", "u1").unwrap();
        assert_eq!(back, Some(user));
    }

    #[test]
    fn test_get_missing() {
        let (_d, db) = test_db();
        let back: Option<TestUser> = db.get_str("test", "nonexistent").unwrap();
        assert_eq!(back, None);
    }

    #[test]
    fn test_delete() {
        let (_d, db) = test_db();
        db.put_str("test", "key1", &"value").unwrap();
        assert!(db.delete_str("test", "key1").unwrap());
        let back: Option<String> = db.get_str("test", "key1").unwrap();
        assert_eq!(back, None);
    }

    #[test]
    fn test_namespace_isolation() {
        let (_d, db) = test_db();
        db.put_str("ns1", "key", &"val1").unwrap();
        db.put_str("ns2", "key", &"val2").unwrap();
        let v1: Option<String> = db.get_str("ns1", "key").unwrap();
        let v2: Option<String> = db.get_str("ns2", "key").unwrap();
        assert_eq!(v1, Some("val1".into()));
        assert_eq!(v2, Some("val2".into()));
    }

    #[test]
    fn test_scan() {
        let (_d, db) = test_db();
        db.put_str("scan", "a", &1u32).unwrap();
        db.put_str("scan", "b", &2u32).unwrap();
        db.put_str("scan", "c", &3u32).unwrap();
        let entries = db.scan("scan").unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn test_increment() {
        let (_d, db) = test_db();
        assert_eq!(db.increment("counters", b"hits", 1).unwrap(), 1);
        assert_eq!(db.increment("counters", b"hits", 5).unwrap(), 6);
        assert_eq!(db.increment("counters", b"hits", -2).unwrap(), 4);
    }

    #[test]
    fn test_ttl_expiry() {
        let (_d, db) = test_db();
        db.cache_put("temp", &"data", Duration::from_millis(100)).unwrap();
        assert!(db.cache_get::<String>("temp").unwrap().is_some());
        std::thread::sleep(Duration::from_millis(200));
        assert!(db.cache_get::<String>("temp").unwrap().is_none());
    }

    #[test]
    fn test_session_api() {
        let (_d, db) = test_db();
        let session = TestUser {
            name: "session1".into(),
            level: 42,
        };
        db.put_session("sess-123", &session).unwrap();
        let back: Option<TestUser> = db.get_session("sess-123").unwrap();
        assert_eq!(back, Some(session));
        let sessions = db.list_sessions().unwrap();
        assert!(sessions.contains(&"sess-123".to_string()));
        assert!(db.delete_session("sess-123").unwrap());
        let back: Option<TestUser> = db.get_session("sess-123").unwrap();
        assert_eq!(back, None);
    }

    #[test]
    fn test_memory_api() {
        let (_d, db) = test_db();
        let block = "persona".to_string();
        db.put_memory(&block, &"Jestem opencode-rs").unwrap();
        let back: Option<String> = db.get_memory(&block).unwrap();
        assert_eq!(back, Some("Jestem opencode-rs".into()));
    }

    #[test]
    fn test_plan_api() {
        let (_d, db) = test_db();
        let plan = serde_json::json!({"steps": ["a", "b", "c"]});
        db.put_plan("proj-1", &plan).unwrap();
        let back: Option<serde_json::Value> = db.get_plan("proj-1").unwrap();
        assert_eq!(back, Some(plan));
    }

    #[test]
    fn test_stats_api() {
        let (_d, db) = test_db();
        db.stats_increment("tokens:claude-sonnet", 1500).unwrap();
        db.stats_increment("tokens:claude-sonnet", 500).unwrap();
        let total: Option<i64> = db.stats_get("tokens:claude-sonnet").unwrap();
        assert_eq!(total, Some(2000));
    }

    #[test]
    fn test_open_fast_mode() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open_fast(dir.path()).unwrap();
        db.put_str("fast", "k", &"v").unwrap();
        let back: Option<String> = db.get_str("fast", "k").unwrap();
        assert_eq!(back, Some("v".into()));
    }
}
