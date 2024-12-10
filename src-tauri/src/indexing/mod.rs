use std::sync::Arc;
use std::fs;
use parking_lot::RwLock;
use tokio::sync::Mutex;
use log::{info, error, warn};
use tantivy::{Index, IndexWriter, schema::*, Document};
use tantivy::query::QueryParser;
use tantivy::collector::TopDocs;
use std::time::{UNIX_EPOCH, SystemTime};
use serde_json;
use serde::Serialize;

const INDEX_BUFFER_SIZE: usize = 100_000_000; // 100MB buffer for better performance
const COMMIT_BATCH_SIZE: usize = 10_000; // Larger batches for better throughput
const MAX_RETRY_ATTEMPTS: usize = 3;
const CHANNEL_BUFFER_SIZE: usize = 100_000; // Large channel buffer for better throughput

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexState {
    Idle,
    Scanning,
    Indexing,
    Completed,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct IndexerState {
    pub total_files: usize,
    pub processed_files: usize,
    pub current_file: String,
    pub state: String,
    pub files_per_second: f32,
    pub elapsed_seconds: u64,
    pub start_time: SystemTime,
}

pub struct Indexer {
    index: Index,
    writer: Arc<Mutex<Option<IndexWriter>>>,
    state: Arc<RwLock<IndexerState>>,
    path_field: Field,
    modified_field: Field,
    size_field: Field,
}

impl Indexer {
    pub fn new() -> Result<Self, String> {
        info!("Creating new Indexer instance");
        let mut schema_builder = Schema::builder();

        let path_field = schema_builder.add_text_field("path", TEXT | STORED);
        let modified_field = schema_builder.add_u64_field("modified", STORED | FAST);
        let size_field = schema_builder.add_u64_field("size", STORED | FAST);

        let schema = schema_builder.build();
        info!("Schema built with fields: path, modified, size");

        let app_data_dir = tauri::api::path::app_data_dir(&tauri::Config::default())
            .ok_or_else(|| "Failed to get app data directory".to_string())?;
        let index_path = app_data_dir.join("search_index");
        
        std::fs::create_dir_all(&index_path)
            .map_err(|e| format!("Failed to create index directory: {}", e))?;

        let index = if index_path.exists() {
            info!("Opening existing index at {:?}", index_path);
            Index::open_in_dir(&index_path)
                .map_err(|e| format!("Failed to open existing index: {}", e))?
        } else {
            info!("Creating new index at {:?}", index_path);
            Index::create_in_dir(&index_path, schema)
                .map_err(|e| format!("Failed to create index: {}", e))?
        };

        Ok(Self {
            index,
            writer: Arc::new(Mutex::new(None)),
            state: Arc::new(RwLock::new(IndexerState {
                total_files: 0,
                processed_files: 0,
                current_file: String::new(),
                state: "idle".to_string(),
                files_per_second: 0.0,
                elapsed_seconds: 0,
                start_time: SystemTime::now(),
            })),
            path_field,
            modified_field,
            size_field,
        })
    }

    pub fn get_state(&self) -> IndexerState {
        let mut state = self.state.write();
        let elapsed = state.start_time.elapsed().unwrap_or_default();
        let elapsed_secs = elapsed.as_secs();
        
        // Calculate files per second
        if elapsed_secs > 0 {
            state.files_per_second = state.processed_files as f32 / elapsed_secs as f32;
        } else {
            state.files_per_second = 0.0;
        }
        
        state.elapsed_seconds = elapsed_secs;
        state.clone()
    }

    pub async fn get_reader(&self) -> tantivy::Result<tantivy::IndexReader> {
        self.index.reader()
    }

    pub async fn update_state<F>(&self, update_fn: F) -> Result<(), String>
    where
        F: FnOnce(&mut IndexerState),
    {
        let mut state = self.state.write();
        update_fn(&mut state);

        // Ensure processed files never exceeds total files
        if state.processed_files > state.total_files {
            state.processed_files = state.total_files;
        }

        // Update files per second
        let elapsed = state.start_time.elapsed().unwrap_or_default();
        let elapsed_secs = elapsed.as_secs();
        if elapsed_secs > 0 {
            state.files_per_second = state.processed_files as f32 / elapsed_secs as f32;
        }

        state.elapsed_seconds = elapsed_secs;
        Ok(())
    }

    pub async fn start_indexing(&self, path: impl AsRef<str>) -> Result<(), String> {
        let path = path.as_ref().to_string();
        info!("=== STARTING INDEXING PROCESS ===");
        info!("Target directory: {}", path);
        
        // Reset state and start scanning phase
        self.update_state(|state| {
            state.total_files = 0;
            state.processed_files = 0;
            state.current_file = format!("Scanning {}", path);
            state.state = "scanning".to_string();
            state.start_time = SystemTime::now();
        }).await?;

        // Clear existing index
        info!("Clearing existing index");
        let mut writer_guard = self.writer.lock().await;
        *writer_guard = Some(self.index.writer_with_num_threads(4, INDEX_BUFFER_SIZE)
            .map_err(|e| format!("Failed to create writer: {}", e))?);
        
        if let Some(writer) = writer_guard.as_mut() {
            writer.delete_all_documents()
                .map_err(|e| format!("Failed to clear index: {}", e))?;
            writer.commit()
                .map_err(|e| format!("Failed to commit index clearing: {}", e))?;
        }
        drop(writer_guard);

        // PHASE 1: Scanning
        info!("=== PHASE 1: SCANNING ===");
        info!("Starting scan of directory: {}", path);
        let scanner = crate::scanner::FileScanner::new();
        let total_files = scanner.scan_directory(&path).await;
        info!("Initial scan completed, found {} files", total_files);
        
        if total_files == 0 {
            error!("No files found in directory: {}", path);
            self.update_state(|state| {
                state.state = "completed".to_string();
                state.current_file = "No files found".to_string();
            }).await?;
            return Ok(());
        }

        // PHASE 2: Initialize indexing
        info!("=== PHASE 2: INDEXING ===");
        info!("Preparing to index {} files", total_files);
        self.update_state(|state| {
            state.total_files = total_files;
            state.processed_files = 0;
            state.state = "indexing".to_string();
            state.start_time = SystemTime::now(); // Reset timer for indexing phase
            state.current_file = "Starting indexing...".to_string();
        }).await?;

        // Initialize writer for indexing
        let mut writer_guard = self.writer.lock().await;
        *writer_guard = Some(self.index.writer_with_num_threads(4, INDEX_BUFFER_SIZE)
            .map_err(|e| format!("Failed to create writer: {}", e))?);
        drop(writer_guard);

        // PHASE 3: Process files
        info!("=== PHASE 3: COLLECTING PATHS ===");
        let mut batch = Vec::with_capacity(COMMIT_BATCH_SIZE);
        let mut processed = 0;

        // Get all paths to process
        let paths = scanner.collect_paths(&path);
        let total = paths.len();
        info!("Collected {} paths to index", total);

        if total != total_files {
            warn!("Path count mismatch: scan found {}, but collected {}", total_files, total);
        }

        // Process each file
        info!("=== PHASE 4: INDEXING FILES ===");
        for path in paths {
            let path_str = path.to_string_lossy().into_owned();
            info!("Processing file: {}", path_str);
            
            // Create and add document
            match self.create_document(&path) {
                Ok(doc) => {
                    batch.push(doc);
                    processed += 1;

                    // Update state
                    self.update_state(move |state| {
                        state.processed_files = processed;
                        state.current_file = path_str.clone();
                    }).await?;

                    // Commit batch if needed
                    if batch.len() >= COMMIT_BATCH_SIZE {
                        info!("Committing batch of {} documents", batch.len());
                        if let Err(e) = self.commit_batch(&mut batch).await {
                            error!("Failed to commit batch: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to create document for {}: {}", path_str, e);
                }
            }
        }

        // Commit any remaining documents
        if !batch.is_empty() {
            info!("Committing final batch of {} documents", batch.len());
            if let Err(e) = self.commit_batch(&mut batch).await {
                error!("Failed to commit final batch: {}", e);
            }
        }

        // Final state update
        self.update_state(|state| {
            state.state = "completed".to_string();
            state.processed_files = processed;
            state.current_file = "Indexing completed".to_string();
        }).await?;

        info!("=== INDEXING COMPLETED ===");
        info!("Total files processed: {}/{}", processed, total);
        Ok(())
    }

    async fn commit_batch(&self, batch: &mut Vec<Document>) -> Result<(), String> {
        let mut writer_guard = self.writer.lock().await;
        if let Some(writer) = writer_guard.as_mut() {
            for doc in batch.drain(..) {
                if let Err(e) = writer.add_document(doc) {
                    error!("Failed to add document: {}", e);
                }
            }
            writer.commit()
                .map_err(|e| format!("Failed to commit batch: {}", e))?;
        }
        Ok(())
    }

    fn create_document(&self, path: impl AsRef<std::path::Path>) -> Result<Document, String> {
        let path = path.as_ref();
        let mut doc = Document::default();
        
        // Get file metadata
        let metadata = fs::metadata(path)
            .map_err(|e| format!("Failed to get metadata for {}: {}", path.display(), e))?;
        
        // Add path
        doc.add_text(self.path_field, path.to_string_lossy().as_ref());
        
        // Add modified time
        let modified = metadata.modified()
            .map_err(|e| format!("Failed to get modified time: {}", e))?
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("Failed to calculate duration: {}", e))?
            .as_secs();
        doc.add_u64(self.modified_field, modified);
        
        // Add file size
        doc.add_u64(self.size_field, metadata.len());
        
        Ok(doc)
    }

    async fn recreate_writer(&self) -> Result<(), String> {
        let mut writer_guard = self.writer.lock().await;
        *writer_guard = Some(self.index.writer(INDEX_BUFFER_SIZE)
            .map_err(|e| format!("Failed to recreate writer: {}", e))?);
        Ok(())
    }

    pub async fn search(&self, query: &str) -> Result<Vec<serde_json::Value>, String> {
        let reader = self.get_reader().await
            .map_err(|e| format!("Failed to get reader: {}", e))?;
        
        let searcher = reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.path_field]);
        
        let query = query_parser.parse_query(query)
            .map_err(|e| format!("Failed to parse query: {}", e))?;
        
        let top_docs = searcher.search(&query, &TopDocs::with_limit(100))
            .map_err(|e| format!("Failed to execute search: {}", e))?;
        
        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let retrieved_doc = searcher.doc(doc_address)
                .map_err(|e| format!("Failed to retrieve document: {}", e))?;
            
            let path = retrieved_doc.get_first(self.path_field)
                .and_then(|f| f.as_text())
                .ok_or_else(|| "Document missing path field".to_string())?;
            
            let modified = retrieved_doc.get_first(self.modified_field)
                .and_then(|f| f.as_u64())
                .unwrap_or_default();
            
            let size = retrieved_doc.get_first(self.size_field)
                .and_then(|f| f.as_u64())
                .unwrap_or_default();
            
            let path_buf = std::path::PathBuf::from(path);
            let name = path_buf.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            
            let mut doc = serde_json::Map::new();
            doc.insert("path".to_string(), serde_json::Value::String(path.to_string()));
            doc.insert("name".to_string(), serde_json::Value::String(name.to_string()));
            doc.insert("size".to_string(), serde_json::Value::Number(serde_json::Number::from(size)));
            doc.insert("modified".to_string(), serde_json::Value::Number(serde_json::Number::from(modified)));
            
            // Convert score to f64 and handle the Option with a default value
            if let Some(score_num) = serde_json::Number::from_f64(score as f64) {
                doc.insert("score".to_string(), serde_json::Value::Number(score_num));
            } else {
                doc.insert("score".to_string(), serde_json::Value::Number(serde_json::Number::from(0)));
            }
            
            results.push(serde_json::Value::Object(doc));
        }
        
        Ok(results)
    }

    pub async fn cancel(&self) -> Result<(), String> {
        self.update_state(|state| {
            state.state = "completed".to_string();
        }).await
    }
} 