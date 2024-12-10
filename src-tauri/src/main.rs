#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{CustomMenuItem, Menu, Submenu, Manager};
use crate::indexing::IndexManager;
use env_logger;
use log::{LevelFilter, info, debug};
use chrono;

pub mod api;
pub mod file_system;
pub mod indexing;
pub mod utils;
pub mod benchmarking;

pub struct AppState {
    pub indexer: Arc<Mutex<IndexManager>>,
    pub app_handle: Arc<tauri::AppHandle>,
}

impl AppState {
    pub fn new(indexer: IndexManager, app_handle: tauri::AppHandle) -> Self {
        Self {
            indexer: Arc::new(Mutex::new(indexer)),
            app_handle: Arc::new(app_handle),
        }
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
                "[{} {} {} {}:{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                record.target(),
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                record.args()
            )
        })
        .filter_level(LevelFilter::Debug)
        .filter_module("constella", LevelFilter::Debug)
        .filter_module("tantivy", LevelFilter::Info)
        .filter_module("ignore", LevelFilter::Info)
        .init();

    debug!("Starting Constella application with debug logging enabled");
    
    // Create index manager first
    let index_manager = IndexManager::new()
        .await
        .expect("Failed to create index manager");

    debug!("Index manager created successfully");

    info!("Initializing Tauri builder");

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
        .setup(|app| {
            info!("Running Tauri setup");
            
            let main_window = app.get_window("main")
                .unwrap_or_else(|| {
                    tauri::WindowBuilder::new(
                        app,
                        "main",
                        tauri::WindowUrl::App("index.html".into())
                    )
                    .title("Constella")
                    .inner_size(800.0, 600.0)
                    .build()
                    .expect("Failed to create main window")
                });
            
            // Handle window close event
            let window_clone = main_window.clone();
            main_window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { .. } = event {
                    info!("Window close requested - hiding window");
                    let _ = window_clone.hide();
                }
            });
            
            // Create app state
            let state = AppState::new(index_manager, app.handle());
            app.manage(state);
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api::commands::select_directory,
            api::commands::start_indexing,
            api::commands::search_files,
            api::commands::verify_index,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
} 