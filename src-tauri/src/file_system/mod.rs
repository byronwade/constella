use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use tokio::sync::RwLock;
use std::collections::HashSet;
use std::time::SystemTime;
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use std::sync::Mutex;
use log::{info, warn, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: PathBuf,
    pub name: String,
    pub extension: Option<String>,
    pub mime_type: Option<String>,
    pub size: u64,
    pub modified: SystemTime,
    pub is_dir: bool,
}

impl FileInfo {
    pub fn from_dir_entry(entry: &walkdir::DirEntry) -> Result<Self, String> {
        let path = entry.path().to_path_buf();
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                warn!("Failed to get metadata for {}: {}", path.display(), e);
                return Err(format!("Failed to get metadata: {}", e));
            }
        };
        
        let name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
            
        let extension = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_string());
            
        let mime_type = extension
            .as_ref()
            .and_then(|ext| mime_guess::from_ext(ext).first())
            .map(|m| m.to_string());
            
        let modified = metadata.modified().map_err(|e| {
            warn!("Failed to get modified time for {}: {}", path.display(), e);
            format!("Failed to get modified time: {}", e)
        })?;

        Ok(FileInfo {
            path,
            name,
            extension,
            mime_type,
            size: metadata.len(),
            modified,
            is_dir: metadata.is_dir(),
        })
    }
}

pub struct FileSystem {
    excluded_paths: RwLock<HashSet<PathBuf>>,
    processed_paths: Arc<Mutex<HashSet<PathBuf>>>,
}

impl FileSystem {
    pub fn new() -> Self {
        Self {
            excluded_paths: RwLock::new(HashSet::new()),
            processed_paths: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    fn should_skip_path(path: &Path) -> bool {
        path.to_str()
            .map(|p| {
                let should_skip = p.contains("node_modules") ||
                    p.contains(".git") ||
                    p.contains("target") ||
                    p.starts_with(".");
                
                if should_skip {
                    info!("Skipping path: {}", p);
                }
                
                should_skip
            })
            .unwrap_or(true)
    }

    pub async fn scan_directory<F>(
        &self,
        path: &PathBuf,
        progress_callback: &F
    ) -> Result<Vec<FileInfo>, String>
    where
        F: Fn(usize) + Send + Sync + 'static,
    {
        info!("Starting directory scan at: {}", path.display());
        
        if !path.exists() {
            error!("Path does not exist: {}", path.display());
            return Err("Directory does not exist".to_string());
        }
        
        if !path.is_dir() {
            error!("Path is not a directory: {}", path.display());
            return Err("Selected path is not a directory".to_string());
        }

        let mut files = Vec::new();
        let walker = WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !Self::should_skip_path(e.path()));

        for entry in walker {
            match entry {
                Ok(entry) => {
                    match FileInfo::from_dir_entry(&entry) {
                        Ok(file_info) => {
                            files.push(file_info);
                            progress_callback(files.len());
                        }
                        Err(e) => {
                            warn!("Failed to process entry {}: {}", entry.path().display(), e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Error walking directory: {}", e);
                }
            }
        }

        info!("Completed directory scan. Found {} files", files.len());
        
        if files.is_empty() {
            warn!("No files found in directory: {}", path.display());
            return Err("No files found in directory".to_string());
        }

        Ok(files)
    }
} 