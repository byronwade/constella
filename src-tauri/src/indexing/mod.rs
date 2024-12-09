use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tantivy::{Document, Index};
use tantivy::schema::{Schema, STORED, TEXT, Field};
use crate::file_system::{FileSystem, FileInfo};
use log::{info, warn, error};

#[derive(Clone)]
pub struct SchemaFields {
    pub path: Field,
    pub name: Field,
    pub file_type: Field,
    pub size: Field,
    pub modified: Field,
    pub content: Field,
}

pub struct IndexManager {
    pub index: Index,
    pub schema: Schema,
    pub fields: SchemaFields,
    pub file_system: Arc<FileSystem>,
    pub indexing_state: Arc<Mutex<bool>>,
}

impl IndexManager {
    pub fn new(file_system: Arc<FileSystem>) -> Result<Self, String> {
        info!("Initializing IndexManager");
        let mut schema_builder = Schema::builder();
        
        info!("Creating schema fields");
        let fields = SchemaFields {
            path: schema_builder.add_text_field("path", TEXT | STORED),
            name: schema_builder.add_text_field("name", TEXT | STORED),
            file_type: schema_builder.add_text_field("file_type", TEXT | STORED),
            size: schema_builder.add_text_field("size", STORED),
            modified: schema_builder.add_text_field("modified", STORED),
            content: schema_builder.add_text_field("content", TEXT),
        };

        info!("Building schema");
        let schema = schema_builder.build();
        
        info!("Creating in-memory index");
        let index = Index::create_in_ram(schema.clone());

        info!("IndexManager initialization complete");
        Ok(Self {
            index,
            schema,
            fields,
            file_system,
            indexing_state: Arc::new(Mutex::new(false)),
        })
    }

    pub async fn index_directory<F>(
        &self,
        path: &PathBuf,
        progress_callback: F,
    ) -> Result<(), String>
    where
        F: Fn(usize) + Send + Sync + 'static,
    {
        info!("Checking indexing state for path: {}", path.display());
        let mut is_indexing = self.indexing_state.lock().await;
        if *is_indexing {
            error!("Indexing already in progress");
            return Err("Indexing already in progress".to_string());
        }
        *is_indexing = true;

        // Create a cleanup guard that will set is_indexing to false when dropped
        struct IndexingGuard<'a> {
            state: &'a mut bool,
        }
        
        impl<'a> Drop for IndexingGuard<'a> {
            fn drop(&mut self) {
                *self.state = false;
            }
        }
        
        let _guard = IndexingGuard { state: &mut is_indexing };

        info!("Creating index writer");
        let mut index_writer = match self.index.writer(50_000_000) {
            Ok(writer) => writer,
            Err(e) => {
                error!("Failed to create index writer: {}", e);
                return Err(format!("Failed to create index writer: {}", e));
            }
        };
        
        info!("Scanning directory for files");
        let files = match self.file_system.scan_directory(path, &progress_callback).await {
            Ok(files) => files,
            Err(e) => {
                error!("Failed to scan directory: {}", e);
                return Err(e);
            }
        };
        
        if files.is_empty() {
            info!("No files found in directory");
            return Ok(());
        }
        
        info!("Processing {} files", files.len());
        for (i, file_info) in files.iter().enumerate() {
            let mut doc = Document::new();
            
            // Add fields to document
            doc.add_text(self.fields.path, file_info.path.to_string_lossy().to_string());
            doc.add_text(self.fields.name, file_info.name.clone());
            doc.add_text(self.fields.file_type, file_info.mime_type.clone().unwrap_or_default());
            doc.add_text(self.fields.size, file_info.size.to_string());
            doc.add_text(self.fields.modified, format!("{:?}", file_info.modified));

            if let Err(e) = index_writer.add_document(doc) {
                warn!("Failed to add document for file {}: {}", file_info.path.display(), e);
                continue;
            }

            // Update progress every 1000 files
            if (i + 1) % 1000 == 0 {
                progress_callback(i + 1);
                info!("Processed {} files", i + 1);
            }
        }

        // Final progress update
        progress_callback(files.len());

        info!("Committing index");
        if let Err(e) = index_writer.commit() {
            error!("Failed to commit index: {}", e);
            return Err(format!("Failed to commit index: {}", e));
        }

        info!("Indexing completed successfully");
        Ok(())
    }

    pub async fn search(&self, query: &str) -> Result<Vec<String>, String> {
        info!("Executing search query: {}", query);
        
        let reader = match self.index.reader() {
            Ok(r) => r,
            Err(e) => {
                error!("Failed to create index reader: {}", e);
                return Err(format!("Failed to create index reader: {}", e));
            }
        };
        
        let searcher = reader.searcher();
        let query_parser = tantivy::query::QueryParser::for_index(
            &self.index,
            vec![self.fields.path, self.fields.name, self.fields.content]
        );
        
        let query = match query_parser.parse_query(query) {
            Ok(q) => q,
            Err(e) => {
                error!("Failed to parse query: {}", e);
                return Err(format!("Failed to parse query: {}", e));
            }
        };
        
        let top_docs = match searcher.search(&query, &tantivy::collector::TopDocs::with_limit(100)) {
            Ok(docs) => docs,
            Err(e) => {
                error!("Failed to execute search: {}", e);
                return Err(format!("Failed to execute search: {}", e));
            }
        };

        let mut results = Vec::new();
        for (_score, doc_address) in top_docs {
            match searcher.doc(doc_address) {
                Ok(doc) => {
                    if let Some(path) = doc.get_first(self.fields.path) {
                        results.push(path.as_text().unwrap_or_default().to_string());
                    }
                }
                Err(e) => warn!("Failed to retrieve document: {}", e),
            }
        }

        info!("Search completed. Found {} results", results.len());
        Ok(results)
    }
} 