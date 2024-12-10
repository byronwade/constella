use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    
    // Ensure OUT_DIR is set
    let out_dir = env::var("OUT_DIR").unwrap_or_else(|_| {
        let manifest_dir = env::var("CARGO_MANIFEST_DIR")
            .expect("CARGO_MANIFEST_DIR not set");
        PathBuf::from(manifest_dir)
            .join("target")
            .join("out")
            .to_string_lossy()
            .to_string()
    });
    
    // Set environment variables needed by Tauri
    env::set_var("TAURI_OUT_DIR", &out_dir);
    
    // Run Tauri build
    tauri_build::build()
}
