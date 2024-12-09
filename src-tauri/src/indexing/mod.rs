use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tantivy::{Index, IndexWriter, schema::*, Document};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::directory::MmapDirectory;
use log::{info, warn, error, debug};
use std::fs;
use std::collections::HashSet;
use std::time::Instant;
use crate::file_system::{FileSystem, FileInfo};
use serde::Serialize;
use futures::future;

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
    pub state: String,
    pub is_complete: bool,
    pub files_found: usize,
    pub start_time: u64,
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

pub struct IndexManager {
    schema: Schema,
    index: Arc<Index>,
    writer: Arc<Mutex<IndexWriter>>,
    fields: SchemaFields,
    indexing_state: Arc<Mutex<IndexingState>>,
    indexed_paths: Arc<Mutex<HashSet<String>>>,
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
            
        let writer = index.writer(50_000_000)
            .map_err(|e| format!("Failed to create index writer: {}", e))?;
            
        let indexing_state = Arc::new(Mutex::new(IndexingState {
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
        }));
        
        Ok(Self {
            schema,
            index: Arc::new(index),
            writer: Arc::new(Mutex::new(writer)),
            fields,
            indexing_state,
            indexed_paths: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    pub async fn start_indexing<F>(&mut self, directory: PathBuf, progress_callback: F) -> Result<(), String>
    where
        F: Fn(&IndexingState) + Send + Sync + 'static + Clone,
    {
        let start_time = Instant::now();
        info!("Starting indexing for directory: {:?}", directory);
        
        // Reset indexing state
        let mut state = self.indexing_state.lock().await;
        state.total_files = 0;
        state.processed_files = 0;
        state.current_file = String::new();
        state.is_complete = false;
        state.state = "Scanning".to_string();
        state.files_found = 0;
        drop(state);
        
        // Create new file system scanner
        let fs = FileSystem::new();
        
        info!("Scanning directory for files...");
        let scan_start = Instant::now();
        
        // Clone necessary references for the closure
        let indexing_state = Arc::clone(&self.indexing_state);
        let progress_callback_scan = progress_callback.clone();
        
        let files = fs.scan_directory(&directory, move |count| {
            let mut state = futures::executor::block_on(indexing_state.lock());
            state.files_found = count;
            progress_callback_scan(&state);
        }).await?;
        
        let scan_duration = scan_start.elapsed();
        info!("Directory scan completed in {:.2}s, found {} files", 
              scan_duration.as_secs_f32(), 
              files.len());
        
        // Update total files count
        let mut state = self.indexing_state.lock().await;
        state.total_files = files.len();
        state.state = "Indexing".to_string();
        progress_callback(&state);
        drop(state);
        
        // Process files in chunks
        let chunk_size = 1000;
        let total_chunks = (files.len() + chunk_size - 1) / chunk_size;
        let chunks: Vec<_> = files.chunks(chunk_size).collect();
        
        for (chunk_index, chunk) in chunks.into_iter().enumerate() {
            let chunk_start = Instant::now();
            info!("Processing chunk {}/{}", chunk_index + 1, total_chunks);
            
            // Process chunk in parallel using futures
            let mut docs = Vec::new();
            let mut futures = Vec::new();
            
            for file_info in chunk {
                // Update current file in state
                let mut state = self.indexing_state.lock().await;
                state.current_file = file_info.path.to_string_lossy().to_string();
                state.processed_files += 1;
                progress_callback(&state);
                drop(state);
                
                // Index the file
                futures.push(self.index_file(file_info));
            }
            
            // Wait for all files in chunk to be processed
            for result in future::join_all(futures).await {
                match result {
                    Ok(doc) => docs.push(doc),
                    Err(e) => error!("Failed to index file: {}", e),
                }
            }
            
            // Add documents to index
            let mut writer = self.writer.lock().await;
            for doc in docs {
                if let Err(e) = writer.add_document(doc) {
                    error!("Failed to add document to index: {}", e);
                }
            }
            
            // Commit after each chunk
            if let Err(e) = writer.commit() {
                error!("Failed to commit chunk: {}", e);
            }
            
            let chunk_duration = chunk_start.elapsed();
            info!("Chunk processed in {:.2}s", chunk_duration.as_secs_f32());
        }
        
        // Mark as complete
        let mut state = self.indexing_state.lock().await;
        state.is_complete = true;
        state.state = "Complete".to_string();
        progress_callback(&state);
        
        let total_duration = start_time.elapsed();
        info!("Indexing completed in {:.2}s", total_duration.as_secs_f32());
        
        Ok(())
    }

    async fn index_file(&self, file_info: &FileInfo) -> Result<Document, String> {
        let mut doc = Document::new();
        
        // Add basic file metadata
        doc.add_text(self.fields.path, file_info.path.to_string_lossy().to_string());
        doc.add_text(self.fields.name, &file_info.name);
        doc.add_text(self.fields.size, file_info.size.to_string());
        
        // Add time fields with proper error handling
        if let Some(modified) = file_info.modified {
            match modified.duration_since(std::time::UNIX_EPOCH) {
                Ok(duration) => {
                    doc.add_text(self.fields.modified, duration.as_secs().to_string());
                },
                Err(e) => {
                    warn!("Invalid modification time for {:?}: {}", file_info.path, e);
                }
            }
        }
        
        // Handle content based on file type
        if !file_info.is_dir {
            let mime_type = file_info.mime_type.as_deref().unwrap_or("application/octet-stream");
            doc.add_text(self.fields.mime_type, mime_type);
            
            // Index content for text files
            if mime_type.starts_with("text/") || matches!(mime_type, 
                "application/json" | 
                "application/javascript" | 
                "application/xml" |
                "application/x-yaml" |
                "application/x-toml"
            ) {
                debug!("Indexing text content for {:?} ({})", file_info.path, mime_type);
                match tokio::fs::read_to_string(&file_info.path).await {
                    Ok(content) => {
                        // Skip if content is too large (>10MB)
                        if content.len() > 10_000_000 {
                            warn!("Skipping large file content for {:?} ({} bytes)", 
                                  file_info.path, content.len());
                        } else {
                            doc.add_text(self.fields.content, &content);
                        }
                    },
                    Err(e) => {
                        warn!("Failed to read content for {:?}: {}", file_info.path, e);
                    }
                }
            }
        }
        
        // Add file extension
        if let Some(ext) = file_info.path.extension() {
            if let Some(ext_str) = ext.to_str() {
                doc.add_text(self.fields.extension, ext_str.to_lowercase());
            }
        }
        
        Ok(doc)
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
            let mime_type_clone = mime_type.clone();
                
            results.push(SearchDoc {
                path,
                name,
                size,
                size_formatted: format_size(size),
                modified_formatted: "Unknown".to_string(), // TODO: Format from timestamp
                mime_type: mime_type_clone,
                is_dir,
                matches: None, // TODO: Add context matches
            });
        }
        
        Ok(results)
    }

    pub async fn verify_index(&self) -> Result<String, String> {
        info!("Verifying index...");
        
        let reader = self.index
            .reader()
            .map_err(|e| format!("Failed to get index reader: {}", e))?;
            
        let searcher = reader.searcher();
        let num_docs = searcher.num_docs();
        
        Ok(format!("Index contains {} documents", num_docs))
    }
}

fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size >= GB {
        format!("{:.2} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.2} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.2} KB", size as f64 / KB as f64)
    } else {
        format!("{} B", size)
    }
} 