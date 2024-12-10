use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use log::info;
use std::path::PathBuf;

pub struct FileScanner {
    total_files: Arc<AtomicUsize>,
}

impl FileScanner {
    pub fn new() -> Self {
        Self {
            total_files: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub async fn scan_directory(&self, path: impl AsRef<Path>) -> usize {
        let path = path.as_ref();
        info!("Starting parallel scan of directory: {:?}", path);
        let start_time = std::time::Instant::now();

        // First pass: Count all files
        let total = walkdir::WalkDir::new(path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|entry| {
                let path = entry.path();
                !self.should_skip_path(path) && entry.file_type().is_file()
            })
            .count();

        info!("Found {} files in {:?}", total, start_time.elapsed());
        
        // Store total and return
        self.total_files.store(total, Ordering::SeqCst);
        total
    }

    pub fn collect_paths<P: AsRef<Path>>(&self, path: P) -> Vec<PathBuf> {
        walkdir::WalkDir::new(path.as_ref())
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|entry| {
                let path = entry.path();
                !self.should_skip_path(path) && entry.file_type().is_file()
            })
            .map(|entry| entry.path().to_path_buf())
            .collect()
    }

    fn should_skip_path(&self, path: &Path) -> bool {
        // Skip hidden files and directories
        if path.file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.starts_with('.'))
            .unwrap_or(false) {
            return true;
        }

        // Skip only specific problematic directories
        if let Some(path_str) = path.to_str() {
            if path_str.contains("System Volume Information") ||
               path_str.contains("$Recycle.Bin") ||
               path_str.contains("$WINDOWS.~BT") {
                return true;
            }
        }

        false
    }
} 