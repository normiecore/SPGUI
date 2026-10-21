//! Tray icon: the app's real home for a recorder that runs all day.
//!
//! Closing the window hides it; quitting is an explicit action from the tray and
//! stops the recorder Deja owns.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::commands::AppState;

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Deja", true, None::<&str>)?;
    let restart = MenuItem::with_id(app, "restart", "Restart recorder", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Deja", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &restart, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id("deja")
        .tooltip("Deja — recording")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show(app),
            "restart" => {
                let state = app.state::<AppState>();
                crate::recorder::restart(&state.recorder);
            }
            "quit" => {
                let state = app.state::<AppState>();
                crate::recorder::stop(&state.recorder);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

pub fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
