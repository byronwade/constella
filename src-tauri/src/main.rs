#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{CustomMenuItem, Menu, Submenu};
use tauri::Manager;
use env_logger;
use log::info;
use crate::indexing::Indexer;

pub mod api;
pub mod scanner;
pub mod indexing;

fn create_context_menu() -> Menu {
    let debug = CustomMenuItem::new("debug", "Toggle Debug Tools");
    let debug_menu = Submenu::new("Debug", Menu::new().add_item(debug));
    Menu::new().add_submenu(debug_menu)
}

#[tokio::main]
async fn main() {
    env_logger::init();
    info!("Starting Constella");

    tauri::Builder::default()
        .menu(create_context_menu())
        .setup(|app| {
            // Initialize indexer
            let indexer = Indexer::new().expect("Failed to create indexer");
            
            // Store in app state
            app.manage(indexer);
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api::commands::start_indexing,
            api::commands::search_files,
            api::commands::cancel_indexing,
            api::commands::get_indexing_progress,
            api::commands::get_index_stats,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
} 