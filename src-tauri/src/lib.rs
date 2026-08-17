pub mod auth;
pub mod installer;
pub mod launcher;
pub mod modloaders;
pub mod smart;
pub mod instance;
pub mod advanced;
pub mod db;
pub mod utils;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
