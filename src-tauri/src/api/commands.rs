use std::path::PathBuf;
use tauri::{State, AppHandle, Manager};
use serde::Serialize;
use log::{info, error};
use crate::indexing::{IndexManager, IndexingState, SearchDoc};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Serialize)]
pub struct IndexingProgress {
    pub total_files: usize,
    pub processed_files: usize,
    pub current_file: String,
    pub is_complete: bool,
    pub state: String,
    pub files_found: usize,
    pub start_time: u64,
}

#[tauri::command]
pub async fn select_directory() -> Result<String, String> {
    info!("select_directory: Starting directory selection dialog");
    let result = tokio::task::spawn_blocking(|| {
        info!("select_directory: Opening file dialog");
        tauri::api::dialog::blocking::FileDialogBuilder::new()
            .set_directory("/")
            .pick_folder()
    }).await.map_err(|e| {
        error!("select_directory: Failed to spawn blocking task: {}", e);
        e.to_string()
    })?;

    match result {
        Some(path) => {
            let path_str = path.to_string_lossy().to_string();
            info!("select_directory: Directory selected: {}", path_str);
            Ok(path_str)
        },
        None => {
            info!("select_directory: No directory selected");
            Err("No directory selected".to_string())
        },
    }
}

#[tauri::command]
pub async fn start_indexing(
    path: String,
    state: State<'_, Arc<Mutex<IndexManager>>>,
    app_handle: AppHandle,
) -> Result<(), String> {
    info!("start_indexing: Starting directory indexing...");
    
    // Initial progress update with current timestamp
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
        
    let progress = IndexingProgress {
        total_files: 0,
        processed_files: 0,
        current_file: "Starting scan...".to_string(),
        is_complete: false,
        state: "Scanning".to_string(),
        files_found: 0,
        start_time,
    };
    
    if let Err(e) = app_handle.emit_all("indexing-progress", progress) {
        error!("Failed to emit initial progress: {}", e);
    }
    
    // Clone necessary data for the background task
    let state_clone = state.clone();
    let app_handle_clone = app_handle.clone();
    let path_clone = path.clone();

    // Spawn background task
    tokio::spawn(async move {
        // Get the inner IndexManager and lock it
        let index_manager = state_clone.lock().await;
        
        // Create progress callback that includes yield points
        let progress_callback = move |state: &IndexingState| {
            let progress = IndexingProgress {
                total_files: state.total_files,
                processed_files: state.processed_files,
                current_file: state.current_file.clone(),
                is_complete: state.is_complete,
                state: state.state.clone(),
                files_found: state.files_found,
                start_time: state.start_time,
            };
            
            if let Err(e) = app_handle_clone.emit_all("indexing-progress", progress) {
                error!("Failed to emit indexing progress: {}", e);
            }

            // Yield to allow other tasks to run
            tokio::task::yield_now();
        };
        
        // Start indexing in background
        if let Err(e) = index_manager.start_indexing(PathBuf::from(path_clone), progress_callback).await {
            error!("Indexing failed: {}", e);
            // Send error progress
            let error_progress = IndexingProgress {
                total_files: 0,
                processed_files: 0,
                current_file: format!("Error: {}", e),
                is_complete: false,
                state: "Error".to_string(),
                files_found: 0,
                start_time,
            };
            if let Err(e) = app_handle_clone.emit_all("indexing-progress", error_progress) {
                error!("Failed to emit error progress: {}", e);
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn search_files(
    state: State<'_, Arc<Mutex<IndexManager>>>,
    query: String,
) -> Result<Vec<SearchDoc>, String> {
    info!("search_files: Executing search query: {}", query);
    let index_manager = state.lock().await;
    let results = index_manager.search(&query).await?;
    Ok(results)
}

#[tauri::command]
pub async fn verify_index(
    state: State<'_, Arc<Mutex<IndexManager>>>,
) -> Result<String, String> {
    info!("verify_index: Starting index verification");
    let index_manager = state.lock().await;
    let result = index_manager.verify_index().await;
    match &result {
        Ok(msg) => info!("verify_index: Verification successful: {}", msg),
        Err(e) => error!("verify_index: Verification failed: {}", e),
    }
    result
} 