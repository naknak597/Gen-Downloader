pub mod commands;
pub mod engine;
pub mod models;
pub mod state;

pub use commands::{
    cancel_download, get_media_info, get_playlist_items, open_media_file, show_in_folder,
    start_download,
};
pub use models::{DownloadPayload, DownloadRequest, MediaMetadata, PlaylistItem, ProgressPayload};
pub use state::DownloadManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(DownloadManager::default())
        .invoke_handler(tauri::generate_handler![
            commands::start_download,
            commands::cancel_download,
            commands::open_media_file,
            commands::show_in_folder,
            commands::get_playlist_items,
            commands::get_media_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
