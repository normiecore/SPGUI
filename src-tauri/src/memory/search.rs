//! Full-text search across Screenpipe's FTS5 indexes.
//!
//! Three content sources are searched independently and merged:
//!   * `frames_fts`      — OCR + accessibility text of every captured frame
//!   * `ui_events_fts`   — typed text, element names, window titles
//!   * `audio_transcriptions_fts` — speech-to-text segments
//!
//! User input is never handed to FTS5 raw: bare tokens are quoted and
//! prefix-matched (`"foo"*`) so punctuation cannot produce a syntax error.

use rusqlite::Connection;
use serde::Serialize;

use super::days_modifier;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    /// `frame`, `event` or `audio`.
    pub kind: String,
    pub id: i64,
    pub ts: String,
    pub app: String,
    pub title: String,
    pub url: Option<String>,
    pub snippet: String,
    /// Lower is better (bm25). `0.0` when the hit came from the LIKE fallback.
    pub score: f64,
}

pub const ALL_KINDS: [&str; 3] = ["frame", "event", "audio"];

/// Build a safe FTS5 query: every token quoted, prefix-matched, OR-joined.
fn fts_query(raw: &str) -> Option<String> {
    let tokens: Vec<String> = raw
        .split_whitespace()
        .map(|t| {
            t.chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect::<String>()
        })
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"*", t.replace('"', "\"\"")))
        .collect();

    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" OR "))
    }
}

pub fn search(
    conn: &Connection,
    raw: &str,
    days: i64,
    kinds: &[String],
    limit: i64,
) -> Result<Vec<SearchHit>, String> {
    let limit = limit.clamp(1, 200);
    let modifier = days_modifier(days);

    let wanted: Vec<&str> = if kinds.is_empty() {
        ALL_KINDS.to_vec()
    } else {
        kinds
            .iter()
            .map(String::as_str)
            .filter(|k| ALL_KINDS.contains(k))
            .collect()
    };
    if wanted.is_empty() {
        return Ok(Vec::new());
    }

    let Some(fts) = fts_query(raw) else {
        return Ok(Vec::new());
    };

    let mut hits: Vec<SearchHit> = Vec::new();

    if wanted.contains(&"frame") {
        hits.extend(search_frames(conn, &fts, raw, &modifier, limit)?);
    }
    if wanted.contains(&"event") {
        hits.extend(search_events(conn, &fts, raw, &modifier, limit)?);
    }
    if wanted.contains(&"audio") {
        hits.extend(search_audio(conn, &fts, raw, &modifier, limit)?);
    }

    hits.sort_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(limit as usize);
    Ok(hits)
}

fn search_frames(
    conn: &Connection,
    fts: &str,
    raw: &str,
    modifier: &str,
    limit: i64,
) -> Result<Vec<SearchHit>, String> {
    const SQL: &str = "SELECT f.id,
                              f.timestamp,
                              COALESCE(f.app_name, 'unknown'),
                              COALESCE(f.window_name, ''),
                              f.browser_url,
                              snippet(frames_fts, 0, '«', '»', '…', 28),
                              bm25(frames_fts)
                         FROM frames_fts
                         JOIN frames f ON f.id = frames_fts.rowid
                        WHERE frames_fts MATCH ?1
                          AND datetime(f.timestamp) >= datetime('now', ?2)
                        ORDER BY bm25(frames_fts)
                        LIMIT ?3";

    let run = |sql: &str, pattern: &str| -> Result<Vec<SearchHit>, String> {
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![pattern, modifier, limit], |row| {
                Ok(SearchHit {
                    kind: "frame".into(),
                    id: row.get(0)?,
                    ts: row.get(1)?,
                    app: row.get(2)?,
                    title: row.get(3)?,
                    url: row.get(4)?,
                    snippet: row.get(5)?,
                    score: row.get::<_, f64>(6).unwrap_or(0.0),
                })
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.flatten().collect())
    };

    match run(SQL, fts) {
        Ok(hits) => Ok(hits),
        Err(_) => {
            let fallback = format!(
                "SELECT f.id, f.timestamp, COALESCE(f.app_name, 'unknown'),
                        COALESCE(f.window_name, ''), f.browser_url,
                        substr(COALESCE(f.full_text, ''), 1, 400), 0.0
                   FROM frames f
                  WHERE f.full_text LIKE ?1
                    AND datetime(f.timestamp) >= datetime('now', ?2)
                  ORDER BY f.timestamp DESC
                  LIMIT ?3"
            );
            run(&fallback, &format!("%{}%", raw.trim()))
        }
    }
}

fn search_events(
    conn: &Connection,
    fts: &str,
    raw: &str,
    modifier: &str,
    limit: i64,
) -> Result<Vec<SearchHit>, String> {
    const SQL: &str = "SELECT e.id,
                              e.timestamp,
                              COALESCE(e.app_name, 'unknown'),
                              COALESCE(e.window_title, ''),
                              e.browser_url,
                              snippet(ui_events_fts, 0, '«', '»', '…', 20),
                              bm25(ui_events_fts)
                         FROM ui_events_fts
                         JOIN ui_events e ON e.id = ui_events_fts.rowid
                        WHERE ui_events_fts MATCH ?1
                          AND datetime(e.timestamp) >= datetime('now', ?2)
                        ORDER BY bm25(ui_events_fts)
                        LIMIT ?3";

    let run = |sql: &str, pattern: &str| -> Result<Vec<SearchHit>, String> {
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![pattern, modifier, limit], |row| {
                Ok(SearchHit {
                    kind: "event".into(),
                    id: row.get(0)?,
                    ts: row.get(1)?,
                    app: row.get(2)?,
                    title: row.get(3)?,
                    url: row.get(4)?,
                    snippet: row.get(5)?,
                    score: row.get::<_, f64>(6).unwrap_or(0.0),
                })
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.flatten().collect())
    };

    match run(SQL, fts) {
        Ok(hits) => Ok(hits),
        Err(_) => {
            let fallback = format!(
                "SELECT e.id, e.timestamp, COALESCE(e.app_name, 'unknown'),
                        COALESCE(e.window_title, ''), e.browser_url,
                        COALESCE(e.element_name, COALESCE(e.text_content, '')), 0.0
                   FROM ui_events e
                  WHERE (e.element_name LIKE ?1 OR e.text_content LIKE ?1 OR e.window_title LIKE ?1)
                    AND datetime(e.timestamp) >= datetime('now', ?2)
                  ORDER BY e.timestamp DESC
                  LIMIT ?3"
            );
            run(&fallback, &format!("%{}%", raw.trim()))
        }
    }
}

fn search_audio(
    conn: &Connection,
    fts: &str,
    raw: &str,
    modifier: &str,
    limit: i64,
) -> Result<Vec<SearchHit>, String> {
    const SQL: &str = "SELECT a.id,
                              a.timestamp,
                              COALESCE(a.device, 'audio'),
                              '',
                              NULL,
                              snippet(audio_transcriptions_fts, 0, '«', '»', '…', 30),
                              bm25(audio_transcriptions_fts)
                         FROM audio_transcriptions_fts
                         JOIN audio_transcriptions a ON a.id = audio_transcriptions_fts.rowid
                        WHERE audio_transcriptions_fts MATCH ?1
                          AND datetime(a.timestamp) >= datetime('now', ?2)
                        ORDER BY bm25(audio_transcriptions_fts)
                        LIMIT ?3";

    let run = |sql: &str, pattern: &str| -> Result<Vec<SearchHit>, String> {
        let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![pattern, modifier, limit], |row| {
                Ok(SearchHit {
                    kind: "audio".into(),
                    id: row.get(0)?,
                    ts: row.get(1)?,
                    app: row.get(2)?,
                    title: row.get(3)?,
                    url: row.get(4)?,
                    snippet: row.get(5)?,
                    score: row.get::<_, f64>(6).unwrap_or(0.0),
                })
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.flatten().collect())
    };

    match run(SQL, fts) {
        Ok(hits) => Ok(hits),
        Err(_) => {
            let fallback = format!(
                "SELECT a.id, a.timestamp, COALESCE(a.device, 'audio'), '', NULL,
                        COALESCE(a.transcription, ''), 0.0
                   FROM audio_transcriptions a
                  WHERE a.transcription LIKE ?1
                    AND datetime(a.timestamp) >= datetime('now', ?2)
                  ORDER BY a.timestamp DESC
                  LIMIT ?3"
            );
            run(&fallback, &format!("%{}%", raw.trim()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_and_prefixes_tokens() {
        assert_eq!(fts_query("hello world"), Some("\"hello\"* OR \"world\"*".into()));
    }

    #[test]
    fn strips_fts_operators() {
        // Raw `*` and `"` are FTS5 syntax; they are dropped rather than escaped,
        // so the token glues together instead of becoming an operator.
        assert_eq!(fts_query("foo*\"bar"), Some("\"foobar\"*".into()));
        assert_eq!(fts_query("AND OR NOT"), Some("\"AND\"* OR \"OR\"* OR \"NOT\"*".into()));
    }

    #[test]
    fn empty_query_is_none() {
        assert_eq!(fts_query("   "), None);
        assert_eq!(fts_query("***"), None);
    }
}
