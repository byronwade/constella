#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::Manager;
use env_logger;
use log::LevelFilter;

mod api;
mod file_system;
mod indexing;
mod utils;

pub struct AppState {
    pub index_manager: Arc<Mutex<indexing::IndexManager>>,
}

impl AppState {
    fn new() -> Self {
        let file_system = Arc::new(file_system::FileSystem::new());
        let index_manager = indexing::IndexManager::new(file_system.clone())
            .expect("Failed to create index manager");
        
        Self {
            index_manager: Arc::new(Mutex::new(index_manager)),
        }
    }
}

fn main() {
    // Initialize logging
    env_logger::Builder::from_default_env()
        .filter_level(LevelFilter::Info)
        .init();

    tauri::Builder::default()
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                let window = app.get_window("main").unwrap();
                window.open_devtools();
            }
            Ok(())
        })
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            api::commands::select_directory,
            api::commands::start_indexing,
            api::commands::search_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
} 