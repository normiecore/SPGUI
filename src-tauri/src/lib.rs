//! Deja — a local memory and workflow layer for Screenpipe.
//!
//! The app is deliberately thin at the top and honest at the bottom: Screenpipe
//! records, Deja reads its SQLite database read-only, and every capability the
//! agent layer will need (capture, perception, execution) is an explicit
//! provider seam rather than an assumption baked into the code.

mod commands;
mod memory;
mod plugins;
mod settings;
mod status;

use std::sync::Mutex;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("app data dir is available");
            std::fs::create_dir_all(&data_dir).ok();

            let settings_path = data_dir.join("settings.json");
            let loaded = settings::load(&settings_path);

            let plugins_dir = data_dir.join("plugins");
            if let Err(err) = plugins::seed_example(&plugins_dir) {
                eprintln!("[deja] could not seed the example plugin: {err}");
            }
            let records = plugins::discover(&plugins_dir, &loaded.plugins_enabled);

            app.manage(commands::AppState {
                data_dir,
                settings: Mutex::new(loaded),
                plugins: Mutex::new(records),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_status,
            commands::get_timeline,
            commands::search_memory,
            commands::get_analytics,
            commands::get_settings,
            commands::save_settings,
            commands::list_plugins,
            commands::refresh_plugins,
            commands::set_plugin_enabled,
            commands::run_plugin,
            commands::seed_example_plugin,
            commands::open_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Deja");
}
