//! Live status of the Screenpipe recorder, plus on-disk footprint.
//!
//! Screenpipe exposes `GET /health` on its local API (default port 3030) with a
//! very detailed payload. Deja only surfaces the fields a human cares about.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub online: bool,
    pub error: Option<String>,
    pub version: Option<String>,
    pub message: Option<String>,
    pub frames_captured: u64,
    pub frames_written: u64,
    pub capture_state: Option<String>,
    pub transcription_engine: Option<String>,
    pub transcriptions: u64,
    pub last_frame_ts: Option<String>,
    pub uptime_seconds: u64,
    pub monitors: Vec<String>,
    pub storage_bytes: u64,
    pub db_bytes: u64,
    pub db_path: String,
    pub data_dir: String,
}

impl Status {
    pub fn offline(error: impl Into<String>, data_dir: &Path, db_path: &Path) -> Self {
        Self {
            online: false,
            error: Some(error.into()),
            version: None,
            message: None,
            frames_captured: 0,
            frames_written: 0,
            capture_state: None,
            transcription_engine: None,
            transcriptions: 0,
            last_frame_ts: None,
            uptime_seconds: 0,
            monitors: Vec::new(),
            storage_bytes: dir_size(data_dir),
            db_bytes: file_size(db_path),
            db_path: db_path.display().to_string(),
            data_dir: data_dir.display().to_string(),
        }
    }
}

fn num(v: &serde_json::Value, path: &[&str]) -> u64 {
    let mut cur = v;
    for key in path {
        cur = match cur.get(*key) {
            Some(next) => next,
            None => return 0,
        };
    }
    cur.as_u64().unwrap_or(0)
}

fn text(v: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut cur = v;
    for key in path {
        cur = cur.get(*key)?;
    }
    match cur {
        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

pub fn probe(port: u16, data_dir: &Path, db_path: &Path) -> Status {
    let url = format!("http://127.0.0.1:{port}/health");

    let agent = match ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(900))
        .timeout(Duration::from_millis(2_500))
        .build()
        .get(&url)
        .call()
    {
        Ok(resp) => resp,
        Err(err) => {
            return Status::offline(
                format!("Screenpipe is not answering on port {port} ({err}). Start Screenpipe, then refresh."),
                data_dir,
                db_path,
            )
        }
    };

    let body: serde_json::Value = match agent.into_json() {
        Ok(v) => v,
        Err(err) => return Status::offline(format!("Screenpipe returned unreadable health data: {err}"), data_dir, db_path),
    };

    let monitors = body
        .get("monitors")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Status {
        online: body
            .get("status")
            .and_then(|s| s.as_str())
            .map(|s| s.eq_ignore_ascii_case("healthy"))
            .unwrap_or(true),
        error: None,
        version: text(&body, &["version"]),
        message: text(&body, &["message"]),
        frames_captured: num(&body, &["pipeline", "frames_captured"]),
        frames_written: num(&body, &["pipeline", "frames_db_written"]),
        capture_state: text(&body, &["capture_status", "status"]),
        transcription_engine: text(&body, &["dependencies", "models", "transcription", "engine"]),
        transcriptions: num(&body, &["audio_pipeline", "transcriptions_completed"]),
        last_frame_ts: text(&body, &["last_frame_timestamp"]),
        uptime_seconds: num(&body, &["pipeline", "uptime_secs"]),
        monitors,
        storage_bytes: dir_size(data_dir),
        db_bytes: file_size(db_path),
        db_path: db_path.display().to_string(),
        data_dir: data_dir.display().to_string(),
    }
}

pub fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Total size of a directory tree, bounded so a huge history cannot stall the UI.
pub fn dir_size(root: &Path) -> u64 {
    fn walk(dir: &Path, total: &mut u64, seen: &mut u32) {
        if *seen > 50_000 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            *seen += 1;
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                walk(&entry.path(), total, seen);
            } else {
                *total += meta.len();
            }
        }
    }

    let mut total = 0;
    let mut seen = 0;
    walk(root, &mut total, &mut seen);
    total
}
