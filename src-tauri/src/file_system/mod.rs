use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use std::fs;
use std::time::SystemTime;
use mime_guess::from_path;
use tokio::fs as tokio_fs;
use log::{info, warn};
use rayon::prelude::*;

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

pub struct FileSystem {}

impl FileSystem {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn scan_directory<F>(&self, root: &Path, progress_callback: F) -> Result<Vec<FileInfo>, String>
    where
        F: Fn(usize) + Send + Sync + 'static,
    {
        info!("Scanning directory: {:?}", root);
        
        // First, collect all paths in parallel using rayon
        let paths: Vec<_> = WalkDir::new(root)
            .into_iter()
            .par_bridge() // Use rayon's parallel iterator
            .filter_map(|entry| {
                match entry {
                    Ok(entry) => Some(entry.path().to_owned()),
                    Err(e) => {
                        warn!("Error walking directory: {}", e);
                        None
                    }
                }
            })
            .collect();
            
        info!("Found {} paths", paths.len());
        progress_callback(paths.len());

        // Process paths in chunks to avoid memory pressure
        let chunk_size = 1000;
        let mut all_files = Vec::with_capacity(paths.len());
        
        for chunk in paths.chunks(chunk_size) {
            // Process chunk in parallel
            let chunk_files: Vec<_> = chunk.par_iter()
                .filter_map(|path| {
                    match self.get_file_info(path) {
                        Ok(info) => Some(info),
                        Err(e) => {
                            warn!("Error getting file info for {:?}: {}", path, e);
                            None
                        }
                    }
                })
                .collect();
                
            all_files.extend(chunk_files);
            
            // Yield to allow other tasks to run
            tokio::task::yield_now().await;
        }

        Ok(all_files)
    }

    fn get_file_info(&self, path: &Path) -> Result<FileInfo, String> {
        let metadata = fs::metadata(path)
            .map_err(|e| format!("Failed to get metadata for {:?}: {}", path, e))?;
            
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
            
        let mime_type = from_path(path)
            .first()
            .map(|m| m.to_string());
            
        Ok(FileInfo {
            path: path.to_owned(),
            name,
            size: metadata.len(),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            is_dir: metadata.is_dir(),
            mime_type,
            content: None, // Content is loaded later when needed
        })
    }

    pub async fn read_file_content(&self, path: &Path) -> Result<String, String> {
        if !path.is_file() {
            return Err("Not a file".to_string());
        }

        tokio_fs::read_to_string(path)
            .await
            .map_err(|e| format!("Failed to read file content: {}", e))
    }
} 