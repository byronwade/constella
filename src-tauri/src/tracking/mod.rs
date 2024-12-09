use std::path::PathBuf;
use std::collections::HashMap;
use std::time::{SystemTime, Duration};
use tokio::sync::RwLock;
use blake3::Hash;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileState {
    size: u64,
    modified: SystemTime,
    hash: Option<Hash>,  // Content hash for important files
    last_indexed: SystemTime,
    last_checked: SystemTime,
    change_frequency: Duration,  // Adaptive tracking of how often this file changes
    importance_score: f32,  // Dynamic score based on file usage and changes
}

pub struct ChangeTracker {
    states: RwLock<HashMap<PathBuf, FileState>>,
    index_frequency: RwLock<AdaptiveFrequency>,
}

#[derive(Debug)]
struct AdaptiveFrequency {
    min_interval: Duration,
    max_interval: Duration,
    current_load: f32,
    system_resources: SystemResources,
}

impl ChangeTracker {
    pub fn new() -> Self {
        Self {
            states: RwLock::new(HashMap::new()),
            index_frequency: RwLock::new(AdaptiveFrequency::new()),
        }
    }

    pub async fn should_reindex(&self, path: &PathBuf, metadata: &std::fs::Metadata) -> bool {
        let mut states = self.states.write().await;
        let now = SystemTime::now();

        if let Some(state) = states.get(path) {
            // Quick check for obvious changes
            if metadata.len() != state.size || metadata.modified().unwrap() != state.modified {
                return true;
            }

            // Check if enough time has passed based on file's change frequency
            if now.duration_since(state.last_checked).unwrap() < state.change_frequency {
                return false;
            }

            // For important files, verify content hash
            if state.importance_score > 0.8 {
                if let Some(current_hash) = self.compute_hash(path).await {
                    if let Some(stored_hash) = state.hash {
                        return current_hash != stored_hash;
                    }
                }
            }

            // Adaptive reindexing based on system load and file importance
            let freq = self.index_frequency.read().await;
            if freq.should_skip_indexing(state.importance_score) {
                return false;
            }
        }

        true
    }

    pub async fn update_state(&self, path: &PathBuf, metadata: &std::fs::Metadata, indexed: bool) {
        let mut states = self.states.write().await;
        let now = SystemTime::now();

        let state = states.entry(path.clone()).or_insert_with(|| FileState {
            size: metadata.len(),
            modified: metadata.modified().unwrap(),
            hash: None,
            last_indexed: now,
            last_checked: now,
            change_frequency: Duration::from_secs(3600), // Start with 1 hour
            importance_score: 0.5, // Start with medium importance
        });

        if indexed {
            // Update state after indexing
            state.size = metadata.len();
            state.modified = metadata.modified().unwrap();
            state.last_indexed = now;
            
            // Compute hash for important files
            if state.importance_score > 0.8 {
                state.hash = self.compute_hash(path).await;
            }

            // Adapt change frequency based on actual changes
            self.adapt_change_frequency(state).await;
        }

        state.last_checked = now;
    }

    async fn adapt_change_frequency(&self, state: &mut FileState) {
        let time_since_last = SystemTime::now()
            .duration_since(state.last_indexed)
            .unwrap_or(Duration::from_secs(0));

        // If file changed more frequently than expected
        if time_since_last < state.change_frequency {
            state.change_frequency = time_since_last + (time_since_last / 2);
            state.importance_score = (state.importance_score + 0.1).min(1.0);
        } else {
            // File changes less frequently than expected
            state.change_frequency = state.change_frequency + (state.change_frequency / 2);
            state.importance_score = (state.importance_score - 0.05).max(0.0);
        }
    }

    async fn compute_hash(&self, path: &PathBuf) -> Option<Hash> {
        tokio::fs::read(path).await
            .ok()
            .map(|content| blake3::hash(&content))
    }
}

impl AdaptiveFrequency {
    fn new() -> Self {
        Self {
            min_interval: Duration::from_secs(1),
            max_interval: Duration::from_secs(3600),
            current_load: 0.0,
            system_resources: SystemResources::new(),
        }
    }

    fn should_skip_indexing(&self, importance: f32) -> bool {
        // Skip indexing if system is under heavy load
        if self.system_resources.is_under_heavy_load() {
            return importance < 0.9; // Only index critical files under heavy load
        }

        // Adaptive indexing based on system resources and file importance
        let threshold = self.calculate_threshold();
        importance < threshold
    }

    fn calculate_threshold(&self) -> f32 {
        // Adjust threshold based on system load
        let base_threshold = 0.2;
        base_threshold + (self.current_load * 0.6)
    }
}

struct SystemResources {
    last_check: SystemTime,
    cpu_usage: f32,
    memory_usage: f32,
    io_usage: f32,
}

impl SystemResources {
    fn new() -> Self {
        Self {
            last_check: SystemTime::now(),
            cpu_usage: 0.0,
            memory_usage: 0.0,
            io_usage: 0.0,
        }
    }

    fn is_under_heavy_load(&self) -> bool {
        self.cpu_usage > 0.8 || self.memory_usage > 0.9 || self.io_usage > 0.7
    }

    fn update(&mut self) {
        // Update system resource metrics
        if let Ok(cpu) = sysinfo::System::new_all().cpu_usage() {
            self.cpu_usage = cpu / 100.0;
        }
        // Update memory and IO metrics similarly
    }
} 