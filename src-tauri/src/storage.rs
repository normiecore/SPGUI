//! Where the disk went.
//!
//! Screenpipe stores two very different things: a SQLite database (mostly text)
//! and a `data/` tree (JPEG frames + audio chunks). This module measures both.
//!
//! It is deliberately on-demand: walking tens of thousands of files is too
//! expensive to run on the 15 s status poll.

use std::collections::BTreeMap;
use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TableSize {
    pub name: String,
    pub bytes: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ColumnBytes {
    pub label: String,
    pub bytes: u64,
    pub rows: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DayMedia {
    pub date: String,
    pub bytes: u64,
    pub files: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StorageReport {
    pub data_dir: String,
    pub db_bytes: u64,
    pub wal_bytes: u64,
    pub media_bytes: u64,
    pub total_bytes: u64,
    pub frame_files: u64,
    pub frame_bytes: u64,
    pub audio_files: u64,
    pub audio_bytes: u64,
    pub tables: Vec<TableSize>,
    pub columns: Vec<ColumnBytes>,
    pub media_by_day: Vec<DayMedia>,
    /// Bytes in `frames.accessibility_tree_json` — duplicated by `elements`.
    pub a11y_json_bytes: u64,
    pub a11y_frames: u64,
    pub frames_rows: u64,
    /// Media older than 30 days.
    pub media_over_30d_bytes: u64,
    pub dbstat: bool,
}

fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn table_sizes(conn: &Connection) -> (Vec<TableSize>, bool) {
    // dbstat is a SQLite compile-time option; fall back rather than fail.
    let Ok(mut stmt) = conn.prepare(
        "SELECT name, SUM(pgsize) FROM dbstat
          WHERE name NOT LIKE 'sqlite_%'
          GROUP BY name
          ORDER BY 2 DESC
          LIMIT 14",
    ) else {
        return (Vec::new(), false);
    };

    let rows = stmt.query_map([], |row| {
        Ok(TableSize {
            name: row.get(0)?,
            bytes: row.get::<_, i64>(1).unwrap_or(0).max(0) as u64,
        })
    });

    match rows {
        Ok(iter) => (iter.flatten().collect(), true),
        Err(_) => (Vec::new(), false),
    }
}

fn column_bytes(conn: &Connection) -> Vec<ColumnBytes> {
    let mut out = Vec::new();

    let sums: [(&str, &str); 3] = [
        (
            "frames.accessibility_tree_json",
            "SELECT COALESCE(SUM(LENGTH(accessibility_tree_json)), 0), COUNT(accessibility_tree_json) FROM frames",
        ),
        (
            "frames.full_text",
            "SELECT COALESCE(SUM(LENGTH(full_text)), 0), COUNT(full_text) FROM frames",
        ),
        (
            "frames.text_json",
            "SELECT COALESCE(SUM(LENGTH(text_json)), 0), COUNT(text_json) FROM frames",
        ),
    ];

    for (label, sql) in sums {
        if let Ok((bytes, rows)) = conn.query_row(sql, [], |row| {
            Ok((row.get::<_, i64>(0).unwrap_or(0).max(0) as u64, row.get::<_, i64>(1).unwrap_or(0) as u64))
        }) {
            out.push(ColumnBytes {
                label: label.into(),
                bytes,
                rows,
            });
        }
    }

    if let Ok((bytes, rows)) = conn.query_row(
        "SELECT COALESCE(SUM(LENGTH(text)), 0), COUNT(*) FROM elements",
        [],
        |row| {
            Ok((
                row.get::<_, i64>(0).unwrap_or(0).max(0) as u64,
                row.get::<_, i64>(1).unwrap_or(0) as u64,
            ))
        },
    ) {
        out.push(ColumnBytes {
            label: "elements.text".into(),
            bytes,
            rows,
        });
    }

    out
}

fn date_of(path: &Path) -> Option<String> {
    // Frames live in YYYY-MM-DD folders; audio files carry _YYYY-MM-DD_ in the name.
    for part in path.iter().rev() {
        let text = part.to_string_lossy();
        if let Some(idx) = text.find(|c: char| c.is_ascii_digit()) {
            let tail = &text[idx..];
            if tail.len() >= 10
                && tail.as_bytes()[4] == b'-'
                && tail.as_bytes()[7] == b'-'
                && tail[..4].chars().all(|c| c.is_ascii_digit())
            {
                return Some(tail[..10].to_string());
            }
        }
    }
    None
}

pub fn report(conn: &Connection, data_dir: &Path) -> StorageReport {
    let db_path = data_dir.join("db.sqlite");
    let db_bytes = file_len(&db_path);
    let wal_bytes = file_len(&data_dir.join("db.sqlite-wal"));

    let mut frame_files = 0u64;
    let mut frame_bytes = 0u64;
    let mut audio_files = 0u64;
    let mut audio_bytes = 0u64;
    let mut other_bytes = 0u64;
    let mut by_day: BTreeMap<String, DayMedia> = BTreeMap::new();

    let media_root = data_dir.join("data");
    let mut stack = vec![media_root.clone()];
    let mut seen = 0u32;
    let cutoff = (chrono::Local::now() - chrono::Duration::days(30))
        .format("%Y-%m-%d")
        .to_string();
    let mut old_bytes = 0u64;

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if seen > 200_000 {
                break;
            }
            seen += 1;
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };

            if meta.is_dir() {
                stack.push(path);
                continue;
            }

            let len = meta.len();
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();

            match ext.as_str() {
                "jpg" | "jpeg" | "png" | "webp" => {
                    frame_files += 1;
                    frame_bytes += len;
                }
                "mp4" | "wav" | "mp3" | "flac" | "ogg" => {
                    audio_files += 1;
                    audio_bytes += len;
                }
                _ => other_bytes += len,
            }

            if let Some(date) = date_of(&path) {
                if date.as_str() < cutoff.as_str() {
                    old_bytes += len;
                }
                let entry = by_day.entry(date.clone()).or_insert(DayMedia {
                    date,
                    bytes: 0,
                    files: 0,
                });
                entry.bytes += len;
                entry.files += 1;
            }
        }
    }

    let media_bytes = frame_bytes + audio_bytes + other_bytes;
    let (tables, dbstat) = table_sizes(conn);
    let columns = column_bytes(conn);

    let a11y_json_bytes = columns
        .iter()
        .find(|c| c.label == "frames.accessibility_tree_json")
        .map(|c| c.bytes)
        .unwrap_or(0);
    let a11y_frames = columns
        .iter()
        .find(|c| c.label == "frames.accessibility_tree_json")
        .map(|c| c.rows)
        .unwrap_or(0);
    let frames_rows = conn
        .query_row("SELECT COUNT(*) FROM frames", [], |row| row.get::<_, i64>(0))
        .unwrap_or(0)
        .max(0) as u64;

    let mut media_by_day: Vec<DayMedia> = by_day.into_values().collect();
    media_by_day.sort_by(|a, b| b.date.cmp(&a.date));

    StorageReport {
        data_dir: data_dir.display().to_string(),
        db_bytes,
        wal_bytes,
        media_bytes,
        total_bytes: db_bytes + wal_bytes + media_bytes,
        frame_files,
        frame_bytes,
        audio_files,
        audio_bytes,
        tables,
        columns,
        media_by_day,
        a11y_json_bytes,
        a11y_frames,
        frames_rows,
        media_over_30d_bytes: old_bytes,
        dbstat,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_dates_from_screenpipe_paths() {
        let frame = Path::new(r"C:\Users\x\.screenpipe\data\data\2026-10-21\1792555393457_m65537.jpg");
        assert_eq!(date_of(frame).as_deref(), Some("2026-10-21"));

        let audio = Path::new(
            r"C:\Users\x\.screenpipe\data\Microphone (Realtek(R) Audio) (input)_2026-10-21_04-48-11.mp4",
        );
        assert_eq!(date_of(audio).as_deref(), Some("2026-10-21"));

        assert_eq!(date_of(Path::new(r"C:\temp\notes.txt")), None);
    }
}
