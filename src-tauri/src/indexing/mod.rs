use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tantivy::{Index, IndexWriter, Document, Term, schema::*, IndexSettings};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::directory::MmapDirectory;
use log::{info, warn, debug, error};
use std::fs;
use std::collections::HashSet;
use std::time::{SystemTime, Instant};
use crate::file_system::{FileSystem, FileInfo};
use serde::Serialize;
use rayon::prelude::*;

#[derive(Debug, Clone, Serialize)]
pub struct SearchDoc {
    pub path: String,
    pub name: String,
    pub size_formatted: String,
    pub modified_formatted: String,
    pub mime_type: String,
    pub extension: String,
    pub permissions: String,
    pub content: Option<String>,
}

#[derive(Clone)]
pub struct SchemaFields {
    pub name: Field,
    pub path: Field,
    pub content: Field,
    pub size: Field,
    pub size_formatted: Field,
    pub modified: Field,
    pub modified_formatted: Field,
    pub created: Field,
    pub mime_type: Field,
    pub extension: Field,
    pub permissions: Field,
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
}

#[derive(Clone)]
pub struct IndexManager {
    schema: Schema,
    index: Arc<Index>,
    writer: Arc<Mutex<IndexWriter>>,
    fields: SchemaFields,
    indexing_state: Arc<Mutex<IndexingState>>,
    indexed_paths: Arc<Mutex<HashSet<PathBuf>>>,
}

impl IndexManager {
    pub async fn new() -> Result<Self, String> {
        info!("Creating new IndexManager");
        
        // Create schema
        let mut schema_builder = Schema::builder();
        
        // Define fields with appropriate options
        let fields = SchemaFields {
            name: schema_builder.add_text_field("name", TEXT | STORED | FAST),
            path: schema_builder.add_text_field("path", TEXT | STORED | FAST),
            content: schema_builder.add_text_field("content", TEXT | STORED),
            size: schema_builder.add_text_field("size", STORED),
            size_formatted: schema_builder.add_text_field("size_formatted", STORED),
            modified: schema_builder.add_text_field("modified", STORED),
            modified_formatted: schema_builder.add_text_field("modified_formatted", STORED),
            created: schema_builder.add_text_field("created", STORED),
            mime_type: schema_builder.add_text_field("mime_type", TEXT | STORED),
            extension: schema_builder.add_text_field("extension", TEXT | STORED),
            permissions: schema_builder.add_text_field("permissions", STORED),
        };
        
        let schema = schema_builder.build();
        
        // Get app data directory for index storage
        let app_data_dir = tauri::api::path::app_data_dir(&tauri::Config::default())
            .ok_or_else(|| "Failed to get app data directory".to_string())?;
            
        let index_path = app_data_dir.join("index");
        info!("Using index path: {:?}", index_path);
        
        // Always remove the old index to ensure schema compatibility
        if index_path.exists() {
            info!("Removing old index to ensure schema compatibility");
            if let Err(e) = std::fs::remove_dir_all(&index_path) {
                warn!("Failed to remove old index: {}", e);
            }
        }
        
        // Create index directory
        fs::create_dir_all(&index_path)
            .map_err(|e| format!("Failed to create index directory: {}", e))?;
            
        // Create new index
        let dir = MmapDirectory::open(&index_path)
            .map_err(|e| format!("Failed to open index directory: {}", e))?;
            
        info!("Creating new index at: {:?}", index_path);
        let index = Index::create(
            dir,
            schema.clone(),
            IndexSettings::default()
        ).map_err(|e| format!("Failed to create index: {}", e))?;
            
        let writer = index.writer(50_000_000)
            .map_err(|e| format!("Failed to create index writer: {}", e))?;
            
        let indexing_state = Arc::new(Mutex::new(IndexingState {
            total_files: 0,
            processed_files: 0,
            current_file: String::new(),
            is_complete: false,
            state: "Ready".to_string(),
            files_found: 0,
            start_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }));

        // Initialize empty indexed paths
        let indexed_paths = Arc::new(Mutex::new(HashSet::new()));
        
        Ok(Self {
            schema,
            index: Arc::new(index),
            writer: Arc::new(Mutex::new(writer)),
            fields,
            indexing_state,
            indexed_paths,
        })
    }
    
    pub async fn start_indexing<F>(&self, directory: PathBuf, progress_callback: F) -> Result<(), String>
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
        
        let files = fs.scan_directory(&directory, |count| {
            let mut state = futures::executor::block_on(self.indexing_state.lock());
            state.files_found = count;
            progress_callback(&state);
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
        
        info!("Getting index writer lock...");
        let writer = Arc::clone(&self.writer);
        
        // Process files in parallel using rayon
        let chunk_size = 1000;
        let chunks: Vec<_> = files.chunks(chunk_size).collect();
        
        let mut total_bytes_indexed = 0u64;
        let mut skipped_files = 0;
        let mut error_files = 0;
        
        for (chunk_index, chunk) in chunks.into_iter().enumerate() {
            let chunk_start = Instant::now();
            debug!("Processing chunk {}/{} ({} files)", 
                   chunk_index + 1, 
                   (files.len() + chunk_size - 1) / chunk_size,
                   chunk.len());
            
            // Process chunk in parallel
            let docs: Vec<_> = chunk.par_iter().filter_map(|file_info| {
                // Update current file in state
                let mut state = futures::executor::block_on(self.indexing_state.lock());
                state.current_file = file_info.path.to_string_lossy().to_string();
                state.processed_files += 1;
                progress_callback(&state);
                drop(state);
                
                // Index the file
                match futures::executor::block_on(self.index_file(&file_info)) {
                    Ok(doc) => {
                        total_bytes_indexed += file_info.size;
                        Some(doc)
                    },
                    Err(e) => {
                        error!("Failed to index file {:?}: {}", file_info.path, e);
                        error_files += 1;
                        None
                    }
                }
            }).collect();
            
            let chunk_duration = chunk_start.elapsed();
            let files_per_second = chunk.len() as f32 / chunk_duration.as_secs_f32();
            
            debug!("Chunk {}/{} completed in {:.2}s ({:.2} files/s)", 
                   chunk_index + 1,
                   (files.len() + chunk_size - 1) / chunk_size,
                   chunk_duration.as_secs_f32(),
                   files_per_second);
            
            // Add documents to index
            let mut writer_lock = futures::executor::block_on(writer.lock());
            for doc in docs {
                if let Err(e) = writer_lock.add_document(doc) {
                    error!("Failed to add document to index: {}", e);
                    error_files += 1;
                }
            }
            
            // Commit every chunk
            if let Err(e) = writer_lock.commit() {
                error!("Failed to commit chunk: {}", e);
            }
        }
        
        let total_duration = start_time.elapsed();
        info!("Indexing completed in {:.2}s", total_duration.as_secs_f32());
        info!("Total files processed: {}", files.len());
        info!("Total bytes indexed: {:.2} MB", total_bytes_indexed as f32 / 1_000_000.0);
        info!("Average speed: {:.2} files/s", 
              files.len() as f32 / total_duration.as_secs_f32());
        if error_files > 0 {
            warn!("Files with errors: {}", error_files);
        }
        if skipped_files > 0 {
            info!("Files skipped: {}", skipped_files);
        }
        
        // Mark indexing as complete
        let mut state = self.indexing_state.lock().await;
        state.is_complete = true;
        state.state = "Complete".to_string();
        progress_callback(&state);
        
        Ok(())
    }
    
    async fn get_document_by_path(&self, path: &PathBuf) -> Result<Document, String> {
        let reader = self.index.reader()
            .map_err(|e| format!("Failed to get index reader: {}", e))?;
        let searcher = reader.searcher();
        
        let path_str = path.to_string_lossy();
        let term = Term::from_field_text(self.fields.path, &path_str);
        let query = tantivy::query::TermQuery::new(term, IndexRecordOption::Basic);
        
        let top_docs = searcher.search(&query, &TopDocs::with_limit(1))
            .map_err(|e| format!("Failed to search for document: {}", e))?;
            
        if let Some((_score, doc_address)) = top_docs.first() {
            searcher.doc(*doc_address)
                .map_err(|e| format!("Failed to retrieve document: {}", e))
        } else {
            Err("Document not found".to_string())
        }
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
            } else {
                debug!("Skipping binary content for {:?} ({})", file_info.path, mime_type);
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
        query_parser.set_field_boost(self.fields.extension, 1.0);
        query_parser.set_field_boost(self.fields.mime_type, 1.0);
        
        // Parse the query
        let query = query_parser
            .parse_query(query)
            .map_err(|e| format!("Failed to parse query: {}", e))?;
            
        // Execute the search
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
                
            let size_formatted = retrieved_doc
                .get_first(self.fields.size_formatted)
                .and_then(|f| f.as_text())
                .unwrap_or("Unknown size")
                .to_string();
                
            let modified_formatted = retrieved_doc
                .get_first(self.fields.modified_formatted)
                .and_then(|f| f.as_text())
                .unwrap_or("Unknown date")
                .to_string();
                
            let mime_type = retrieved_doc
                .get_first(self.fields.mime_type)
                .and_then(|f| f.as_text())
                .unwrap_or("")
                .to_string();
                
            let extension = retrieved_doc
                .get_first(self.fields.extension)
                .and_then(|f| f.as_text())
                .unwrap_or("")
                .to_string();
                
            let permissions = retrieved_doc
                .get_first(self.fields.permissions)
                .and_then(|f| f.as_text())
                .unwrap_or("Unknown permissions")
                .to_string();
                
            let content = retrieved_doc
                .get_first(self.fields.content)
                .and_then(|f| f.as_text())
                .map(|s| s.to_string());
                
            results.push(SearchDoc {
                path,
                name,
                size_formatted,
                modified_formatted,
                mime_type,
                extension,
                permissions,
                content,
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