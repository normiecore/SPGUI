//! Tauri command surface. Everything the UI can do goes through here.

use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;

use crate::memory::{
    self,
    analytics::Analytics,
    search::SearchHit,
    timeline::{TimelinePage, TimelineSession},
};
use crate::plugins::{self, PluginInfo, PluginRecord, PluginRun};
use crate::settings::{self, Settings};
use crate::status::{self, Status};

pub struct AppState {
    /// `<app data>/com.normiecore.deja`
    pub data_dir: PathBuf,
    pub settings: Mutex<Settings>,
    pub plugins: Mutex<Vec<PluginRecord>>,
}

impl AppState {
    pub fn settings_snapshot(&self) -> Settings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn settings_path(&self) -> PathBuf {
        self.data_dir.join("settings.json")
    }

    pub fn plugins_dir(&self) -> PathBuf {
        self.data_dir.join("plugins")
    }

    pub fn data_dir_override(&self) -> Option<String> {
        let dir = self.settings_snapshot().data_dir;
        if dir.trim().is_empty() {
            None
        } else {
            Some(dir)
        }
    }

    pub fn screenpipe_dir(&self) -> PathBuf {
        memory::data_dir(self.data_dir_override().as_deref())
    }

    pub fn db_path(&self) -> PathBuf {
        memory::db_path(self.data_dir_override().as_deref())
    }

    pub fn rediscover(&self) {
        let settings = self.settings_snapshot();
        let records = plugins::discover(&self.plugins_dir(), &settings.plugins_enabled);
        if let Ok(mut guard) = self.plugins.lock() {
            *guard = records;
        }
    }

    pub fn plugin_infos(&self) -> Vec<PluginInfo> {
        self.plugins
            .lock()
            .map(|list| list.iter().map(|r| r.info.clone()).collect())
            .unwrap_or_default()
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub data_dir: String,
    pub plugins_dir: String,
    pub settings_path: String,
    pub screenpipe_dir: String,
    pub db_path: String,
    pub db_exists: bool,
}

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        name: "Deja".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: state.data_dir.display().to_string(),
        plugins_dir: state.plugins_dir().display().to_string(),
        settings_path: state.settings_path().display().to_string(),
        screenpipe_dir: state.screenpipe_dir().display().to_string(),
        db_path: state.db_path().display().to_string(),
        db_exists: state.db_path().is_file(),
    }
}

#[tauri::command]
pub fn get_status(state: State<'_, AppState>, port: Option<u16>) -> Status {
    let port = port.unwrap_or(3030);
    status::probe(port, &state.screenpipe_dir(), &state.db_path())
}

#[tauri::command]
pub fn get_timeline(
    state: State<'_, AppState>,
    days: Option<i64>,
    limit: Option<i64>,
) -> Result<TimelinePage, String> {
    let settings = state.settings_snapshot();
    let conn = memory::open(state.data_dir_override().as_deref())?;
    let mut page = memory::timeline::page(
        &conn,
        days.unwrap_or(settings.days_default),
        limit.unwrap_or(80),
    )?;

    let before = page.sessions.len();
    page.sessions
        .retain(|session: &TimelineSession| !settings::is_excluded(&settings, &session.app));
    if page.sessions.len() != before {
        page.total_sessions = page.total_sessions.saturating_sub(before - page.sessions.len());
    }
    Ok(page)
}

#[tauri::command]
pub fn search_memory(
    state: State<'_, AppState>,
    query: String,
    days: Option<i64>,
    kinds: Option<Vec<String>>,
    limit: Option<i64>,
) -> Result<Vec<SearchHit>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let settings = state.settings_snapshot();
    let conn = memory::open(state.data_dir_override().as_deref())?;
    memory::search::search(
        &conn,
        &query,
        days.unwrap_or(settings.days_default),
        &kinds.unwrap_or_default(),
        limit.unwrap_or(50),
    )
}

#[tauri::command]
pub fn get_analytics(state: State<'_, AppState>, days: Option<i64>) -> Result<Analytics, String> {
    let settings = state.settings_snapshot();
    let conn = memory::open(state.data_dir_override().as_deref())?;
    memory::analytics::compute(&conn, days.unwrap_or(settings.days_default))
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings_snapshot()
}

#[tauri::command]
pub fn save_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings, String> {
    settings::save(&state.settings_path(), &settings)?;
    if let Ok(mut guard) = state.settings.lock() {
        *guard = settings.clone();
    }
    state.rediscover();
    Ok(settings)
}

#[tauri::command]
pub fn list_plugins(state: State<'_, AppState>) -> Vec<PluginInfo> {
    state.plugin_infos()
}

#[tauri::command]
pub fn refresh_plugins(state: State<'_, AppState>) -> Vec<PluginInfo> {
    state.rediscover();
    state.plugin_infos()
}

#[tauri::command]
pub fn set_plugin_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<Vec<PluginInfo>, String> {
    {
        let mut settings = state
            .settings
            .lock()
            .map_err(|_| "settings lock poisoned".to_string())?;
        settings.plugins_enabled.insert(id, enabled);
        settings::save(&state.settings_path(), &settings)?;
    }
    state.rediscover();
    Ok(state.plugin_infos())
}

/// Run a plugin. With no explicit `input`, Deja builds a payload from the
/// current analytics so digest-style plugins work out of the box.
#[tauri::command]
pub fn run_plugin(
    state: State<'_, AppState>,
    id: String,
    input: Option<String>,
) -> Result<PluginRun, String> {
    let record = {
        let guard = state.plugins.lock().map_err(|_| "plugin lock poisoned".to_string())?;
        guard
            .iter()
            .find(|r| r.info.id == id)
            .cloned()
            .ok_or_else(|| format!("No plugin with id '{id}'."))?
    };

    let payload = match input {
        Some(raw) if !raw.trim().is_empty() => raw,
        _ => {
            let settings_snapshot = state.settings_snapshot();
            let analytics = memory::open(state.data_dir_override().as_deref())
                .and_then(|conn| memory::analytics::compute(&conn, settings_snapshot.days_default))
                .ok();
            let status = status::probe(3030, &state.screenpipe_dir(), &state.db_path());
            serde_json::json!({
                "generatedAt": chrono::Local::now().to_rfc3339(),
                "settings": settings_snapshot,
                "analytics": analytics,
                "status": status,
            })
            .to_string()
        }
    };

    plugins::run(
        &record,
        &state.data_dir,
        &state.db_path(),
        &payload,
    )
}

#[tauri::command]
pub fn seed_example_plugin(state: State<'_, AppState>) -> Result<String, String> {
    let dir = plugins::seed_example(&state.plugins_dir())?;
    state.rediscover();
    Ok(dir.display().to_string())
}

/// Open a folder or file in Explorer, or a URL in the default browser.
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Nothing to open.".into());
    }

    #[cfg(windows)]
    {
        let mut cmd = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            let mut c = std::process::Command::new("rundll32.exe");
            c.arg("url.dll,FileProtocolHandler").arg(trimmed);
            c
        } else {
            let mut c = std::process::Command::new("explorer.exe");
            c.arg(trimmed);
            c
        };
        cmd.spawn().map_err(|e| format!("could not open: {e}"))?;
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(trimmed)
            .spawn()
            .map_err(|e| format!("could not open: {e}"))?;
        Ok(())
    }
}
