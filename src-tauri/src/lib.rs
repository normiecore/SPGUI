//! Deja — a local memory and workflow layer for Screenpipe.
//!
//! For unattended daily use the app has to own three things: the recorder
//! process, the window lifecycle (tray, hide-on-close, start at login), and its
//! own logs. Everything else is read-only over Screenpipe's database.

mod commands;
mod flows;
mod logging;
mod memory;
mod plugins;
mod recorder;
mod settings;
mod status;
mod storage;
mod tray;

use std::sync::{Arc, Mutex};

use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be registered first: a second launch should focus the first one,
        // never fight it for the WebView2 user-data folder.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("app data dir is available");
            std::fs::create_dir_all(&data_dir).ok();

            logging::init(data_dir.join("deja.log"));
            logging::log(format!("Deja {} starting", env!("CARGO_PKG_VERSION")));

            let settings_path = data_dir.join("settings.json");
            let loaded = settings::load(&settings_path);

            let plugins_dir = data_dir.join("plugins");
            if let Err(err) = plugins::seed_example(&plugins_dir) {
                logging::log(format!("could not seed the example plugin: {err}"));
            }
            let records = plugins::discover(&plugins_dir, &loaded.plugins_enabled);

            let recorder = Arc::new(Mutex::new(recorder::Recorder::new(
                3030,
                loaded.retention_days,
                Some(data_dir.join("screenpipe.log")),
            )));
            recorder::start(Arc::clone(&recorder));

            let autostart = loaded.autostart;
            app.manage(commands::AppState {
                data_dir,
                settings: Mutex::new(loaded),
                plugins: Mutex::new(records),
                recorder,
            });

            // Keep the login item in sync with the setting.
            {
                use tauri_plugin_autostart::ManagerExt;
                // Rewrite unconditionally: the registered path must follow the
                // installed binary, not wherever the app last ran from.
                let manager = app.autolaunch();
                if autostart {
                    let _ = manager.enable();
                } else {
                    let _ = manager.disable();
                }
            }

            tray::build(app.handle())?;

            // Launched by the login item: stay in the tray, don't steal focus.
            if std::env::args().any(|arg| arg == "--hidden") {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window parks the app in the tray; quitting is explicit.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_status,
            commands::get_timeline,
            commands::search_memory,
            commands::get_analytics,
            commands::get_flows,
            commands::get_storage,
            commands::get_settings,
            commands::save_settings,
            commands::list_plugins,
            commands::refresh_plugins,
            commands::set_plugin_enabled,
            commands::run_plugin,
            commands::seed_example_plugin,
            commands::open_path,
            commands::recorder_status,
            commands::recorder_start,
            commands::recorder_stop,
            commands::recorder_restart,
            commands::set_retention_days,
            commands::set_autostart,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Deja")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let state = app.state::<commands::AppState>();
                recorder::stop(&state.recorder);
                logging::log("Deja exiting — recorder stopped");
            }
        });
}
