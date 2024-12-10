use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Instant, Duration};
use std::collections::HashSet;
use std::fs;
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use crossbeam_channel::bounded;
use parking_lot::RwLock;
use num_cpus;

use log::{info, warn, debug, error};
use tokio::sync::Mutex;
use tantivy::{Index, IndexWriter, schema::*, Document};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::directory::MmapDirectory;
use serde::Serialize;
use crate::file_system::FileInfo;

// Performance-optimized constants
const COMMIT_BATCH_SIZE: usize = 100_000; // Reduced for more frequent commits
const INDEX_BUFFER_SIZE: usize = 2_000_000_000; // 2GB buffer for better memory usage
const CHANNEL_SIZE: usize = 1_000_000; // Reduced channel size
const PROGRESS_UPDATE_INTERVAL: u64 = 500; // Increased to reduce overhead
const PROCESSOR_BATCH_SIZE: usize = 10_000; // Reduced batch size
const MAX_CONCURRENT_INDEXERS: usize = 4; // Reduced for better resource usage
const MAX_CONCURRENT_SCANNERS: usize = 1; // Single scanner to reduce contention
const SCAN_QUEUE_SIZE: usize = 50_000; // Reduced queue size
const SCAN_BATCH_SIZE: usize = 500; // Smaller batches
const SCAN_YIELD_THRESHOLD: usize = 5_000; // More frequent yields
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(15); // Reduced timeout
const ERROR_RETRY_DELAY: Duration = Duration::from_millis(100); // New constant for error retries
const MAX_ERROR_RETRIES: usize = 3; // New constant for max retries

#[derive(Debug, Clone, Serialize)]
pub struct SearchDoc {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub size_formatted: String,
    pub modified_formatted: String,
    pub mime_type: String,
    pub is_dir: bool,
    pub matches: Option<Vec<SearchMatch>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchMatch {
    pub line: usize,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexingState {
    pub total_files: usize,
    pub processed_files: usize,
    pub current_file: String,
    pub is_complete: bool,
    pub state: String,
    pub files_found: usize,
    pub start_time: u64,
    pub speed: u64,
    pub phase: String,
}

impl Default for IndexingState {
    fn default() -> Self {
        Self {
            total_files: 0,
            processed_files: 0,
            current_file: String::new(),
            is_complete: false,
            state: "Initializing".to_string(),
            files_found: 0,
            start_time: 0,
            speed: 0,
            phase: "Scanning".to_string(),
        }
    }
}

pub struct IndexManager {
    schema: Schema,
    index: Index,
    writer: Arc<Mutex<IndexWriter>>,
    fields: SchemaFields,
    state: Arc<RwLock<IndexingState>>,
    indexed_paths: Arc<RwLock<HashSet<String>>>,
    buffer_size: usize,
}

#[derive(Clone)]
pub struct SchemaFields {
    pub name: Field,
    pub path: Field,
    pub content: Field,
    pub size: Field,
    pub modified: Field,
    pub created: Field,
    pub mime_type: Field,
    pub extension: Field,
}

impl IndexManager {
    pub async fn new() -> Result<Self, String> {
        // Create schema
        let mut schema_builder = Schema::builder();
        
        // Add fields with appropriate options
        let name = schema_builder.add_text_field("name", TEXT | STORED);
        let path = schema_builder.add_text_field("path", TEXT | STORED);
        let content = schema_builder.add_text_field("content", TEXT);
        let size = schema_builder.add_text_field("size", TEXT | STORED);
        let modified = schema_builder.add_text_field("modified", TEXT | STORED);
        let created = schema_builder.add_text_field("created", TEXT | STORED);
        let mime_type = schema_builder.add_text_field("mime_type", TEXT | STORED);
        let extension = schema_builder.add_text_field("extension", TEXT | STORED);
        
        let schema = schema_builder.build();
        
        let fields = SchemaFields {
            name,
            path,
            content,
            size,
            modified,
            created,
            mime_type,
            extension,
        };
        
        // Get app data directory for index storage
        let app_dir = tauri::api::path::app_data_dir(&tauri::Config::default())
            .ok_or_else(|| "Failed to get app data directory".to_string())?;
            
        let index_path = app_dir.join("index");
        
        // Create index directory if it doesn't exist
        if !index_path.exists() {
            fs::create_dir_all(&index_path)
                .map_err(|e| format!("Failed to create index directory: {}", e))?;
        }
        
        // Create or open index
        let dir = MmapDirectory::open(&index_path)
            .map_err(|e| format!("Failed to open index directory: {}", e))?;
            
        let index = Index::open_or_create(dir, schema.clone())
            .map_err(|e| format!("Failed to create/open index: {}", e))?;
            
        let writer = index.writer(INDEX_BUFFER_SIZE)
            .map_err(|e| format!("Failed to create index writer: {}", e))?;
            
        let state = Arc::new(RwLock::new(IndexingState {
            total_files: 0,
            processed_files: 0,
            current_file: String::new(),
            state: "Ready".to_string(),
            is_complete: false,
            files_found: 0,
            start_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            speed: 0,
            phase: "Scanning".to_string(),
        }));
        
        Ok(Self {
            schema,
            index,
            writer: Arc::new(Mutex::new(writer)),
            fields,
            state,
            indexed_paths: Arc::new(RwLock::new(HashSet::new())),
            buffer_size: INDEX_BUFFER_SIZE,
        })
    }

    // Helper function to prepare document
    fn prepare_document(fields: &SchemaFields, file_info: &FileInfo) -> Document {
        let mut doc = Document::new();
        
        // Fast document preparation with capacity hints
        doc.add_text(fields.path, file_info.path.to_string_lossy().to_string());
        doc.add_text(fields.name, &file_info.name);
        doc.add_text(fields.size, file_info.size.to_string());
        
        if let Some(mime) = &file_info.mime_type {
            doc.add_text(fields.mime_type, mime);
        }
        
        if let Some(modified) = &file_info.modified {
            if let Ok(modified_str) = modified.duration_since(std::time::UNIX_EPOCH) {
                doc.add_text(fields.modified, modified_str.as_secs().to_string());
            }
        }
        
        doc
    }

    fn prepare_document_batch(fields: &SchemaFields, file_infos: &[FileInfo]) -> Vec<Document> {
        file_infos.iter().map(|file_info| {
            let mut doc = Document::new();
            
            // Fast document preparation without allocations
            doc.add_text(fields.path, file_info.path.to_string_lossy());
            doc.add_text(fields.name, &file_info.name);
            doc.add_text(fields.size, file_info.size.to_string());
            
            if let Some(mime) = &file_info.mime_type {
                doc.add_text(fields.mime_type, mime);
            }
            
            if let Some(modified) = &file_info.modified {
                if let Ok(modified_str) = modified.duration_since(std::time::UNIX_EPOCH) {
                    doc.add_text(fields.modified, modified_str.as_secs().to_string());
                }
            }
            
            doc
        }).collect()
    }

    pub async fn start_indexing<F>(&mut self, directory: PathBuf, progress_callback: F) -> Result<(), String>
    where
        F: Fn(&IndexingState) + Send + Sync + Clone + 'static,
    {
        debug!("Starting optimized indexing for directory: {:?}", directory);
        let start_time = Instant::now();
        
        let (tx, rx) = bounded::<Vec<FileInfo>>(SCAN_QUEUE_SIZE);
        let (doc_tx, doc_rx) = bounded::<Vec<Document>>(SCAN_QUEUE_SIZE);
        
        let processed_count = Arc::new(AtomicUsize::new(0));
        let total_count = Arc::new(AtomicUsize::new(0));
        let phase = Arc::new(RwLock::new(String::from("Scanning")));
        let error_count = Arc::new(AtomicUsize::new(0));
        let is_complete = Arc::new(AtomicBool::new(false));
        let should_stop = Arc::new(AtomicBool::new(false));
        
        // Memory-efficient progress tracking
        let progress_handle = {
            let progress_callback = progress_callback.clone();
            let processed_count = Arc::clone(&processed_count);
            let total_count = Arc::clone(&total_count);
            let phase = Arc::clone(&phase);
            let error_count = Arc::clone(&error_count);
            let start = start_time.clone();
            let should_stop = Arc::clone(&should_stop);
            
            tokio::spawn(async move {
                let mut last_processed = 0;
                let mut last_time = Instant::now();
                let mut consecutive_same_count = 0;
                let mut last_error_count = 0;
                
                while !should_stop.load(Ordering::Relaxed) {
                    tokio::time::sleep(tokio::time::Duration::from_millis(PROGRESS_UPDATE_INTERVAL)).await;
                    
                    let current_processed = processed_count.load(Ordering::Relaxed);
                    let current_total = total_count.load(Ordering::Relaxed);
                    let current_phase = phase.read().clone();
                    let current_errors = error_count.load(Ordering::Relaxed);
                    let now = Instant::now();
                    
                    // Detect stalls and errors
                    if current_processed == last_processed {
                        consecutive_same_count += 1;
                        if consecutive_same_count > 20 {
                            warn!("Processing stalled - {} files processed, {} total, {} errors", 
                                current_processed, current_total, current_errors);
                            
                            if current_errors > last_error_count {
                                warn!("New errors detected during stall");
                            }
                        }
                    } else {
                        consecutive_same_count = 0;
                    }
                    
                    // Improved speed calculation with error rate
                    let elapsed = now.duration_since(last_time).as_secs_f64();
                    let files_processed = current_processed.saturating_sub(last_processed);
                    let speed = if elapsed > 0.0 { (files_processed as f64 / elapsed) as u64 } else { 0 };
                    
                    let error_rate = if current_processed > 0 {
                        (current_errors as f64 / current_processed as f64) * 100.0
                    } else {
                        0.0
                    };
                    
                    // Enhanced progress state
                    let state = match current_phase.as_str() {
                        "Scanning" => format!("Scanning... (found {} files)", current_total),
                        "Processing" => {
                            if error_rate > 0.0 {
                                format!("Processing files ({} files/sec, {:.1}% error rate)", 
                                    speed, error_rate)
                            } else {
                                format!("Processing files ({} files/sec)", speed)
                            }
                        }
                        _ => current_phase.clone(),
                    };
                    
                    progress_callback(&IndexingState {
                        total_files: current_total,
                        processed_files: if current_phase == "Processing" { current_processed } else { 0 },
                        current_file: String::new(),
                        is_complete: false,
                        state,
                        files_found: current_total,
                        start_time: start.elapsed().as_secs(),
                        speed,
                        phase: current_phase,
                    });
                    
                    last_processed = current_processed;
                    last_time = now;
                    last_error_count = current_errors;
                }
            })
        };
        
        // Optimized document writer with error recovery
        let writer = self.writer.clone();
        let writer_handle = tokio::spawn({
            let should_stop = Arc::clone(&should_stop);
            let error_count = Arc::clone(&error_count);
            
            async move {
                let mut current_batch = Vec::with_capacity(COMMIT_BATCH_SIZE);
                let mut retry_count = 0;
                
                while let Ok(mut docs) = doc_rx.recv() {
                    if should_stop.load(Ordering::Relaxed) {
                        break;
                    }
                    
                    // Efficient batch processing
                    current_batch.extend(docs.drain(..));
                    
                    if current_batch.len() >= COMMIT_BATCH_SIZE {
                        let mut success = false;
                        
                        // Retry loop for resilient writes
                        while !success && retry_count < MAX_ERROR_RETRIES {
                            let mut writer_guard = writer.lock().await;
                            
                            let batch_result = {
                                let mut has_error = false;
                                for doc in current_batch.drain(..) {
                                    if let Err(e) = writer_guard.add_document(doc) {
                                        has_error = true;
                                        error_count.fetch_add(1, Ordering::Relaxed);
                                        warn!("Failed to add document: {}", e);
                                        break;
                                    }
                                }
                                if has_error { Err("Failed to add documents".to_string()) } else { Ok(()) }
                            };
                            
                            match batch_result {
                                Ok(_) => {
                                    if let Err(e) = writer_guard.commit() {
                                        warn!("Commit failed (attempt {}): {}", retry_count + 1, e);
                                        retry_count += 1;
                                        error_count.fetch_add(1, Ordering::Relaxed);
                                        tokio::time::sleep(ERROR_RETRY_DELAY).await;
                                    } else {
                                        success = true;
                                        retry_count = 0;
                                    }
                                }
                                Err(e) => {
                                    warn!("Batch write failed (attempt {}): {}", retry_count + 1, e);
                                    retry_count += 1;
                                    error_count.fetch_add(1, Ordering::Relaxed);
                                    tokio::time::sleep(ERROR_RETRY_DELAY).await;
                                }
                            }
                            
                            // Release lock before delay
                            drop(writer_guard);
                        }
                        
                        if !success {
                            error!("Failed to write batch after {} attempts", MAX_ERROR_RETRIES);
                            should_stop.store(true, Ordering::Release);
                            break;
                        }
                    }
                }
                
                // Final cleanup with timeout
                if !current_batch.is_empty() {
                    let cleanup_timeout = tokio::time::sleep(CLEANUP_TIMEOUT);
                    tokio::pin!(cleanup_timeout);
                    
                    let cleanup_result = tokio::select! {
                        _ = async {
                            let mut writer_guard = writer.lock().await;
                            for doc in current_batch.drain(..) {
                                if let Err(e) = writer_guard.add_document(doc) {
                                    error!("Failed to add document in final batch: {}", e);
                                }
                            }
                            if let Err(e) = writer_guard.commit() {
                                error!("Failed to commit final batch: {}", e);
                            }
                            // Drop the guard to release the lock before waiting
                            drop(writer_guard);
                        } => Ok(()),
                        _ = &mut cleanup_timeout => {
                            error!("Final cleanup timed out");
                            Err(())
                        }
                    };
                    
                    if cleanup_result.is_err() {
                        should_stop.store(true, Ordering::Release);
                    }
                }
            }
        });
        
        // Optimized file scanner with CPU throttling
        let scanner_handle = std::thread::spawn({
            let tx = tx.clone();
            let total_count = Arc::clone(&total_count);
            let phase = Arc::clone(&phase);
            let should_stop = Arc::clone(&should_stop);
            
            move || {
                let batch = Vec::with_capacity(SCAN_BATCH_SIZE);
                let files_since_yield = Arc::new(AtomicUsize::new(0));
                
                let walker = ignore::WalkBuilder::new(&directory)
                    .hidden(false)
                    .ignore(false)
                    .git_ignore(false)
                    .threads(MAX_CONCURRENT_SCANNERS)
                    .build_parallel();
                
                walker.run(|| {
                    let tx = tx.clone();
                    let total_count = Arc::clone(&total_count);
                    let should_stop = Arc::clone(&should_stop);
                    let files_since_yield = Arc::clone(&files_since_yield);
                    let mut local_batch: Vec<FileInfo> = Vec::with_capacity(PROCESSOR_BATCH_SIZE);
                    
                    Box::new(move |entry| {
                        if should_stop.load(Ordering::Relaxed) {
                            return ignore::WalkState::Quit;
                        }

                        let entry = match entry {
                            Ok(entry) => entry,
                            Err(_) => return ignore::WalkState::Continue,
                        };
                        
                        let path = entry.path().to_owned();
                        if let Ok(metadata) = fs::metadata(&path) {
                            if !metadata.is_dir() {
                                total_count.fetch_add(1, Ordering::Relaxed);
                                files_since_yield.fetch_add(1, Ordering::Relaxed);
                                
                                let file_info = FileInfo {
                                    path: path.clone(),
                                    name: path.file_name()
                                            .map(|n| n.to_string_lossy().to_string())
                                            .unwrap_or_default(),
                                    size: metadata.len(),
                                    modified: metadata.modified().ok(),
                                    created: metadata.created().ok(),
                                    is_dir: false,
                                    mime_type: mime_guess::from_path(&path).first().map(|m| m.to_string()),
                                    content: None,
                                };
                                
                                local_batch.push(file_info);
                                
                                // Send batch if full using drain for efficiency
                                if local_batch.len() >= SCAN_BATCH_SIZE {
                                    let mut batch: Vec<FileInfo> = Vec::with_capacity(SCAN_BATCH_SIZE);
                                    batch.extend(local_batch.drain(..));
                                    
                                    // Try to send with timeout and backoff
                                    let mut backoff: u64 = 1;
                                    while !should_stop.load(Ordering::Relaxed) {
                                        match tx.try_send(batch) {
                                            Ok(_) => {
                                                break;
                                            }
                                            Err(crossbeam_channel::TrySendError::Full(returned_batch)) => {
                                                batch = returned_batch;
                                                std::thread::sleep(Duration::from_millis(backoff));
                                                backoff = (backoff * 2).min(100); // Exponential backoff capped at 100ms
                                            }
                                            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                                                return ignore::WalkState::Quit;
                                            }
                                        }
                                    }
                                }
                                
                                // Yield to other tasks periodically with adaptive delay
                                let current_files = files_since_yield.load(Ordering::Relaxed);
                                if current_files >= SCAN_YIELD_THRESHOLD {
                                    let yield_duration = if total_count.load(Ordering::Relaxed) > 100_000 {
                                        Duration::from_millis(5) // Longer yields for large directories
                                    } else {
                                        Duration::from_millis(1)
                                    };
                                    std::thread::sleep(yield_duration);
                                    files_since_yield.store(0, Ordering::Relaxed);
                                }
                            }
                        }
                        ignore::WalkState::Continue
                    })
                });
                
                // Send remaining files
                if !batch.is_empty() && !should_stop.load(Ordering::Relaxed) {
                    while !should_stop.load(Ordering::Relaxed) && tx.send(batch.clone()).is_err() {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                
                *phase.write() = String::from("Processing");
            }
        });
        
        // Spawn optimized document processors
        let thread_count = num_cpus::get().min(MAX_CONCURRENT_INDEXERS);
        let doc_processors: Vec<_> = (0..thread_count).map(|_| {
            let doc_tx = doc_tx.clone();
            let processed_count = Arc::clone(&processed_count);
            let rx = rx.clone();
            let fields = self.fields.clone();
            
            std::thread::spawn(move || {
                let docs_batch: Vec<Document> = Vec::with_capacity(PROCESSOR_BATCH_SIZE);
                let mut consecutive_errors = 0;
                let mut total_errors = 0;
                
                while let Ok(batch) = rx.recv() {
                    // Process in smaller chunks for better responsiveness
                    for chunk in batch.chunks(PROCESSOR_BATCH_SIZE / 4) {
                        let docs = Self::prepare_document_batch(&fields, chunk);
                        processed_count.fetch_add(chunk.len(), Ordering::Relaxed);
                        
                        // Try to send with backoff on error
                        let mut backoff = 1;
                        let mut retry_count = 0;
                        let mut docs_to_send = docs;
                        
                        while retry_count < MAX_ERROR_RETRIES {
                            match doc_tx.try_send(docs_to_send) {
                                Ok(_) => {
                                    consecutive_errors = 0;
                                    total_errors = 0;
                                    break;
                                }
                                Err(crossbeam_channel::TrySendError::Full(returned_docs)) => {
                                    docs_to_send = returned_docs;
                                    std::thread::sleep(Duration::from_millis(backoff));
                                    backoff = (backoff * 2).min(100);
                                    retry_count += 1;
                                }
                                Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                                    warn!("Document channel disconnected");
                                    return;
                                }
                            }
                        }
                        
                        // Handle retry failures
                        if retry_count >= MAX_ERROR_RETRIES {
                            consecutive_errors += 1;
                            total_errors += 1;
                            warn!("Failed to send documents after {} retries", MAX_ERROR_RETRIES);
                            
                            // Break if too many errors
                            if consecutive_errors > 5 || total_errors > 20 {
                                error!("Too many errors in document processor (consecutive: {}, total: {})", 
                                    consecutive_errors, total_errors);
                                return;
                            }
                        }
                    }
                }
            })
        }).collect();
        
        // Wait for scanner to complete
        if let Err(e) = scanner_handle.join() {
            warn!("Scanner thread panicked: {:?}", e);
        }
        
        // Close channels to stop workers
        drop(tx);
        
        // Wait for document processors
        for handle in doc_processors {
            if let Err(e) = handle.join() {
                warn!("Document processor thread panicked: {:?}", e);
            }
        }
        
        // Close document channel and wait for writer
        drop(doc_tx);
        if let Err(e) = writer_handle.await {
            warn!("Writer task failed: {:?}", e);
        }
        
        // Stop progress updates
        progress_handle.abort();
        
        let elapsed = start_time.elapsed();
        let processed = processed_count.load(Ordering::Relaxed);
        let final_speed = (processed as f64 / elapsed.as_secs_f64()) as u64;
        
        info!("Indexing completed in {:.2} seconds. Processed {} files ({:.0} files/sec).", 
            elapsed.as_secs_f64(),
            processed,
            final_speed
        );
        
        // Final progress update
        progress_callback(&IndexingState {
            total_files: processed,
            processed_files: processed,
            current_file: String::new(),
            is_complete: true,
            state: "Complete".to_string(),
            files_found: processed,
            start_time: start_time.elapsed().as_secs(),
            speed: final_speed,
            phase: "Complete".to_string(),
        });
        
        // Wait for cleanup before marking as complete
        let writer = self.writer.clone();
        let cleanup_handle = tokio::spawn(async move {
            let result = async {
                let mut writer_guard = writer.lock().await;
                
                // Create new writer for replacement
                let temp_writer = writer_guard.index()
                    .writer(1024)
                    .map_err(|e| format!("Failed to create temp writer: {}", e))?;
                
                // Replace the writer and take ownership of the old one
                let mut old_writer = std::mem::replace(&mut *writer_guard, temp_writer);
                drop(writer_guard); // Release the lock before cleanup
                
                // Perform cleanup on the old writer with retries
                let mut retry_count = 0;
                
                while retry_count < MAX_ERROR_RETRIES {
                    match old_writer.commit() {
                        Ok(_) => break,
                        Err(e) => {
                            warn!("Commit failed during cleanup (attempt {}): {}", retry_count + 1, e);
                            retry_count += 1;
                            if retry_count >= MAX_ERROR_RETRIES {
                                return Err(format!("Failed to commit after {} retries", MAX_ERROR_RETRIES));
                            }
                            tokio::time::sleep(ERROR_RETRY_DELAY).await;
                        }
                    }
                }
                
                // Wait for merging threads with timeout
                let merge_timeout = tokio::time::sleep(CLEANUP_TIMEOUT);
                tokio::pin!(merge_timeout);
                
                let merge_result = tokio::select! {
                    result = tokio::task::spawn_blocking(move || old_writer.wait_merging_threads()) => {
                        match result {
                            Ok(Ok(_)) => Ok(()),
                            Ok(Err(e)) => Err(format!("Merging threads error: {}", e)),
                            Err(e) => Err(format!("Blocking task error: {}", e))
                        }
                    }
                    _ = merge_timeout => {
                        warn!("Merging threads timeout after {} seconds", CLEANUP_TIMEOUT.as_secs());
                        Err("Merging threads timeout".to_string())
                    }
                };

                merge_result
            }.await;
            
            if let Err(ref e) = result {
                error!("Cleanup failed: {}", e);
            }
            result
        });

        // Final cleanup with timeout
        let cleanup_timeout = tokio::time::sleep(CLEANUP_TIMEOUT);
        tokio::pin!(cleanup_timeout);

        match tokio::select! {
            result = cleanup_handle => result.map_err(|e| format!("Cleanup task failed: {}", e))?,
            _ = cleanup_timeout => {
                warn!("Final cleanup timed out after {} seconds", CLEANUP_TIMEOUT.as_secs());
                should_stop.store(true, Ordering::Release);
                Ok(())
            }
        } {
            Ok(_) => {
                is_complete.store(true, Ordering::Release);
                Ok(())
            }
            Err(e) => {
                warn!("Final cleanup error: {}", e);
                Err(e)
            }
        }
    }

    pub async fn get_stats(&self) -> Result<String, String> {
        let reader = self.index.reader()
            .map_err(|e| format!("Failed to get index reader: {}", e))?;
        let searcher = reader.searcher();
        let num_docs = searcher.num_docs();
        Ok(format!("Index contains {} documents", num_docs))
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchDoc>, String> {
        info!("Executing search query: {}", query);
        
        let reader = self.index
            .reader()
            .map_err(|e| format!("Failed to get index reader: {}", e))?;
            
        let searcher = reader.searcher();
        
        // Create a query parser that searches in name, path, and content fields
        let mut query_parser = QueryParser::for_index(&self.index, vec![
            self.fields.name,
            self.fields.path,
            self.fields.content,
            self.fields.extension,
            self.fields.mime_type
        ]);
        
        // Set field boosts
        query_parser.set_field_boost(self.fields.name, 3.0);
        query_parser.set_field_boost(self.fields.path, 2.0);
        query_parser.set_field_boost(self.fields.content, 1.0);
        
        // Parse and execute the query
        let query = query_parser
            .parse_query(query)
            .map_err(|e| format!("Failed to parse query: {}", e))?;
            
        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(100))
            .map_err(|e| format!("Search failed: {}", e))?;
            
        let mut results = Vec::new();
        
        // Convert search results to SearchDoc structs
        for (_score, doc_address) in top_docs {
            let retrieved_doc = searcher
                .doc(doc_address)
                .map_err(|e| format!("Failed to retrieve document: {}", e))?;
                
            let path = retrieved_doc
                .get_first(self.fields.path)
                .and_then(|f| f.as_text())
                .ok_or_else(|| "Document missing path field".to_string())?
                .to_string();
                
            let name = retrieved_doc
                .get_first(self.fields.name)
                .and_then(|f| f.as_text())
                .ok_or_else(|| "Document missing name field".to_string())?
                .to_string();
                
            let size = retrieved_doc
                .get_first(self.fields.size)
                .and_then(|f| f.as_text())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
                
            let mime_type = retrieved_doc
                .get_first(self.fields.mime_type)
                .and_then(|f| f.as_text())
                .unwrap_or("")
                .to_string();
                
            let is_dir = mime_type.is_empty();
                
            results.push(SearchDoc {
                path,
                name,
                size,
                size_formatted: Self::format_size(size),
                modified_formatted: "Unknown".to_string(), // TODO: Format from timestamp
                mime_type,
                is_dir,
                matches: None, // TODO: Add context matches
            });
        }
        
        Ok(results)
    }

    fn format_size(size: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;
        const TB: u64 = GB * 1024;

        if size >= TB {
            format!("{:.2} TB", size as f64 / TB as f64)
        } else if size >= GB {
            format!("{:.2} GB", size as f64 / GB as f64)
        } else if size >= MB {
            format!("{:.2} MB", size as f64 / MB as f64)
        } else if size >= KB {
            format!("{:.2} KB", size as f64 / KB as f64)
        } else {
            format!("{} B", size)
        }
    }

    pub async fn add_document(&self, path: PathBuf) -> Result<(), String> {
        let mut writer = self.writer.lock().await;
        let file_info = FileInfo::from_path(&path)?;
        let doc = Self::prepare_document(&self.fields, &file_info);
        
        writer.add_document(doc)
            .map_err(|e| format!("Failed to add document: {}", e))?;

        // Use a more efficient batching approach with a counter
        let doc_count = self.indexed_paths.read().len();
        
        if doc_count >= COMMIT_BATCH_SIZE {
            writer.commit()
                .map_err(|e| format!("Failed to commit: {}", e))?;
        }
        
        Ok(())
    }
} 