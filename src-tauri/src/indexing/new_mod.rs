use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tantivy::{schema::*, Index, IndexWriter, collector::TopDocs, query::QueryParser};
use serde::{Serialize, Deserialize};
use crate::file_system::FileSystem;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexProgress {
    pub total_files: usize,
    pub processed_files: usize,
    pub current_file: String,
    pub state: String,
    pub is_complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub path: String,
    pub name: String,
    pub matches: Vec<SearchMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    pub line: usize,
    pub content: String,
}

pub struct IndexManager {
    index: Index,
    writer: Arc<Mutex<IndexWriter>>,
    schema: Schema,
    file_system: Arc<Mutex<FileSystem>>,
    indexing_state: Arc<Mutex<IndexingState>>,
}

#[derive(Debug)]
struct IndexingState {
    is_paused: bool,
    is_cancelled: bool,
    is_complete: bool,
    total_files: usize,
    processed_files: usize,
    current_file: String,
}

impl IndexManager {
    pub fn new() -> tantivy::Result<Self> {
        let mut schema_builder = Schema::builder();
        schema_builder.add_text_field("path", TEXT | STORED);
        schema_builder.add_text_field("content", TEXT);
        let schema = schema_builder.build();

        let index = Index::create_in_ram(schema.clone());
        let writer = index.writer(50_000_000)?; // 50MB buffer

        Ok(Self {
            index,
            writer: Arc::new(Mutex::new(writer)),
            schema,
            file_system: Arc::new(Mutex::new(FileSystem::new())),
            indexing_state: Arc::new(Mutex::new(IndexingState {
                is_paused: false,
                is_cancelled: false,
                is_complete: false,
                total_files: 0,
                processed_files: 0,
                current_file: String::new(),
            })),
        })
    }

    pub async fn search(&self, query: &str, limit: usize) -> tantivy::Result<Vec<SearchResult>> {
        let reader = self.index.reader()?;
        let searcher = reader.searcher();
        
        let query_parser = QueryParser::for_index(&self.index, vec![
            self.schema.get_field("path").unwrap(),
            self.schema.get_field("content").unwrap(),
        ]);

        let query = query_parser.parse_query(query)?;
        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::new();
        for (_score, doc_address) in top_docs {
            let retrieved_doc = searcher.doc(doc_address)?;
            let path = retrieved_doc
                .get_first(self.schema.get_field("path").unwrap())
                .and_then(|f| f.as_text())
                .unwrap_or("")
                .to_string();

            let path_buf = PathBuf::from(&path);
            let name = path_buf
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            // TODO: Implement proper content matching
            let matches = vec![SearchMatch {
                line: 1,
                content: "Match found".to_string(),
            }];

            results.push(SearchResult {
                path,
                name,
                matches,
            });
        }

        Ok(results)
    }

    pub async fn index_directory(&self, path: &PathBuf, progress_tx: mpsc::Sender<IndexProgress>) -> Result<(), String> {
        println!("Starting directory indexing: {:?}", path);
        let fs = self.file_system.clone();

        // Reset state
        {
            let mut state = self.indexing_state.lock().await;
            state.total_files = 0;
            state.processed_files = 0;
            state.current_file = "Starting file scan...".to_string();
            state.is_complete = false;
            state.is_cancelled = false;
            state.is_paused = false;
        }
        self.send_progress(&progress_tx).await;

        // Get all files with progress updates
        let fs_lock = fs.lock().await;
        let files = fs_lock.scan_directory(path, move |count| {
            let progress = IndexProgress {
                total_files: count,
                processed_files: 0,
                current_file: format!("Found {} files...", count),
                state: "Running".to_string(),
                is_complete: false,
            };
            let _ = progress_tx.try_send(progress);
        }).await.map_err(|e| e.to_string())?;
        drop(fs_lock);

        let total_files = files.len();
        
        // Update state to start indexing
        {
            let mut state = self.indexing_state.lock().await;
            state.total_files = total_files;
            state.processed_files = 0;
            state.current_file = format!("Starting to index {} files...", total_files);
        }
        self.send_progress(&progress_tx).await;

        // Process files
        for (i, file_info) in files.into_iter().enumerate() {
            // Check for cancellation
            {
                let state = self.indexing_state.lock().await;
                if state.is_cancelled {
                    println!("Indexing cancelled");
                    return Ok(());
                }
            }

            // Handle pause
            loop {
                let state = self.indexing_state.lock().await;
                if !state.is_paused {
                    break;
                }
                drop(state);
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }

            // Update state with current file
            {
                let mut state = self.indexing_state.lock().await;
                state.processed_files = i + 1;
                state.current_file = file_info.path.to_string_lossy().to_string();
            }
            self.send_progress(&progress_tx).await;

            // Index the file
            if let Err(e) = self.index_file(&file_info).await {
                eprintln!("Error indexing file {:?}: {}", file_info.path, e);
                continue;
            }

            // Commit every 1000 files
            if (i + 1) % 1000 == 0 {
                println!("Committing changes after {} files", i + 1);
                if let Err(e) = self.writer.lock().await.commit() {
                    eprintln!("Error committing changes: {}", e);
                }
            }
        }

        // Final commit
        println!("Final commit of changes");
        if let Err(e) = self.writer.lock().await.commit() {
            eprintln!("Error in final commit: {}", e);
        }

        // Mark as complete and send final progress
        {
            let mut state = self.indexing_state.lock().await;
            state.is_complete = true;
            state.current_file = format!("Completed indexing {} files", total_files);
        }
        self.send_progress(&progress_tx).await;

        Ok(())
    }

    async fn index_file(&self, file_info: &crate::file_system::FileInfo) -> Result<(), String> {
        let writer = self.writer.lock().await;
        let mut doc = Document::new();
        
        doc.add_text(
            self.schema.get_field("path").unwrap(),
            &file_info.path.to_string_lossy()
        );
        
        // Only try to read content for non-directory files
        if !file_info.is_dir {
            // Only read text files
            if let Some(mime_type) = &file_info.mime_type {
                if mime_type.starts_with("text/") {
                    match tokio::fs::read_to_string(&file_info.path).await {
                        Ok(content) => {
                            println!("Indexing content for file: {:?}", file_info.path);
                            doc.add_text(
                                self.schema.get_field("content").unwrap(),
                                &content
                            );
                        },
                        Err(e) => {
                            eprintln!("Error reading file {:?}: {}", file_info.path, e);
                        }
                    }
                }
            }
        }
        
        writer.add_document(doc).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn pause(&mut self) -> Result<(), String> {
        let mut state = self.indexing_state.lock().await;
        state.is_paused = true;
        Ok(())
    }

    pub async fn resume(&mut self) -> Result<(), String> {
        let mut state = self.indexing_state.lock().await;
        state.is_paused = false;
        Ok(())
    }

    pub async fn cancel(&mut self) -> Result<(), String> {
        let mut state = self.indexing_state.lock().await;
        state.is_cancelled = true;
        Ok(())
    }

    pub async fn get_status(&self) -> IndexProgress {
        let state = self.indexing_state.lock().await;
        IndexProgress {
            total_files: state.total_files,
            processed_files: state.processed_files,
            current_file: state.current_file.clone(),
            state: if state.is_complete {
                "Completed".to_string()
            } else if state.is_cancelled {
                "Cancelled".to_string()
            } else if state.is_paused {
                "Paused".to_string()
            } else if state.total_files == 0 {
                "Running".to_string()  // Changed from "Counting" to "Running"
            } else {
                "Running".to_string()  // Changed from "Indexing" to "Running"
            },
            is_complete: state.is_complete
        }
    }

    async fn send_progress(&self, progress_tx: &mpsc::Sender<IndexProgress>) {
        let progress = self.get_status().await;
        let _ = progress_tx.try_send(progress);
    }
} 