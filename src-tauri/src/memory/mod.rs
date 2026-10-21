//! Read-only access to Screenpipe's local SQLite database.
//!
//! Screenpipe owns this file — Deja never writes to it. Every query opens a
//! short-lived read-only connection so a long-running window cannot hold a
//! lock against the recorder.

pub mod analytics;
pub mod search;
pub mod timeline;

use std::path::PathBuf;

use rusqlite::{Connection, OpenFlags};

/// Where Screenpipe keeps its data.
///
/// Resolution order: explicit setting → `SCREENPIPE_DATA_DIR` → `~/.screenpipe`.
pub fn data_dir(override_dir: Option<&str>) -> PathBuf {
    if let Some(dir) = override_dir.map(str::trim).filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    if let Ok(dir) = std::env::var("SCREENPIPE_DATA_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    home_dir().join(".screenpipe")
}

pub fn home_dir() -> PathBuf {
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub fn db_path(override_dir: Option<&str>) -> PathBuf {
    data_dir(override_dir).join("db.sqlite")
}

/// Open the Screenpipe database read-only.
pub fn open(override_dir: Option<&str>) -> Result<Connection, String> {
    let path = db_path(override_dir);
    if !path.is_file() {
        return Err(format!(
            "Screenpipe database not found at {}. Start Screenpipe and record for a minute, or point Deja at the right folder in Settings.",
            path.display()
        ));
    }
    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Could not open {}: {e}", path.display()))?;
    conn.busy_timeout(std::time::Duration::from_millis(3_000))
        .map_err(|e| format!("Could not configure the database connection: {e}"))?;
    Ok(conn)
}

/// SQLite interval modifier for "the last N days".
pub fn days_modifier(days: i64) -> String {
    format!("-{} days", days.clamp(1, 3650))
}

/// Collapse whitespace and clamp a blob of OCR text to something a row can show.
pub fn snippet(text: &str, max_chars: usize) -> String {
    let mut out = String::with_capacity(max_chars + 8);
    let mut last_space = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_space && !out.is_empty() {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(ch);
            last_space = false;
        }
        if out.chars().count() >= max_chars {
            out.push('…');
            break;
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_collapses_whitespace() {
        assert_eq!(snippet("a\n\n  b\tc", 40), "a b c");
    }

    #[test]
    fn snippet_truncates_with_ellipsis() {
        let out = snippet(&"x".repeat(100), 10);
        assert!(out.ends_with('…'));
        assert_eq!(out.chars().count(), 11);
    }

    #[test]
    fn days_modifier_is_clamped() {
        assert_eq!(days_modifier(0), "-1 days");
        assert_eq!(days_modifier(7), "-7 days");
        assert_eq!(days_modifier(99_999), "-3650 days");
    }

    /// Smoke test against a real Screenpipe install, when there is one.
    ///
    /// The point is schema drift: a Screenpipe upgrade that renames or nulls a
    /// column should surface here, not as an empty view at runtime.
    #[test]
    fn live_database_smoke() {
        if !db_path(None).is_file() {
            eprintln!("skipping: no Screenpipe database on this machine");
            return;
        }
        let conn = open(None).expect("read-only open must succeed");

        let frames: i64 = conn
            .query_row("SELECT COUNT(*) FROM frames", [], |row| row.get(0))
            .expect("frames table must be queryable");
        if frames == 0 {
            eprintln!("skipping deeper checks: database has no frames yet");
            return;
        }

        let sessions = timeline::sessions(&conn, 7, 20).expect("timeline query");
        assert!(!sessions.is_empty(), "frames exist but no session was built");

        let hits = search::search(&conn, "the", 7, &[], 20).expect("search query");
        assert!(hits.len() <= 20, "search must respect the limit");

        let analytics = analytics::compute(&conn, 7).expect("analytics query");
        assert!(analytics.total_frames > 0);
        assert_eq!(analytics.hourly.len(), 24);
    }
}
