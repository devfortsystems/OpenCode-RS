use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub name: String,
    pub timestamp_str: String,
    pub files: HashMap<PathBuf, String>,
}

pub struct CheckpointManager {
    pub root_dir: PathBuf,
    pub snapshots: Vec<Snapshot>,
}

impl CheckpointManager {
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            root_dir,
            snapshots: Vec::new(),
        }
    }

    /// Tworzy nowy punkt kontrolny wszystkich plików źródłowych projektu
    pub fn create_checkpoint(&mut self, name: &str) -> Result<String> {
        let mut files = HashMap::new();
        Self::collect_files(&self.root_dir, &self.root_dir, &mut files, 4)?;

        let now = chrono::Local::now();
        let timestamp_str = now.format("%Y-%m-%d %H:%M:%S").to_string();

        let snapshot = Snapshot {
            name: name.to_string(),
            timestamp_str: timestamp_str.clone(),
            files,
        };

        let file_count = snapshot.files.len();
        self.snapshots.push(snapshot);

        Ok(format!(
            "Zapisano punkt kontrolny [{}] ({} plików) o {}",
            name, file_count, timestamp_str
        ))
    }

    /// Przywraca stan projektu z ostatniego punktu kontrolnego
    pub fn rollback_latest(&self) -> Result<String> {
        if let Some(snapshot) = self.snapshots.last() {
            let mut restored_count = 0;
            for (path, content) in &snapshot.files {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).ok();
                }
                fs::write(path, content)?;
                restored_count += 1;
            }
            Ok(format!(
                "Pomyślnie cofnięto zmiany do punktu [{}] (przywrócono {} plików z {})",
                snapshot.name, restored_count, snapshot.timestamp_str
            ))
        } else {
            Err(anyhow!("Brak zapisanych punktów kontrolnych do przywrócenia"))
        }
    }

    /// Rekurencyjnie zbiera pliki źródłowe
    fn collect_files(
        dir: &Path,
        root: &Path,
        files: &mut HashMap<PathBuf, String>,
        max_depth: usize,
    ) -> Result<()> {
        if max_depth == 0 || !dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if name == ".git" || name == "target" || name == "node_modules" || name.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                Self::collect_files(&path, root, files, max_depth - 1)?;
            } else if path.is_file() {
                // Czytaj tylko pliki tekstowe do 500KB
                if let Ok(meta) = entry.metadata() {
                    if meta.len() < 500 * 1024 {
                        if let Ok(content) = fs::read_to_string(&path) {
                            files.insert(path, content);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
