use tauri::{State, Manager};
use log::{info, error, debug};
use serde::Serialize;
use crate::indexing::IndexingState;
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct IndexingProgress {
    pub total_files: usize,
    pub processed_files: usize,
    pub current_file: String,
    pub state: String,
    pub is_complete: bool,
    pub files_found: usize,
    pub start_time: u64,
}

#[tauri::command]
pub async fn select_directory() -> Result<String, String> {
    let path = tauri::api::dialog::blocking::FileDialogBuilder::new()
        .set_title("Select Directory to Index")
        .pick_folder();

    match path {
        Some(path) => Ok(path.to_string_lossy().to_string()),
        None => Err("No directory selected".to_string()),
    }
}

#[tauri::command]
pub async fn start_indexing(path: String, state: State<'_, AppState>) -> Result<(), String> {
    info!("Starting indexing for path: {}", path);
    let indexer = state.indexer.clone();
    let app_handle = state.app_handle.clone();
    
    debug!("Creating progress callback");
    // Create progress callback
    let progress_callback = move |state: &IndexingState| {
        let progress = IndexingProgress {
            total_files: state.total_files,
            processed_files: state.processed_files,
            current_file: state.current_file.clone(),
            state: state.state.clone(),
            is_complete: state.is_complete,
            files_found: state.files_found,
            start_time: state.start_time,
        };
        
        debug!("Emitting progress: {:?}", progress);
        if let Err(e) = app_handle.emit_all("indexing-progress", progress) {
            error!("Failed to emit progress: {}", e);
        }
    };

    debug!("Spawning indexing task");
    // Start indexing in the background
    tokio::spawn(async move {
        debug!("Acquiring index manager lock");
        let index_manager = indexer.lock().await;
        debug!("Starting indexing operation");
        if let Err(e) = index_manager.start_indexing(path, progress_callback).await {
            error!("Indexing failed: {}", e);
        }
    });

    debug!("Indexing task spawned");
    Ok(())
}

#[tauri::command]
pub async fn search_files(
    query: String,
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    info!("search_files: Executing search query: {}", query);
    let index_manager = state.indexer.lock().await;
    let results = index_manager.search(&query).await?;
    Ok(results)
}

#[tauri::command]
pub async fn verify_index(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let index_manager = state.indexer.lock().await;
    index_manager.get_stats().await
} 