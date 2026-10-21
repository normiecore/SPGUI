//! User settings, persisted as JSON in the app data directory.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Blank means "use Screenpipe's default data directory".
    pub data_dir: String,
    pub days_default: i64,
    pub excluded_apps: Vec<String>,
    pub theme: String,
    /// Plugin id → enabled. Missing entries fall back to the manifest default.
    pub plugins_enabled: BTreeMap<String, bool>,
    /// Reserved for the future agent layer; the key itself is stored here.
    pub llm_model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            data_dir: String::new(),
            days_default: 7,
            excluded_apps: vec!["deja.exe".into()],
            theme: "dark".into(),
            plugins_enabled: BTreeMap::new(),
            llm_model: String::new(),
        }
    }
}

pub fn load(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Settings>(&raw).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("could not write {}: {e}", path.display()))
}

pub fn is_excluded(settings: &Settings, app: &str) -> bool {
    let app = app.to_ascii_lowercase();
    settings
        .excluded_apps
        .iter()
        .any(|ex| !ex.trim().is_empty() && app == ex.trim().to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let s = Settings::default();
        assert_eq!(s.days_default, 7);
        assert!(s.data_dir.is_empty());
    }

    #[test]
    fn exclusion_is_case_insensitive() {
        let mut s = Settings::default();
        s.excluded_apps.push("Brave.exe".into());
        assert!(is_excluded(&s, "brave.exe"));
        assert!(!is_excluded(&s, "code.exe"));
    }
}
