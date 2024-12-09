use std::path::PathBuf;
use std::sync::Arc;
use serde::Serialize;
use crate::AppState;
use tauri::api::dialog;
use log::{info, error};

#[derive(Clone, Serialize)]
pub struct IndexingProgress {
    pub total_files: usize,
    pub processed_files: usize,
    pub current_file: String,
    pub state: String,
}

#[derive(Clone, Serialize)]
pub struct SearchResult {
    pub path: String,
}

#[tauri::command]
pub async fn select_directory() -> Result<String, String> {
    let result = tokio::task::spawn_blocking(|| {
        dialog::blocking::FileDialogBuilder::new()
            .set_directory("/")
            .pick_folder()
    }).await.map_err(|e| e.to_string())?;

    match result {
        Some(path) => Ok(path.to_string_lossy().to_string()),
        None => Err("No directory selected".to_string()),
    }
}

#[tauri::command]
pub async fn start_indexing(
    path: String,
    state: tauri::State<'_, AppState>,
    window: tauri::Window,
) -> Result<(), String> {
    info!("Starting indexing process for path: {}", path);
    
    let path = PathBuf::from(path);
    if !path.exists() {
        error!("Path does not exist: {}", path.display());
        window.emit("indexing-error", "Selected directory does not exist").map_err(|e| e.to_string())?;
        return Err("Selected directory does not exist".to_string());
    }

    if !path.is_dir() {
        error!("Path is not a directory: {}", path.display());
        window.emit("indexing-error", "Selected path is not a directory").map_err(|e| e.to_string())?;
        return Err("Selected path is not a directory".to_string());
    }

    // Emit initial progress
    window.emit("indexing-progress", IndexingProgress {
        total_files: 0,
        processed_files: 0,
        current_file: "Starting indexing process...".to_string(),
        state: "Running".to_string(),
    }).map_err(|e| e.to_string())?;

    // Create an Arc<Window> to share between threads
    let window = Arc::new(window);
    let window_clone = Arc::clone(&window);

    // Create a progress callback that doesn't mutate state
    let progress_callback = move |count: usize| {
        let progress = IndexingProgress {
            total_files: count,
            processed_files: count,
            current_file: format!("Found {} files", count),
            state: "Running".to_string(),
        };
        
        // Use the cloned window to emit progress
        if let Err(e) = window_clone.emit("indexing-progress", &progress) {
            error!("Failed to emit progress: {}", e);
        }
    };

    info!("Acquiring index manager lock...");
    let index_manager = state.index_manager.lock().await;
    
    info!("Starting directory indexing...");
    match index_manager.index_directory(&path, progress_callback).await {
        Ok(_) => {
            info!("Indexing completed successfully");
            window.emit("indexing-complete", true).map_err(|e| e.to_string())?;
            Ok(())
        }
        Err(e) => {
            error!("Indexing failed: {}", e);
            window.emit("indexing-error", e.to_string()).map_err(|e| e.to_string())?;
            Err(format!("Failed to index directory: {}", e))
        }
    }
}

#[tauri::command]
pub async fn search_files(
    query: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<SearchResult>, String> {
    info!("Executing search query: {}", query);
    let index_manager = state.index_manager.lock().await;
    index_manager
        .search(&query)
        .await
        .map(|paths| {
            let results: Vec<SearchResult> = paths.into_iter()
                .map(|path| SearchResult { path })
                .collect();
            info!("Found {} results for query: {}", results.len(), query);
            results
        })
} 