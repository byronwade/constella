use tauri::State;
use crate::indexing::{Indexer, IndexState};
use log::info;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct IndexingProgress {
    pub state: IndexState,
    pub stats: IndexingStats,
    pub current_file: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexingStats {
    pub total_files: usize,
    pub processed_files: usize,
    pub percent_complete: f32,
    pub files_per_second: f32,
    pub elapsed_seconds: u64,
    pub estimated_remaining_seconds: Option<u64>,
}

#[tauri::command]
pub async fn get_indexing_progress(indexer: State<'_, Indexer>) -> Result<IndexingProgress, String> {
    let state = indexer.get_state();
    
    Ok(IndexingProgress {
        state: match state.state.as_str() {
            "idle" => IndexState::Idle,
            "scanning" => IndexState::Scanning,
            "indexing" => IndexState::Indexing,
            "completed" => IndexState::Completed,
            _ => IndexState::Error("Unknown state".to_string()),
        },
        stats: IndexingStats {
            total_files: state.total_files,
            processed_files: state.processed_files,
            percent_complete: if state.total_files > 0 {
                (state.processed_files as f32 / state.total_files as f32) * 100.0
            } else {
                0.0
            },
            files_per_second: state.files_per_second,
            elapsed_seconds: state.elapsed_seconds,
            estimated_remaining_seconds: if state.files_per_second > 0.0 {
                let remaining_files = state.total_files.saturating_sub(state.processed_files);
                Some((remaining_files as f32 / state.files_per_second) as u64)
            } else {
                None
            },
        },
        current_file: state.current_file,
    })
}

#[tauri::command]
pub async fn start_indexing(directory: String, indexer: State<'_, Indexer>) -> Result<(), String> {
    info!("Starting indexing for directory: {}", directory);
    indexer.start_indexing(&directory).await
}

#[tauri::command]
pub async fn search_files(query: String, indexer: State<'_, Indexer>) -> Result<Vec<serde_json::Value>, String> {
    info!("Searching for: {}", query);
    indexer.search(&query).await
}

#[tauri::command]
pub async fn cancel_indexing(indexer: State<'_, Indexer>) -> Result<(), String> {
    info!("Cancelling indexing");
    indexer.cancel().await
}

#[tauri::command]
pub async fn get_index_stats(indexer: State<'_, Indexer>) -> Result<serde_json::Value, String> {
    let reader = indexer.get_reader().await
        .map_err(|e| format!("Failed to get reader: {}", e))?;
    let searcher = reader.searcher();
    
    let mut stats = serde_json::Map::new();
    stats.insert("total_documents".to_string(), serde_json::Value::Number(serde_json::Number::from(searcher.num_docs())));
    stats.insert("last_updated".to_string(), serde_json::Value::String(chrono::Local::now().to_rfc3339()));
    
    Ok(serde_json::Value::Object(stats))
} 