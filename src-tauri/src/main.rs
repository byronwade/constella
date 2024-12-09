#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{CustomMenuItem, Menu, Submenu};
use crate::indexing::IndexManager;
use env_logger;
use log::{LevelFilter, info, error};
use chrono;

mod api;
mod file_system;
mod indexing;
mod utils;

pub struct AppState {
    pub index_manager: Arc<Mutex<IndexManager>>,
}

impl AppState {
    async fn new() -> Result<Self, String> {
        let index_manager = IndexManager::new().await?;
        
        Ok(Self {
            index_manager: Arc::new(Mutex::new(index_manager)),
        })
    }
}

fn create_context_menu() -> Menu {
    let debug = CustomMenuItem::new("debug", "Toggle Debug Tools");
    let debug_menu = Submenu::new("Debug", Menu::new().add_item(debug));
    Menu::new().add_submenu(debug_menu)
}

#[tokio::main]
async fn main() {
    // Initialize logging with debug level and detailed format
    env_logger::Builder::from_default_env()
        .format(|buf, record| {
            use std::io::Write;
            writeln!(buf,
                "[{} {} {}:{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            )
        })
        .filter_level(LevelFilter::Debug)
        .init();

    info!("Starting Constella application");
    info!("Initializing Tauri builder");

    let app_state = match AppState::new().await {
        Ok(state) => state,
        Err(e) => {
            error!("Failed to create application state: {}", e);
            std::process::exit(1);
        }
    };

    tauri::Builder::default()
        .menu(create_context_menu())
        .on_menu_event(|event| {
            match event.menu_item_id() {
                "debug" => {
                    info!("Debug menu item clicked");
                    let window = event.window();
                    window.open_devtools();
                    
                    // Toggle logging
                    let mut builder = env_logger::Builder::from_default_env();
                    if log::max_level() == LevelFilter::Off {
                        info!("Enabling detailed logging");
                        builder.filter_level(LevelFilter::Debug);
                    } else {
                        info!("Disabling logging");
                        builder.filter_level(LevelFilter::Off);
                    }
                    builder.init();
                }
                _ => {}
            }
        })
        .setup(|_app| {
            info!("Running Tauri setup");
            Ok(())
        })
        .manage(app_state.index_manager.clone())
        .invoke_handler(tauri::generate_handler![
            api::commands::select_directory,
            api::commands::start_indexing,
            api::commands::search_files,
            api::commands::verify_index,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
} 