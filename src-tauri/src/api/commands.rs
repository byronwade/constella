use std::path::PathBuf;
use std::sync::Arc;
use tauri::{State, AppHandle, Manager};
use tokio::sync::Mutex;
use log::{info, error};
use serde::Serialize;
use crate::indexing::{IndexManager, IndexingState, SearchDoc};

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
    state: State<'_, Arc<Mutex<IndexManager>>>,
    app_handle: AppHandle,
    path: String,
) -> Result<(), String> {
    let start_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    // Create a channel for progress updates
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel(32);
    let app_handle_for_updates = app_handle.clone();

    // Spawn a task to handle progress updates
    tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            if let Err(e) = app_handle_for_updates.emit_all("indexing-progress", progress) {
                error!("Failed to emit progress: {}", e);
            }
        }
    });

    // Clone necessary data for the background task
    let index_manager = Arc::clone(&state.inner());
    let app_handle_for_errors = app_handle;
    let path_clone = path;
    let progress_tx = Arc::new(progress_tx);

    // Spawn the indexing task
    tokio::spawn(async move {
        let progress_tx = Arc::clone(&progress_tx);
        let progress_callback = move |state: &IndexingState| {
            let progress = IndexingProgress {
                total_files: state.total_files,
                processed_files: state.processed_files,
                current_file: state.current_file.clone(),
                is_complete: state.is_complete,
                state: state.state.clone(),
                files_found: state.files_found,
                start_time,
            };
            
            if let Err(e) = progress_tx.try_send(progress) {
                error!("Failed to send progress update: {}", e);
            }
        };

        // Start indexing
        let mut index_manager = index_manager.lock().await;
        if let Err(e) = (&mut *index_manager).start_indexing(PathBuf::from(path_clone), progress_callback).await {
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
            
            if let Err(e) = app_handle_for_errors.emit_all("indexing-progress", error_progress) {
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
    let results = (&*index_manager).search(&query).await?;
    Ok(results)
}

#[tauri::command]
pub async fn verify_index(
    state: State<'_, Arc<Mutex<IndexManager>>>,
) -> Result<String, String> {
    let index_manager = state.lock().await;
    (&*index_manager).verify_index().await
} 