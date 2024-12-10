use std::path::{Path, PathBuf};
use std::fs;
use std::time::SystemTime;
use mime_guess::from_path;
use log::{info, warn, debug};
use tokio::sync::{mpsc, Semaphore};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use memmap2::Mmap;
use std::io::Read;
use parking_lot::Mutex;
use std::collections::VecDeque;
use tokio::task;
use ignore::WalkBuilder;
use crossbeam_channel::bounded;

const BATCH_SIZE: usize = 100_000; // Increased batch size for better performance
const MAX_CONCURRENT_READS: usize = 4_000; // Increased concurrent reads
const READ_BUFFER_SIZE: usize = 128 * 1024; // Increased to 128KB buffer
const CHANNEL_SIZE: usize = 200_000; // Larger channel size for better throughput

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub is_dir: bool,
    pub mime_type: Option<String>,
    pub content: Option<String>,
}

impl FileInfo {
    pub fn from_path(path: &PathBuf) -> Result<Self, String> {
        let metadata = fs::metadata(path)
            .map_err(|e| format!("Failed to get metadata: {}", e))?;
            
        Ok(FileInfo {
            path: path.clone(),
            name: path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
            size: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            is_dir: metadata.is_dir(),
            mime_type: from_path(path).first().map(|m| m.to_string()),
            content: None,
        })
    }
}

struct WorkQueue {
    queue: Mutex<VecDeque<PathBuf>>,
    total: Arc<AtomicUsize>,
}

impl WorkQueue {
    fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::with_capacity(BATCH_SIZE)),
            total: Arc::new(AtomicUsize::new(0)),
        }
    }
}

pub struct FileSystem {
    work_queue: Arc<WorkQueue>,
    semaphore: Arc<Semaphore>,
    sender: mpsc::Sender<PathBuf>,
    total_files: Arc<AtomicUsize>,
}

impl FileSystem {
    pub fn new() -> Self {
        let (sender, _) = mpsc::channel(1000); // Bounded channel for backpressure
        Self {
            work_queue: Arc::new(WorkQueue::new()),
            semaphore: Arc::new(Semaphore::new(num_cpus::get() * 2)),
            sender,
            total_files: Arc::new(AtomicUsize::new(0))
        }
    }

    pub async fn scan_directory<F>(&self, root: PathBuf, progress_callback: F) -> Result<Vec<FileInfo>, String>
    where
        F: Fn(usize) + Send + Sync + 'static + Clone,
    {
        debug!("Starting directory scan at: {:?}", root);
        
        if !root.exists() {
            return Err(format!("Directory does not exist: {:?}", root));
        }

        let (tx, rx) = bounded(CHANNEL_SIZE);
        let progress = Arc::new(AtomicUsize::new(0));
        let total_found = Arc::new(AtomicUsize::new(0));
        
        // Clone these before moving into closures
        let progress_for_progress = Arc::clone(&progress);
        let progress_for_walker = Arc::clone(&progress);
        let total_for_walker = Arc::clone(&total_found);
        let callback = progress_callback.clone();

        // Progress reporter with better logging and more frequent updates
        let progress_handle = tokio::spawn(async move {
            let mut last_count = 0;
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await; // More frequent updates
                let count = progress_for_progress.load(Ordering::Relaxed);
                // Call callback on every update, not just when count changes
                callback(count);
                if count != last_count {
                    debug!("Scanned {} files", count);
                    last_count = count;
                }
            }
        });

        debug!("Starting parallel file walk");
        
        // Spawn the walker in a dedicated thread
        let walker_handle = task::spawn_blocking(move || {
            let walker = WalkBuilder::new(&root)
                .hidden(false)
                .ignore(false)
                .git_ignore(false)
                .threads(num_cpus::get())
                .build_parallel();

            let tx_clone = tx.clone();
            
            walker.run(|| {
                let tx = tx_clone.clone();
                let progress = Arc::clone(&progress_for_walker);
                let total_found = Arc::clone(&total_for_walker);
                
                Box::new(move |entry| {
                    let entry = match entry {
                        Ok(entry) => entry,
                        Err(e) => {
                            warn!("Error walking directory: {}", e);
                            return ignore::WalkState::Continue;
                        }
                    };

                    let path = entry.path().to_owned();
                    match fs::metadata(&path) {
                        Ok(metadata) => {
                            // Update total first to ensure UI shows correct total
                            total_found.fetch_add(1, Ordering::Relaxed);
                            
                            let file_info = FileInfo {
                                path: path.clone(),
                                name: path.file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_default(),
                                size: metadata.len(),
                                modified: metadata.modified().ok(),
                                created: metadata.created().ok(),
                                is_dir: metadata.is_dir(),
                                mime_type: from_path(&path).first().map(|m| m.to_string()),
                                content: None,
                            };
                            
                            if let Err(e) = tx.send(file_info) {
                                warn!("Failed to send file info: {}", e);
                            }
                            
                            // Update progress after sending file info
                            let count = progress.fetch_add(1, Ordering::Relaxed);
                            if count % 1_000 == 0 { // More frequent debug logging
                                debug!("Processed {} files", count);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to get metadata for {:?}: {}", path, e);
                        }
                    }
                    
                    ignore::WalkState::Continue
                })
            });
        });

        // Collect results with progress tracking
        let mut files = Vec::with_capacity(BATCH_SIZE);
        let collector_handle = task::spawn(async move {
            let mut count = 0;
            while let Ok(file_info) = rx.recv() {
                files.push(file_info);
                count += 1;
                if count % 10_000 == 0 { // More frequent collection updates
                    debug!("Collected {} files", count);
                }
            }
            debug!("Collection complete - total files: {}", files.len());
            files
        });

        // Wait for walker to complete
        if let Err(e) = walker_handle.await {
            warn!("Error during directory walk: {}", e);
        }

        // Cleanup progress reporter
        progress_handle.abort();

        // Get collected files
        let files = match collector_handle.await {
            Ok(files) => files,
            Err(e) => {
                warn!("Error collecting files: {}", e);
                Vec::new()
            }
        };

        let total = total_found.load(Ordering::Relaxed);
        info!("Scan complete - found {} files out of {} total", files.len(), total);
        progress_callback(files.len()); // Final callback with total count
        
        Ok(files)
    }

    async fn read_file_content_optimized(&self, path: &Path) -> Result<String, String> {
        let metadata = fs::metadata(path)
            .map_err(|e| format!("Failed to get metadata: {}", e))?;

        // Use memory mapping for large files
        if metadata.len() > READ_BUFFER_SIZE as u64 * 2 {
            let file = fs::File::open(path)
                .map_err(|e| format!("Failed to open file: {}", e))?;
                
            let mmap = unsafe { Mmap::map(&file) }
                .map_err(|e| format!("Failed to memory map file: {}", e))?;
                
            String::from_utf8(mmap.to_vec())
                .map_err(|e| format!("Failed to decode file content: {}", e))
        } else {
            // Use buffered reading for smaller files
            let mut file = fs::File::open(path)
                .map_err(|e| format!("Failed to open file: {}", e))?;
                
            let mut buffer = Vec::with_capacity(READ_BUFFER_SIZE);
            file.read_to_end(&mut buffer)
                .map_err(|e| format!("Failed to read file: {}", e))?;
                
            String::from_utf8(buffer)
                .map_err(|e| format!("Failed to decode file content: {}", e))
        }
    }

    pub async fn read_file_content(&self, path: &Path) -> Result<String, String> {
        if !path.is_file() {
            return Err("Not a file".to_string());
        }

        let _permit = self.semaphore.acquire().await
            .map_err(|e| format!("Failed to acquire semaphore: {}", e))?;

        self.read_file_content_optimized(path).await
    }
} 