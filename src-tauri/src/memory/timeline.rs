//! Timeline: turn Screenpipe's frame + UI-event streams into readable work sessions.
//!
//! Screenpipe stores one row per captured frame and one row per input event.
//! A "session" here is a run of activity in the same app/window with no gap
//! longer than [`SESSION_GAP_SECONDS`].

use chrono::{DateTime, FixedOffset};
use rusqlite::Connection;
use serde::Serialize;

use super::{days_modifier, snippet};

const SESSION_GAP_SECONDS: i64 = 300;
const MAX_EVENTS_PER_SESSION: usize = 6;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEvent {
    pub id: i64,
    pub ts: String,
    pub kind: String,
    pub label: String,
    pub detail: Option<String>,
}

/// A page of sessions plus the honest totals for the whole window, so the UI
/// never implies "95 frames" when the range actually holds 270.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TimelinePage {
    pub sessions: Vec<TimelineSession>,
    pub total_sessions: usize,
    pub total_frames: i64,
    pub total_events: i64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TimelineSession {
    pub app: String,
    pub window: String,
    pub url: Option<String>,
    pub started_at: String,
    pub ended_at: String,
    pub duration_seconds: i64,
    pub frame_count: i64,
    pub event_count: i64,
    pub preview: String,
    pub thumbnail: Option<String>,
    pub events: Vec<TimelineEvent>,
}

enum Item {
    Frame {
        at: DateTime<FixedOffset>,
        app: String,
        window: String,
        url: Option<String>,
        text: String,
        snapshot: Option<String>,
    },
    Event {
        at: DateTime<FixedOffset>,
        app: String,
        window: String,
        event: TimelineEvent,
    },
}

impl Item {
    fn at(&self) -> DateTime<FixedOffset> {
        match self {
            Item::Frame { at, .. } | Item::Event { at, .. } => *at,
        }
    }
    fn app(&self) -> String {
        match self {
            Item::Frame { app, .. } | Item::Event { app, .. } => app.clone(),
        }
    }
    fn window(&self) -> String {
        match self {
            Item::Frame { window, .. } | Item::Event { window, .. } => window.clone(),
        }
    }
}

fn parse_ts(raw: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(raw).ok()
}

/// Return a page of sessions for the last `days`, newest first.
///
/// `total_sessions` / `total_frames` describe the whole window, not the page.
pub fn page(conn: &Connection, days: i64, limit: i64) -> Result<TimelinePage, String> {
    let modifier = days_modifier(days);
    let total_frames: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM frames WHERE datetime(timestamp) >= datetime('now', ?1)",
            [&modifier],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let total_events: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM ui_events WHERE datetime(timestamp) >= datetime('now', ?1)",
            [&modifier],
            |row| row.get(0),
        )
        .unwrap_or(0);

    let all = sessions(conn, days, 5_000)?;
    let total_sessions = all.len();
    let mut sessions = all;
    sessions.truncate(limit.clamp(1, 500) as usize);

    Ok(TimelinePage {
        sessions,
        total_sessions,
        total_frames,
        total_events,
    })
}

/// Build every session in the window, newest first.
pub fn sessions(conn: &Connection, days: i64, limit: i64) -> Result<Vec<TimelineSession>, String> {
    let modifier = days_modifier(days);
    let frame_limit = (limit.clamp(1, 5_000)) * 12;

    let mut items: Vec<Item> = Vec::new();

    let mut stmt = conn
        .prepare(
            "SELECT id, timestamp, COALESCE(app_name, 'unknown'), COALESCE(window_name, ''),
                    browser_url, COALESCE(full_text, ''), snapshot_path
               FROM frames
              WHERE datetime(timestamp) >= datetime('now', ?1)
              ORDER BY timestamp DESC
              LIMIT ?2",
        )
        .map_err(|e| format!("timeline frames query failed to prepare: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![modifier, frame_limit], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })
        .map_err(|e| format!("timeline frames query failed: {e}"))?;

    for row in rows.flatten() {
        let (_id, ts, app, window, url, text, snapshot) = row;
        if let Some(at) = parse_ts(&ts) {
            items.push(Item::Frame {
                at,
                app,
                window,
                url,
                text,
                snapshot,
            });
        }
    }

    let mut stmt = conn
        .prepare(
            "SELECT id, timestamp, COALESCE(app_name, 'unknown'), COALESCE(window_title, ''),
                    event_type, element_role, element_name, text_content, browser_url
               FROM ui_events
              WHERE datetime(timestamp) >= datetime('now', ?1)
              ORDER BY timestamp DESC
              LIMIT ?2",
        )
        .map_err(|e| format!("timeline events query failed to prepare: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![modifier, frame_limit], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
            ))
        })
        .map_err(|e| format!("timeline events query failed: {e}"))?;

    for row in rows.flatten() {
        let (id, ts, app, window, event_type, role, name, text, url) = row;
        let Some(at) = parse_ts(&ts) else { continue };
        let label = match (role.as_deref(), name.as_deref()) {
            (Some(role), Some(name)) if !name.is_empty() => format!("{event_type} · {role} \"{name}\""),
            (Some(role), _) => format!("{event_type} · {role}"),
            _ => event_type.clone(),
        };
        items.push(Item::Event {
            at,
            app,
            window,
            event: TimelineEvent {
                id,
                ts,
                kind: event_type,
                label,
                detail: text.or(url).filter(|s| !s.is_empty()),
            },
        });
    }

    items.sort_by_key(|i| i.at());

    let mut sessions: Vec<TimelineSession> = Vec::new();
    let mut current: Option<(String, String, DateTime<FixedOffset>, DateTime<FixedOffset>)> = None;
    let mut frames_in = 0i64;
    let mut events_in = 0i64;
    let mut preview = String::new();
    let mut thumbnail: Option<String> = None;
    let mut url: Option<String> = None;
    let mut events: Vec<TimelineEvent> = Vec::new();

    let flush = |sessions: &mut Vec<TimelineSession>,
                     current: &mut Option<(
        String,
        String,
        DateTime<FixedOffset>,
        DateTime<FixedOffset>,
    )>,
                     frames_in: &mut i64,
                     events_in: &mut i64,
                     preview: &mut String,
                     thumbnail: &mut Option<String>,
                     url: &mut Option<String>,
                     events: &mut Vec<TimelineEvent>| {
        if let Some((app, window, started, ended)) = current.take() {
            sessions.push(TimelineSession {
                app,
                window,
                url: url.take(),
                started_at: started.to_rfc3339(),
                ended_at: ended.to_rfc3339(),
                duration_seconds: (ended - started).num_seconds().max(0),
                frame_count: *frames_in,
                event_count: *events_in,
                preview: std::mem::take(preview),
                thumbnail: thumbnail.take(),
                events: std::mem::take(events),
            });
        }
        *frames_in = 0;
        *events_in = 0;
    };

    for item in items {
        let app = item.app();
        let window = item.window();
        let at = item.at();

        // Screenpipe records UI events without a window title more often than
        // frames do, so an empty title must never split a session — otherwise a
        // steady stream of clicks fragments into one "0 frames" session each.
        let continues = current
            .as_ref()
            .map(|(cur_app, cur_window, _, last)| {
                cur_app == &app
                    && (at - *last).num_seconds() <= SESSION_GAP_SECONDS
                    && (window.is_empty() || cur_window.is_empty() || cur_window == &window)
            })
            .unwrap_or(false);

        if !continues {
            flush(
                &mut sessions,
                &mut current,
                &mut frames_in,
                &mut events_in,
                &mut preview,
                &mut thumbnail,
                &mut url,
                &mut events,
            );
            current = Some((app.clone(), window.clone(), at, at));
        } else if let Some((_, cur_window, _, last)) = current.as_mut() {
            *last = at;
            if cur_window.is_empty() && !window.is_empty() {
                *cur_window = window;
            }
        }

        match item {
            Item::Frame {
                text, snapshot, url: frame_url, ..
            } => {
                frames_in += 1;
                if !text.trim().is_empty() {
                    preview = snippet(&text, 240);
                }
                if snapshot.is_some() {
                    thumbnail = snapshot;
                }
                if frame_url.is_some() {
                    url = frame_url;
                }
            }
            Item::Event { event, .. } => {
                events_in += 1;
                if events.len() < MAX_EVENTS_PER_SESSION {
                    events.push(event);
                }
            }
        }
    }

    flush(
        &mut sessions,
        &mut current,
        &mut frames_in,
        &mut events_in,
        &mut preview,
        &mut thumbnail,
        &mut url,
        &mut events,
    );

    sessions.reverse();
    sessions.truncate(limit.clamp(1, 5_000) as usize);
    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_screenpipe_timestamps() {
        let ts = "2026-10-21T04:03:13.457707300+00:00";
        assert!(parse_ts(ts).is_some(), "9-digit nanos must parse");
    }

    /// A click with no window title must join the surrounding session instead of
    /// creating a "0 frames · 1 event" session of its own.
    #[test]
    fn untitled_events_join_the_current_session() {
        use rusqlite::Connection;

        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE frames (
                id INTEGER PRIMARY KEY, timestamp TEXT, app_name TEXT, window_name TEXT,
                browser_url TEXT, full_text TEXT, snapshot_path TEXT);
             CREATE TABLE ui_events (
                id INTEGER PRIMARY KEY, timestamp TEXT, app_name TEXT, window_title TEXT,
                event_type TEXT, element_role TEXT, element_name TEXT, text_content TEXT,
                browser_url TEXT);",
        )
        .unwrap();

        let now = chrono::Utc::now();
        let at = |secs: i64| (now + chrono::Duration::seconds(secs)).to_rfc3339();

        conn.execute(
            "INSERT INTO frames (timestamp, app_name, window_name, full_text) VALUES (?1, 'code.exe', 'main.rs', 'fn main()')",
            [at(-60)],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ui_events (timestamp, app_name, window_title, event_type, element_role, element_name)
             VALUES (?1, 'code.exe', '', 'click', 'Button', 'Save')",
            [at(-30)],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO frames (timestamp, app_name, window_name, full_text) VALUES (?1, 'code.exe', 'main.rs', 'fn other()')",
            [at(-10)],
        )
        .unwrap();

        let sessions = sessions(&conn, 1, 10).unwrap();
        assert_eq!(sessions.len(), 1, "expected one merged session, got {sessions:#?}");
        assert_eq!(sessions[0].frame_count, 2);
        assert_eq!(sessions[0].event_count, 1);
        assert_eq!(sessions[0].window, "main.rs");
    }
}
