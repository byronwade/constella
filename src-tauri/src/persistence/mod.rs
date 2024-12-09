use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use tantivy::directory::MmapDirectory;
use crate::stats::IndexStats;
use crate::tracking::FileState;

#[derive(Debug)]
pub struct PersistenceManager {
    index_path: PathBuf,
    state_path: PathBuf,
    stats_path: PathBuf,
    state: Arc<RwLock<IndexState>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct IndexState {
    version: String,
    last_save: SystemTime,
    file_states: HashMap<PathBuf, FileState>,
    stats: IndexStats,
    config: IndexConfig,
}

#[derive(Debug, Serialize, Deserialize)]
struct IndexConfig {
    root_paths: Vec<PathBuf>,
    excluded_patterns: Vec<String>,
    max_file_size: u64,
    index_batch_size: usize,
    compression_enabled: bool,
}

impl PersistenceManager {
    pub fn new(base_path: impl AsRef<Path>) -> std::io::Result<Self> {
        let base_path = base_path.as_ref();
        std::fs::create_dir_all(base_path)?;

        Ok(Self {
            index_path: base_path.join("index"),
            state_path: base_path.join("state.json"),
            stats_path: base_path.join("stats.json"),
            state: Arc::new(RwLock::new(IndexState::default())),
        })
    }

    pub async fn save_index(&self, index: &tantivy::Index) -> std::io::Result<()> {
        let directory = MmapDirectory::open(&self.index_path)?;
        index.directory_mut().atomic_write(&directory)?;
        Ok(())
    }

    pub async fn load_index(&self) -> tantivy::Result<Option<tantivy::Index>> {
        if !self.index_path.exists() {
            return Ok(None);
        }

        let directory = MmapDirectory::open(&self.index_path)?;
        let index = tantivy::Index::open(directory)?;
        Ok(Some(index))
    }

    pub async fn save_state(&self, file_states: &HashMap<PathBuf, FileState>) -> std::io::Result<()> {
        let mut state = self.state.write().await;
        state.file_states = file_states.clone();
        state.last_save = SystemTime::now();

        let json = serde_json::to_string_pretty(&*state)?;
        tokio::fs::write(&self.state_path, json).await?;
        Ok(())
    }

    pub async fn save_stats(&self, stats: &IndexStats) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(stats)?;
        tokio::fs::write(&self.stats_path, json).await?;
        Ok(())
    }

    pub async fn load_state(&self) -> std::io::Result<Option<IndexState>> {
        if !self.state_path.exists() {
            return Ok(None);
        }

        let json = tokio::fs::read_to_string(&self.state_path).await?;
        let state = serde_json::from_str(&json)?;
        Ok(Some(state))
    }
} 