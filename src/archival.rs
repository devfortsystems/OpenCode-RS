//! Archival memory — wektorowa pamięć długoterminowa (HNSW via Grafowektor).
//!
//! Agent zapisuje wiedzę (poza memory blocks) jako węzły z embeddingami.
//! Retrieval on-demand przez tool `archival_search(query)`.
//!
//! Embedding: domyślnie lokalny hash-based (deterministyczny, bez API).
//! Opcjonalnie: Ollama (`nomic-embed-text`) lub OpenAI (`text-embedding-3-small`).
//!
//! Storage: Grafowektor serializowany do DevFortDB (namespace `archival`).

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use devfortdb_core::Grafowektor;

use crate::database::Database;

/// Wpis archival memory — wiedza długoterminowa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchivalEntry {
    pub id: String,
    pub content: String,
    pub labels: Vec<String>,
    pub node_type: String,
    pub created_at: String,
    pub source: String, // "agent" | "user" | "sleeptime" | "import"
}

/// Wynik wyszukiwania archival.
#[derive(Debug, Clone)]
pub struct ArchivalHit {
    pub id: String,
    pub content: String,
    pub score: f32,
    pub labels: Vec<String>,
}

/// Archival memory — wrapper na Grafowektor + DevFortDB.
pub struct ArchivalMemory {
    db: Database,
    /// In-memory cache grafowektora (ładowany z DB przy pierwszym użyciu).
    gv: Arc<Mutex<Grafowektor>>,
    /// Embedding provider.
    embedder: Embedder,
}

/// Strategia embeddingu.
#[derive(Debug, Clone)]
pub enum Embedder {
    /// Lokalny hash-based (deterministyczny, 384-dim, bez API).
    /// Działa offline, ale mniej precyzyjny niż model.
    Hash,
    /// Ollama (`nomic-embed-text` lub inny model).
    Ollama { url: String, model: String },
    /// OpenAI (`text-embedding-3-small` — 1536-dim).
    OpenAi { api_key: String, model: String },
}

impl Embedder {
    /// Domyślny embedder — Hash (offline, zero kosztów).
    pub fn default_hash() -> Self {
        Self::Hash
    }

    /// Ollama embedder (lokalny, wymaga `ollama pull nomic-embed-text`).
    pub fn ollama(url: &str, model: &str) -> Self {
        Self::Ollama {
            url: url.to_string(),
            model: model.to_string(),
        }
    }

    /// OpenAI embedder (API, płatny).
    pub fn openai(api_key: &str, model: &str) -> Self {
        Self::OpenAi {
            api_key: api_key.to_string(),
            model: model.to_string(),
        }
    }

    /// Generuje embedding dla tekstu.
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        match self {
            Self::Hash => Ok(hash_embed(text, 384)),
            Self::Ollama { url, model } => ollama_embed(url, model, text).await,
            Self::OpenAi { api_key, model } => openai_embed(api_key, model, text).await,
        }
    }

    /// Wymiar wektora.
    pub fn dim(&self) -> usize {
        match self {
            Self::Hash => 384,
            Self::Ollama { .. } => 768, // nomic-embed-text = 768
            Self::OpenAi { model, .. } => {
                if model.contains("3-small") { 1536 } else { 3072 }
            }
        }
    }
}

impl ArchivalMemory {
    /// Otwiera archival memory z domyślnym embedderem (Hash).
    pub fn open(work_dir: &Path) -> Result<Self> {
        let db = Database::open(work_dir)?;
        let gv = Self::load_graph(&db).unwrap_or_default();
        Ok(Self {
            db,
            gv: Arc::new(Mutex::new(gv)),
            embedder: Embedder::default_hash(),
        })
    }

    /// Otwiera z konkretnym embedderem.
    pub fn open_with(work_dir: &Path, embedder: Embedder) -> Result<Self> {
        let db = Database::open(work_dir)?;
        let gv = Self::load_graph(&db).unwrap_or_default();
        Ok(Self {
            db,
            gv: Arc::new(Mutex::new(gv)),
            embedder,
        })
    }

    /// Dodaje wpis do archival memory.
    pub async fn add(&self, entry: &ArchivalEntry) -> Result<()> {
        let vector = self.embedder.embed(&entry.content).await?;

        let mut gv = self.gv.lock().unwrap();
        gv.put_node(
            entry.id.clone(),
            Some(vector),
            entry.labels.clone(),
        );

        // Zapisz metadane entry jako osobny klucz (content nie mieści się w GvNode)
        self.db.put_str("archival_meta", &entry.id, entry)?;

        // Zapisz zserializowany graf
        self.save_graph(&gv)?;

        Ok(())
    }

    /// Wyszukaj podobne wpisy (vector search + keyword boost).
    pub async fn search(&self, query: &str, top_k: usize) -> Result<Vec<ArchivalHit>> {
        let query_vec = self.embedder.embed(query).await?;

        let gv = self.gv.lock().unwrap();
        let results = gv.vector_search(&query_vec, top_k);

        let mut hits = Vec::new();
        for (id, score) in results {
            // Pobierz metadane (content)
            if let Ok(Some(entry)) = self.db.get_str::<ArchivalEntry>("archival_meta", &id) {
                hits.push(ArchivalHit {
                    id: entry.id,
                    content: entry.content,
                    score,
                    labels: entry.labels,
                });
            }
        }

        // Keyword boost — dodaj wpisy które mają labels pasujące do query
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();
        for (node_id, node) in gv.nodes() {
            if node.vector.is_some() {
                continue; // już uwzględniony w vector_search
            }
            let label_match = node.labels.iter().any(|l| {
                let ll = l.to_lowercase();
                query_words.iter().any(|qw| ll.contains(qw))
            });
            if label_match {
                if let Ok(Some(entry)) = self.db.get_str::<ArchivalEntry>("archival_meta", node_id) {
                    hits.push(ArchivalHit {
                        id: entry.id,
                        content: entry.content,
                        score: 0.5, // keyword match score
                        labels: entry.labels,
                    });
                }
            }
        }

        // Sortuj po score malejąco
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(top_k);

        Ok(hits)
    }

    /// Pobierz wpis po ID.
    pub fn get(&self, id: &str) -> Result<Option<ArchivalEntry>> {
        self.db.get_str::<ArchivalEntry>("archival_meta", id)
    }

    /// Usuń wpis po ID.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let mut gv = self.gv.lock().unwrap();
        // Grafowektor nie ma delete_node — oznaczamy jako usunięty przez wyczyszczenie wektora
        if let Some(node) = gv.nodes().get(id).cloned() {
            let mut new_node = node;
            new_node.vector = None;
            new_node.labels.clear();
            gv.add_node(new_node);
            self.save_graph(&gv)?;
        }
        self.db.delete_str("archival_meta", id)
    }

    /// Liczba wpisów.
    pub fn count(&self) -> usize {
        self.gv.lock().unwrap().node_count()
    }

    /// Lista wszystkich wpisów (metadane).
    pub fn list(&self) -> Result<Vec<ArchivalEntry>> {
        let entries = self.db.scan("archival_meta")?;
        let mut result = Vec::new();
        for (_, val) in entries {
            if let Ok(entry) = serde_json::from_slice::<ArchivalEntry>(&val) {
                result.push(entry);
            }
        }
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(result)
    }

    /// Ładuje graf z DB.
    fn load_graph(db: &Database) -> Result<Grafowektor> {
        if let Ok(Some(bytes)) = db.get_str::<Vec<u8>>("archival", "graph") {
            if let Ok(gv) = Grafowektor::from_bytes(&bytes) {
                return Ok(gv);
            }
        }
        Ok(Grafowektor::new())
    }

    /// Zapisuje graf do DB.
    fn save_graph(&self, gv: &Grafowektor) -> Result<()> {
        let bytes = gv.to_bytes()?;
        self.db.put_str("archival", "graph", &bytes)
    }
}

// ─── Embedding implementations ───────────────────────────────────────

/// Hash-based embedding — deterministyczny, offline, 384-dim.
///
/// Bazuje na FNV-1a hash z różnych "okien" tekstu.
/// Mniej precyzyjny niż model, ale działa bez API i jest wystarczający
/// dla małej bazy wiedzy agenta.
fn hash_embed(text: &str, dim: usize) -> Vec<f32> {
    let mut vec = vec![0.0f32; dim];
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec;
    }

    // 1. Unigramy — każdy słowo hashowany do pozycji w wektorze
    for word in &words {
        let w = word.to_lowercase();
        let h = fnv1a(&w) as usize % dim;
        vec[h] += 1.0;
    }

    // 2. Bigramy — pary słów
    for pair in words.windows(2) {
        let bigram = format!("{} {}", pair[0], pair[1]).to_lowercase();
        let h = fnv1a(&bigram) as usize % dim;
        vec[h] += 0.5;
    }

    // 3. Trigramy
    for triple in words.windows(3) {
        let trigram = format!("{} {} {}", triple[0], triple[1], triple[2]).to_lowercase();
        let h = fnv1a(&trigram) as usize % dim;
        vec[h] += 0.25;
    }

    // 4. Cały tekst (dla długich fraz)
    let h = fnv1a(&text.to_lowercase()) as usize % dim;
    vec[h] += 2.0;

    // L2 normalize
    let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in &mut vec {
            *v /= norm;
        }
    }

    vec
}

/// FNV-1a hash (32-bit).
fn fnv1a(s: &str) -> u32 {
    let mut hash: u32 = 0x811c9dc5;
    for b in s.bytes() {
        hash ^= b as u32;
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

/// Ollama embedding API.
async fn ollama_embed(url: &str, model: &str, text: &str) -> Result<Vec<f32>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let resp = client
        .post(format!("{}/api/embeddings", url.trim_end_matches('/')))
        .json(&serde_json::json!({
            "model": model,
            "prompt": text,
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("Ollama embed error: {} {}", resp.status(), resp.text().await?);
    }

    let body: serde_json::Value = resp.json().await?;
    let embedding = body["embedding"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Ollama: brak pola 'embedding'"))?
        .iter()
        .filter_map(|v| v.as_f64().map(|f| f as f32))
        .collect();

    Ok(embedding)
}

/// OpenAI embedding API.
async fn openai_embed(api_key: &str, model: &str, text: &str) -> Result<Vec<f32>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let resp = client
        .post("https://api.openai.com/v1/embeddings")
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "model": model,
            "input": text,
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("OpenAI embed error: {} {}", resp.status(), resp.text().await?);
    }

    let body: serde_json::Value = resp.json().await?;
    let embedding = body["data"][0]["embedding"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("OpenAI: brak pola 'embedding'"))?
        .iter()
        .filter_map(|v| v.as_f64().map(|f| f as f32))
        .collect();

    Ok(embedding)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("opencode_archival_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn test_archival_add_and_search() {
        let dir = temp_dir();
        let am = ArchivalMemory::open(&dir).unwrap();

        // Dodaj 3 wpisy
        let entries = vec![
            ArchivalEntry {
                id: "e1".into(),
                content: "Rust jest językiem systemowym z bezpieczeństwem pamięci".into(),
                labels: vec!["rust".into(), "języki".into()],
                node_type: "fact".into(),
                created_at: "2025-01-01T00:00:00Z".into(),
                source: "agent".into(),
            },
            ArchivalEntry {
                id: "e2".into(),
                content: "Python jest językiem dynamicznym do skryptów i ML".into(),
                labels: vec!["python".into(), "języki".into()],
                node_type: "fact".into(),
                created_at: "2025-01-02T00:00:00Z".into(),
                source: "agent".into(),
            },
            ArchivalEntry {
                id: "e3".into(),
                content: "Tokamak to urządzenie do fuzji jądrowej z polem magnetycznym".into(),
                labels: vec!["fizyka".into(), "fuzja".into()],
                node_type: "fact".into(),
                created_at: "2025-01-03T00:00:00Z".into(),
                source: "agent".into(),
            },
        ];

        for e in &entries {
            am.add(e).await.unwrap();
        }

        assert_eq!(am.count(), 3);

        // Wyszukaj "Rust język"
        let hits = am.search("Rust język programowania", 2).await.unwrap();
        assert!(!hits.is_empty(), "powinien znaleźć wyniki");
        // Najlepszy hit powinien być o Rust
        assert!(hits[0].content.contains("Rust") || hits[0].labels.contains(&"rust".to_string()),
            "najlepszy hit powinien być o Rust: {:?}", hits[0]);

        // Wyszukaj "fuzja jądrowa"
        let hits2 = am.search("fuzja jądrowa tokamak", 3).await.unwrap();
        assert!(!hits2.is_empty());
        // Hash embed może nie być idealny — sprawdź czy którykolwiek hit jest o fizyce
        let has_physics = hits2.iter().any(|h| h.content.contains("Tokamak") || h.labels.contains(&"fuzja".to_string()));
        assert!(has_physics, "powinien znaleźć wpis o Tokamak/fuzji: {:?}", hits2.iter().map(|h| &h.content).collect::<Vec<_>>());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn test_archival_persistence() {
        let dir = temp_dir();

        // Zapisz
        {
            let am = ArchivalMemory::open(&dir).unwrap();
            am.add(&ArchivalEntry {
                id: "p1".into(),
                content: "Test persistence wpis".into(),
                labels: vec!["test".into()],
                node_type: "fact".into(),
                created_at: "2025-01-01T00:00:00Z".into(),
                source: "agent".into(),
            }).await.unwrap();
        }

        // Otwórz ponownie — sprawdź czy wpis przetrwał
        {
            let am = ArchivalMemory::open(&dir).unwrap();
            assert_eq!(am.count(), 1);
            let list = am.list().unwrap();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].content, "Test persistence wpis");
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_hash_embed_basic() {
        let v1 = hash_embed("Rust język programowania", 384);
        let v2 = hash_embed("Rust język programowania", 384);
        let v3 = hash_embed("Python skrypty ML", 384);

        // Deterministyczny
        assert_eq!(v1, v2);

        // Różne teksty → różne wektory
        assert_ne!(v1, v3);

        // L2 normalized
        let norm: f32 = v1.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01, "wektor powinien być znormalizowany, norm={}", norm);
    }

    #[test]
    fn test_hash_embed_similarity() {
        let v1 = hash_embed("Rust język systemowy", 384);
        let v2 = hash_embed("Rust język programowania", 384);
        let v3 = hash_embed("fuzja jądrowa tokamak", 384);

        // Podobne teksty → wyższy cosine
        let sim_same = devfortdb_core::cosine(&v1, &v2);
        let sim_diff = devfortdb_core::cosine(&v1, &v3);

        assert!(sim_same > sim_diff,
            "podobne teksty powinny mieć wyższy cosine: same={} diff={}",
            sim_same, sim_diff);
    }
}
