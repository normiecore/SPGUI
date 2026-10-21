//! Plugin registry.
//!
//! A plugin is a folder under `<app data>/plugins/<id>/` containing a
//! `plugin.json` manifest and (optionally) an executable entry point. Deja runs
//! the entry point as a child process, hands it a JSON payload on stdin, and
//! reads JSON/text back from stdout. That keeps plugins language-agnostic —
//! Node, Python, a compiled binary, whatever is on PATH.
//!
//! Built-in providers (capture, perception, execution) are listed alongside
//! discovered plugins so the seams of the agent layer are visible, even where
//! the implementation does not exist yet.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

pub const RUN_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PluginRuntime {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// `digest` | `export` | `capture` | `perception` | `executor` | `view`
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub default_enabled: bool,
    #[serde(default)]
    pub runtime: Option<PluginRuntime>,
}

fn default_kind() -> String {
    "digest".to_string()
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub kind: String,
    pub capabilities: Vec<String>,
    pub dir: String,
    pub enabled: bool,
    /// `ready` | `needs-runtime` | `error` | `active` | `planned`
    pub status: String,
    pub problem: Option<String>,
    pub builtin: bool,
}

#[derive(Clone, Debug)]
pub struct PluginRecord {
    pub info: PluginInfo,
    pub manifest: Option<PluginManifest>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PluginRun {
    pub id: String,
    pub ok: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

/// Built-in providers that the future agent layer plugs into.
pub fn builtins() -> Vec<PluginInfo> {
    let mk = |id: &str, name: &str, kind: &str, status: &str, desc: &str, caps: &[&str]| PluginInfo {
        id: id.into(),
        name: name.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        description: desc.into(),
        kind: kind.into(),
        capabilities: caps.iter().map(|c| c.to_string()).collect(),
        dir: String::new(),
        enabled: status == "active",
        status: status.into(),
        problem: None,
        builtin: true,
    };

    vec![
        mk(
            "screenpipe",
            "Screenpipe capture",
            "capture",
            "active",
            "Records screen frames, UI events and audio through the bundled open-source Screenpipe engine.",
            &["capture:frames", "capture:ui-events", "capture:audio"],
        ),
        mk(
            "windows-uia",
            "Windows UI Automation",
            "perception",
            "planned",
            "Reads the foreground window and accessibility tree via UI Automation. Needed before Deja can decide when to act.",
            &["read:foreground", "read:a11y-tree"],
        ),
        mk(
            "sendinput",
            "Windows input executor",
            "executor",
            "planned",
            "Replays clicks and keystrokes with SendInput. Requires the perception provider and explicit user approval.",
            &["act:click", "act:keys"],
        ),
    ]
}

/// Load every plugin folder under `dir`, plus built-ins.
pub fn discover(dir: &Path, enabled: &BTreeMap<String, bool>) -> Vec<PluginRecord> {
    let mut records = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();

        for plugin_dir in dirs {
            let manifest_path = plugin_dir.join("plugin.json");
            let folder_name = plugin_dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if !manifest_path.is_file() {
                continue;
            }

            match std::fs::read_to_string(&manifest_path)
                .map_err(|e| e.to_string())
                .and_then(|raw| serde_json::from_str::<PluginManifest>(&raw).map_err(|e| e.to_string()))
            {
                Ok(manifest) => {
                    let id = if manifest.id.trim().is_empty() {
                        folder_name.clone()
                    } else {
                        manifest.id.clone()
                    };
                    let has_runtime = manifest
                        .runtime
                        .as_ref()
                        .map(|r| !r.command.trim().is_empty())
                        .unwrap_or(false);

                    let info = PluginInfo {
                        id: id.clone(),
                        name: manifest.name.clone(),
                        version: manifest.version.clone(),
                        description: manifest.description.clone(),
                        kind: manifest.kind.clone(),
                        capabilities: manifest.capabilities.clone(),
                        dir: plugin_dir.display().to_string(),
                        enabled: *enabled.get(&id).unwrap_or(&manifest.default_enabled),
                        status: if has_runtime { "ready" } else { "needs-runtime" }.into(),
                        problem: None,
                        builtin: false,
                    };
                    records.push(PluginRecord {
                        info,
                        manifest: Some(manifest),
                    });
                }
                Err(problem) => {
                    records.push(PluginRecord {
                        info: PluginInfo {
                            id: folder_name.clone(),
                            name: folder_name,
                            version: String::new(),
                            description: "Manifest could not be read.".into(),
                            kind: "unknown".into(),
                            capabilities: Vec::new(),
                            dir: plugin_dir.display().to_string(),
                            enabled: false,
                            status: "error".into(),
                            problem: Some(problem),
                            builtin: false,
                        },
                        manifest: None,
                    });
                }
            }
        }
    }

    let mut out: Vec<PluginRecord> = builtins()
        .into_iter()
        .map(|info| PluginRecord {
            info,
            manifest: None,
        })
        .collect();
    out.extend(records);
    out
}

/// Execute a plugin with `input` on stdin.
pub fn run(record: &PluginRecord, data_dir: &Path, db_path: &Path, input: &str) -> Result<PluginRun, String> {
    if !record.info.enabled {
        return Err(format!("Plugin '{}' is disabled.", record.info.name));
    }
    let manifest = record
        .manifest
        .as_ref()
        .ok_or_else(|| format!("Plugin '{}' has no readable manifest.", record.info.name))?;
    let runtime = manifest
        .runtime
        .as_ref()
        .filter(|r| !r.command.trim().is_empty())
        .ok_or_else(|| format!("Plugin '{}' declares no runtime command.", record.info.name))?;

    let started = Instant::now();
    let mut cmd = Command::new(&runtime.command);
    cmd.args(&runtime.args)
        .current_dir(&record.info.dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("DEJA_PLUGIN_DIR", &record.info.dir)
        .env("DEJA_DATA_DIR", data_dir)
        .env("DEJA_FRAMES_DB", db_path);
    for (key, value) in &runtime.env {
        cmd.env(key, value);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start '{}': {e}", runtime.command))?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }

    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });

    match rx.recv_timeout(RUN_TIMEOUT) {
        Ok(Ok(output)) => Ok(PluginRun {
            id: record.info.id.clone(),
            ok: output.status.success(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration_ms: started.elapsed().as_millis() as u64,
        }),
        Ok(Err(err)) => Err(format!("Plugin '{}' failed: {err}", record.info.name)),
        Err(_) => {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            Err(format!(
                "Plugin '{}' did not finish within {}s and was stopped.",
                record.info.name,
                RUN_TIMEOUT.as_secs()
            ))
        }
    }
}

const EXAMPLE_MANIFEST: &str = include_str!("../../plugins/daily-digest/plugin.json");
const EXAMPLE_ENTRY: &str = include_str!("../../plugins/daily-digest/index.mjs");

/// Drop the reference plugin into the plugins folder on first run.
pub fn seed_example(plugins_dir: &Path) -> Result<PathBuf, String> {
    let target = plugins_dir.join("daily-digest");
    std::fs::create_dir_all(&target)
        .map_err(|e| format!("could not create {}: {e}", target.display()))?;

    let manifest = target.join("plugin.json");
    if !manifest.is_file() {
        std::fs::write(&manifest, EXAMPLE_MANIFEST)
            .map_err(|e| format!("could not write {}: {e}", manifest.display()))?;
    }
    let entry = target.join("index.mjs");
    if !entry.is_file() {
        std::fs::write(&entry, EXAMPLE_ENTRY)
            .map_err(|e| format!("could not write {}: {e}", entry.display()))?;
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_expose_the_agent_seams() {
        let kinds: Vec<String> = builtins().into_iter().map(|p| p.kind).collect();
        assert!(kinds.contains(&"capture".to_string()));
        assert!(kinds.contains(&"perception".to_string()));
        assert!(kinds.contains(&"executor".to_string()));
    }

    #[test]
    fn example_manifest_parses() {
        let manifest: PluginManifest = serde_json::from_str(EXAMPLE_MANIFEST).expect("example manifest must parse");
        assert_eq!(manifest.id, "daily-digest");
    }
}
