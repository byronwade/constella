use std::sync::Arc;
use std::fs;
use std::path::Path;
use parking_lot::RwLock;
use tokio::sync::Mutex;
use log::{info, error, debug};
use tantivy::{Index, IndexWriter, schema::*, Document};
use tantivy::directory::MmapDirectory;
use tantivy::query::QueryParser;
use tantivy::collector::TopDocs;
use std::time::UNIX_EPOCH;

const INDEX_BUFFER_SIZE: usize = 50_000_000; // 50MB
const COMMIT_INTERVAL: usize = 1000; // Commit every 1000 documents

// System directories and files to skip
const SKIP_PATHS: &[&str] = &[
    "/usr",
    "/System",
    "/Library",
    "/private",
    "/dev",
    "/bin",
    "/sbin",
    "/opt",
    "/var",
    "/etc",
    "/.VolumeIcon.icns", // Skip volume icon explicitly
    "/.Spotlight-V100",  // Skip Spotlight index
    "/.fseventsd",       // Skip FSEvents
];

fn should_skip_path(path: &Path) -> bool {
    // Skip hidden files and directories
    if path.file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.starts_with('.'))
        .unwrap_or(false) {
        return true;
    }

    // Skip system directories and special files
    if let Some(path_str) = path.to_str() {
        if SKIP_PATHS.iter().any(|skip| path_str.starts_with(skip)) {
            debug!("Skipping system path: {}", path_str);
            return true;
        }
    }

    false
}

#[derive(Debug)]
pub struct SchemaFields {
    pub path: Field,
    pub content: Field,
    pub file_name: Field,
    pub file_type: Field,
    pub modified: Field,
    pub size: Field,
}

pub struct IndexingState {
    pub total_files: usize,
    pub processed_files: usize,
    pub current_file: String,
    pub state: String,
    pub is_complete: bool,
    pub files_found: usize,
    pub start_time: u64,
}

impl Default for IndexingState {
    fn default() -> Self {
        Self {
            total_files: 0,
            processed_files: 0,
            current_file: String::new(),
            state: "Idle".to_string(),
            is_complete: false,
            files_found: 0,
            start_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

pub struct IndexManager {
    fields: SchemaFields,
    index: Index,
    writer: Arc<Mutex<IndexWriter>>,
    state: Arc<RwLock<IndexingState>>,
}

impl IndexManager {
    pub fn new() -> Result<Self, String> {
        info!("Creating new IndexManager");
        let mut schema_builder = Schema::builder();

        // Define schema fields
        let path = schema_builder.add_text_field("path", TEXT | STORED);
        let content = schema_builder.add_text_field("content", TEXT);
        let file_name = schema_builder.add_text_field("file_name", TEXT | STORED);
        let file_type = schema_builder.add_text_field("file_type", TEXT | STORED);
        let modified = schema_builder.add_text_field("modified", TEXT | STORED);
        let size = schema_builder.add_text_field("size", TEXT | STORED);

        let schema = schema_builder.build();
        let fields = SchemaFields {
            path,
            content,
            file_name,
            file_type,
            modified,
            size,
        };

        info!("Schema built successfully");

        // Try to create or open the index directory
        let index_path = "index";
        if let Err(e) = fs::create_dir_all(index_path) {
            return Err(format!("Failed to create index directory: {}", e));
        }

        info!("Index directory created/verified");

        // Create or open the index
        let index = match MmapDirectory::open(index_path) {
            Ok(dir) => {
                match Index::open(dir) {
                    Ok(existing_index) => {
                        if existing_index.schema() != schema {
                            info!("Schema mismatch detected, recreating index");
                            // Remove old index
                            if let Err(e) = fs::remove_dir_all(index_path) {
                                return Err(format!("Failed to remove old index: {}", e));
                            }
                            if let Err(e) = fs::create_dir_all(index_path) {
                                return Err(format!("Failed to recreate index directory: {}", e));
                            }
                            // Create new index
                            Index::create_in_dir(index_path, schema)
                                .map_err(|e| format!("Failed to create new index: {}", e))?
                        } else {
                            info!("Using existing index with matching schema");
                            existing_index
                        }
                    },
                    Err(_) => {
                        info!("Creating new index");
                        Index::create_in_dir(index_path, schema)
                            .map_err(|e| format!("Failed to create new index: {}", e))?
                    }
                }
            },
            Err(e) => return Err(format!("Failed to open index directory: {}", e))
        };

        info!("Index opened/created successfully");

        // Create index writer
        let writer = index.writer(INDEX_BUFFER_SIZE)
            .map_err(|e| format!("Failed to create index writer: {}", e))?;

        info!("Index writer created successfully");

        Ok(Self {
            fields,
            index,
            writer: Arc::new(Mutex::new(writer)),
            state: Arc::new(RwLock::new(IndexingState::default())),
        })
    }

    pub async fn start_indexing(&self, directory: String, progress_callback: impl Fn(&IndexingState) + Send + 'static) -> Result<(), String> {
        info!("Starting indexing for directory: {}", directory);
        let start_time = std::time::Instant::now();
        
        // Update initial state
        {
            let mut state = self.state.write();
            state.total_files = 0;
            state.processed_files = 0;
            state.current_file = format!("Starting scan of {}", directory);
            state.is_complete = false;
            state.state = "Scanning".to_string();
            state.files_found = 0;
            progress_callback(&state);
        }

        // Create a channel for file paths
        let (tx, mut rx) = tokio::sync::mpsc::channel(1000);
        let tx_clone = tx.clone();

        // Spawn file system walker
        let walker_handle = tokio::spawn(async move {
            let walker = walkdir::WalkDir::new(&directory)
                .follow_links(true)
                .into_iter()
                .filter_entry(|e| !should_skip_path(e.path()));

            for entry in walker {
                match entry {
                    Ok(entry) => {
                        if entry.file_type().is_file() {
                            // Skip files we don't have permission to read
                            if let Ok(metadata) = entry.metadata() {
                                if metadata.permissions().readonly() {
                                    debug!("Skipping readonly file: {}", entry.path().display());
                                    continue;
                                }
                            }

                            if let Err(e) = tx_clone.send(entry.path().to_path_buf()).await {
                                error!("Failed to send path: {}", e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        // Log but don't fail on common errors
                        match e.io_error().map(|e| e.kind()) {
                            Some(std::io::ErrorKind::PermissionDenied) => {
                                debug!("Permission denied: {}", e);
                            }
                            Some(std::io::ErrorKind::NotFound) => {
                                debug!("File not found (may have been deleted): {}", e);
                            }
                            _ => {
                                error!("Walker error: {}", e);
                            }
                        }
                    }
                }
            }
        });

        // Process files
        let mut writer = self.writer.lock().await;
        let mut processed = 0;
        let mut total = 0;

        while let Some(path) = rx.recv().await {
            total += 1;
            {
                let mut state = self.state.write();
                state.total_files = total;
                state.files_found = total;
                progress_callback(&state);
            }

            let path_str = path.to_string_lossy().into_owned();
            debug!("Processing file: {}", path_str);

            // Skip if path should be ignored
            if should_skip_path(&path) {
                debug!("Skipping excluded path: {}", path_str);
                continue;
            }

            // Read file metadata
            let metadata = match fs::metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    debug!("Failed to read metadata for {}: {}", path_str, e);
                    continue;
                }
            };

            // Skip large files (e.g., > 10MB)
            if metadata.len() > 10_000_000 {
                debug!("Skipping large file: {} ({} bytes)", path_str, metadata.len());
                continue;
            }

            // Read file content
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    debug!("Skipping binary or unreadable file {}: {}", path_str, e);
                    continue;
                }
            };

            // Create document
            let mut doc = Document::new();
            doc.add_text(self.fields.path, &path_str);
            doc.add_text(self.fields.content, &content);
            doc.add_text(self.fields.file_name, path.file_name().unwrap_or_default().to_string_lossy().as_ref());
            doc.add_text(self.fields.file_type, path.extension().unwrap_or_default().to_string_lossy().as_ref());
            
            // Handle modified time with a fallback to 0 if we can't get the modification time
            let modified_time = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs().to_string())
                .unwrap_or_else(|| "0".to_string());
            doc.add_text(self.fields.modified, &modified_time);
            
            doc.add_text(self.fields.size, metadata.len().to_string());

            // Add document to index
            if let Err(e) = writer.add_document(doc) {
                error!("Failed to add document for {}: {}", path_str, e);
                continue;
            }

            processed += 1;
            {
                let mut state = self.state.write();
                state.processed_files = processed;
                state.current_file = path_str;
                progress_callback(&state);
            }

            // Commit periodically
            if processed % COMMIT_INTERVAL == 0 {
                if let Err(e) = writer.commit() {
                    error!("Failed to commit batch: {}", e);
                }
            }
        }

        // Wait for walker to complete
        if let Err(e) = walker_handle.await {
            error!("Walker task failed: {}", e);
        }

        // Final commit
        if let Err(e) = writer.commit() {
            error!("Failed to commit final batch: {}", e);
        }

        let elapsed = start_time.elapsed();
        info!("Indexing completed in {:?}", elapsed);
        
        // Update final state
        {
            let mut state = self.state.write();
            state.state = "Complete".to_string();
            state.is_complete = true;
            progress_callback(&state);
        }
        
        Ok(())
    }

    pub async fn search(&self, query: &str) -> Result<Vec<serde_json::Value>, String> {
        let reader = self.index.reader()
            .map_err(|e| format!("Failed to get index reader: {}", e))?;
        let searcher = reader.searcher();

        // Create a query parser that searches in name, path, and content fields
        let mut query_parser = QueryParser::for_index(&self.index, vec![
            self.fields.file_name,
            self.fields.path,
            self.fields.content,
            self.fields.file_type
        ]);

        // Set field boosts
        query_parser.set_field_boost(self.fields.file_name, 3.0);
        query_parser.set_field_boost(self.fields.path, 2.0);
        query_parser.set_field_boost(self.fields.content, 1.0);

        // Parse and execute query
        let query = query_parser.parse_query(query)
            .map_err(|e| format!("Failed to parse query: {}", e))?;
        let top_docs = searcher.search(&query, &TopDocs::with_limit(100))
            .map_err(|e| format!("Search failed: {}", e))?;

        let mut results = Vec::new();
        for (_score, doc_address) in top_docs {
            let retrieved_doc = searcher.doc(doc_address)
                .map_err(|e| format!("Failed to retrieve document: {}", e))?;
            let mut doc_json = serde_json::Map::new();

            // Get values from document
            if let Some(name) = retrieved_doc
                .get_first(self.fields.file_name)
                .and_then(|f| f.as_text()) {
                doc_json.insert("name".to_string(), serde_json::Value::String(name.to_string()));
            }

            if let Some(path) = retrieved_doc
                .get_first(self.fields.path)
                .and_then(|f| f.as_text()) {
                doc_json.insert("path".to_string(), serde_json::Value::String(path.to_string()));
            }

            if let Some(file_type) = retrieved_doc
                .get_first(self.fields.file_type)
                .and_then(|f| f.as_text()) {
                doc_json.insert("type".to_string(), serde_json::Value::String(file_type.to_string()));
            }

            results.push(serde_json::Value::Object(doc_json));
        }

        Ok(results)
    }

    pub async fn get_stats(&self) -> Result<String, String> {
        let state = self.state.read();
        Ok(format!(
            "Total files: {}, Processed: {}, Complete: {}",
            state.total_files, state.processed_files, state.is_complete
        ))
    }
} 