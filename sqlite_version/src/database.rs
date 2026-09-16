//! SQLite Database — wbudowana otwarta baza danych w opencode-rs (wersja SQLite).
//!
//! Zastępuje zamknięty silnik DevFortDB czystym, publicznym silnikiem SQLite (`rusqlite bundled`).
//! Zapewnia transakcyjność ACID, optymalny tryb WAL oraz wsparcie dla JSON i TTL.
//! Używana w opencode-rs do:
//! - **Sesje** — zapis/odczyt historii czatu (JSON)
//! - **Memory blocks** — persistentna pamięć (persona/human/project)
//! - **Plan projektu** — zapis/aktualizacja planu
//! - **Archival memory** — metadane wektorowej pamięci długoterminowej
//! - **Cache** — cache modeli, providerów, wyników z TTL
//! - **Token usage stats** — per-model statystyki użycia tokenów
//!
//! API jest w 100% kompatybilne z dotychczasowym:
//! `Database::open(work_dir)` → `Database` z namespace'ami.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};
use serde::de::DeserializeOwned;
use serde::Serialize;

/// Wbudowana baza danych opencode-rs — wrapper na SQLite.
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Ustala katalog bazy: `.opencode-rs/db` (fallback do legacy `.opencode/db`)
    pub fn resolve_db_dir(work_dir: &Path) -> PathBuf {
        let rs_path = work_dir.join(".opencode-rs").join("db");
        if rs_path.exists() {
            return rs_path;
        }
        let legacy_path = work_dir.join(".opencode").join("db");
        if legacy_path.exists() {
            return legacy_path;
        }
        rs_path
    }

    /// Otwiera bazę SQLite w katalogu `.opencode-rs/db/opencode.db`.
    pub fn open(work_dir: &Path) -> Result<Self> {
        let db_dir = Self::resolve_db_dir(work_dir);
        std::fs::create_dir_all(&db_dir)?;
        let db_file = db_dir.join("opencode.db");

        let conn = Connection::open(&db_file)
            .map_err(|e| anyhow!("Nie można otworzyć bazy SQLite {}: {e}", db_file.display()))?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;
             CREATE TABLE IF NOT EXISTS kv_store (
                 namespace TEXT NOT NULL,
                 key BLOB NOT NULL,
                 value BLOB NOT NULL,
                 expires_at INTEGER,
                 PRIMARY KEY (namespace, key)
             );
             CREATE INDEX IF NOT EXISTS idx_kv_expires ON kv_store(expires_at);",
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Otwiera bazę w trybie fast (synchronous = OFF) — dla operacji o wysokiej przepustowości.
    pub fn open_fast(work_dir: &Path) -> Result<Self> {
        let db = Self::open(work_dir)?;
        let conn = db.conn.lock().unwrap();
        conn.execute_batch("PRAGMA synchronous = OFF;")?;
        drop(conn);
        Ok(db)
    }

    // ── Generic JSON API ────────────────────────────────────────────────

    /// Zapis JSON do namespace.
    pub fn put<T: Serialize>(&self, ns: &str, key: &[u8], value: &T) -> Result<()> {
        let json_bytes = serde_json::to_vec(value)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO kv_store (namespace, key, value, expires_at)
             VALUES (?1, ?2, ?3, NULL)
             ON CONFLICT(namespace, key) DO UPDATE SET value = excluded.value, expires_at = NULL;",
            params![ns, key, json_bytes],
        )?;
        Ok(())
    }

    /// Zapis JSON z TTL (czas wygaśnięcia).
    pub fn put_ttl<T: Serialize>(
        &self,
        ns: &str,
        key: &[u8],
        value: &T,
        ttl: Duration,
    ) -> Result<()> {
        let json_bytes = serde_json::to_vec(value)?;
        let expires_at = (SystemTime::now() + ttl)
            .duration_since(UNIX_EPOCH)?
            .as_millis() as i64;

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO kv_store (namespace, key, value, expires_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(namespace, key) DO UPDATE SET value = excluded.value, expires_at = excluded.expires_at;",
            params![ns, key, json_bytes, expires_at],
        )?;
        Ok(())
    }

    /// Odczyt JSON z namespace (z automatyczną walidacją TTL).
    pub fn get<T: DeserializeOwned>(&self, ns: &str, key: &[u8]) -> Result<Option<T>> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64;
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT value, expires_at FROM kv_store WHERE namespace = ?1 AND key = ?2;",
        )?;

        let mut rows = stmt.query(params![ns, key])?;
        if let Some(row) = rows.next()? {
            let expires_at: Option<i64> = row.get(1)?;
            if let Some(exp) = expires_at {
                if exp <= now {
                    drop(rows);
                    drop(stmt);
                    // Wpis wygasł — usuń go
                    conn.execute(
                        "DELETE FROM kv_store WHERE namespace = ?1 AND key = ?2;",
                        params![ns, key],
                    )?;
                    return Ok(None);
                }
            }
            let bytes: Vec<u8> = row.get(0)?;
            let val = serde_json::from_slice(&bytes)?;
            Ok(Some(val))
        } else {
            Ok(None)
        }
    }

    /// Usunięcie klucza z namespace.
    pub fn delete(&self, ns: &str, key: &[u8]) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let count = conn.execute(
            "DELETE FROM kv_store WHERE namespace = ?1 AND key = ?2;",
            params![ns, key],
        )?;
        Ok(count > 0)
    }

    /// Scan wszystkich aktywnych kluczy w namespace.
    pub fn scan(&self, ns: &str) -> Result<Vec<(Vec<u8>, Vec<u8>)>> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as i64;
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT key, value FROM kv_store
             WHERE namespace = ?1 AND (expires_at IS NULL OR expires_at >= ?2);",
        )?;

        let rows = stmt.query_map(params![ns, now], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;

        let mut entries = Vec::new();
        for r in rows {
            entries.push(r?);
        }
        Ok(entries)
    }

    /// Atomiczny licznik w namespace.
    pub fn increment(&self, ns: &str, key: &[u8], delta: i64) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;

        let current: i64 = {
            let mut stmt = tx.prepare(
                "SELECT value FROM kv_store WHERE namespace = ?1 AND key = ?2;",
            )?;
            let mut rows = stmt.query(params![ns, key])?;
            if let Some(row) = rows.next()? {
                let bytes: Vec<u8> = row.get(0)?;
                if bytes.len() == 8 {
                    let arr: [u8; 8] = bytes.as_slice().try_into().unwrap();
                    i64::from_be_bytes(arr)
                } else {
                    String::from_utf8_lossy(&bytes).trim().parse::<i64>().unwrap_or(0)
                }
            } else {
                0
            }
        };

        let new_val = current + delta;
        let bytes = new_val.to_be_bytes().to_vec();

        tx.execute(
            "INSERT INTO kv_store (namespace, key, value, expires_at)
             VALUES (?1, ?2, ?3, NULL)
             ON CONFLICT(namespace, key) DO UPDATE SET value = excluded.value, expires_at = NULL;",
            params![ns, key, bytes],
        )?;

        tx.commit()?;
        Ok(new_val)
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
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT value FROM kv_store WHERE namespace = 'stats' AND key = ?1;",
        )?;
        let mut rows = stmt.query(params![key.as_bytes()])?;
        if let Some(row) = rows.next()? {
            let bytes: Vec<u8> = row.get(0)?;
            if bytes.len() == 8 {
                let arr: [u8; 8] = bytes.as_slice().try_into().unwrap();
                Ok(Some(i64::from_be_bytes(arr)))
            } else {
                let s = String::from_utf8_lossy(&bytes);
                Ok(s.trim().parse::<i64>().ok())
            }
        } else {
            Ok(None)
        }
    }
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

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
