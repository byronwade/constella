use tauri::{State, Manager};
use log::info;
use serde::Serialize;
use crate::indexing::IndexingState;
use crate::AppState;
use std::path::PathBuf;

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
    let indexer = state.indexer.clone();
    let app_handle = state.app_handle.clone();
    
    // Create a new tokio runtime for the indexing task
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("Failed to create runtime: {}", e))?;
    
    // Spawn the indexing task in a new thread with its own runtime
    std::thread::spawn(move || {
        rt.block_on(async {
            let mut index_manager = indexer.lock().await;
            
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
                
                if let Err(e) = app_handle.as_ref().emit_all("indexing-progress", progress) {
                    log::error!("Failed to emit progress: {}", e);
                }
            };

            if let Err(e) = index_manager.start_indexing(PathBuf::from(path), progress_callback).await {
                log::error!("Indexing failed: {}", e);
            }
        });
    });

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
    
    let json_results = results.into_iter()
        .map(|doc| serde_json::to_value(&doc))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    
    Ok(json_results)
}

#[tauri::command]
pub async fn verify_index(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let index_manager = state.indexer.lock().await;
    index_manager.get_stats().await
} 