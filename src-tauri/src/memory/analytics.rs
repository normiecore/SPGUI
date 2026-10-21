//! Aggregate views over the recorded history: what you used, when, and how much.

use std::collections::BTreeMap;

use rusqlite::Connection;
use serde::Serialize;

use super::days_modifier;

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppUsage {
    pub app: String,
    pub frames: i64,
    pub events: i64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DayApp {
    pub app: String,
    pub frames: i64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DayRow {
    pub date: String,
    pub total: i64,
    pub apps: Vec<DayApp>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Analytics {
    pub days: i64,
    pub total_frames: i64,
    pub total_events: i64,
    pub total_transcriptions: i64,
    pub distinct_apps: i64,
    pub first_ts: Option<String>,
    pub last_ts: Option<String>,
    pub top_apps: Vec<AppUsage>,
    pub daily: Vec<DayRow>,
    /// Frames per hour of day (local time), always 24 entries.
    pub hourly: Vec<i64>,
}

fn count(conn: &Connection, sql: &str, modifier: &str) -> i64 {
    conn.query_row(sql, [modifier], |row| row.get::<_, i64>(0))
        .unwrap_or(0)
}

pub fn compute(conn: &Connection, days: i64) -> Result<Analytics, String> {
    let modifier = days_modifier(days);

    let total_frames = count(
        conn,
        "SELECT COUNT(*) FROM frames WHERE datetime(timestamp) >= datetime('now', ?1)",
        &modifier,
    );
    let total_events = count(
        conn,
        "SELECT COUNT(*) FROM ui_events WHERE datetime(timestamp) >= datetime('now', ?1)",
        &modifier,
    );
    let total_transcriptions = count(
        conn,
        "SELECT COUNT(*) FROM audio_transcriptions WHERE datetime(timestamp) >= datetime('now', ?1)",
        &modifier,
    );

    let mut by_app: BTreeMap<String, AppUsage> = BTreeMap::new();

    {
        let mut stmt = conn
            .prepare(
                "SELECT COALESCE(NULLIF(app_name, ''), 'unknown') AS app, COUNT(*)
                   FROM frames
                  WHERE datetime(timestamp) >= datetime('now', ?1)
                  GROUP BY 1
                  ORDER BY 2 DESC
                  LIMIT 40",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([&modifier], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for (app, frames) in rows.flatten() {
            by_app
                .entry(app.clone())
                .or_insert_with(|| AppUsage {
                    app,
                    ..Default::default()
                })
                .frames = frames;
        }
    }

    {
        let mut stmt = conn
            .prepare(
                "SELECT COALESCE(NULLIF(app_name, ''), 'unknown') AS app, COUNT(*)
                   FROM ui_events
                  WHERE datetime(timestamp) >= datetime('now', ?1)
                  GROUP BY 1
                  ORDER BY 2 DESC
                  LIMIT 40",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([&modifier], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for (app, events) in rows.flatten() {
            by_app
                .entry(app.clone())
                .or_insert_with(|| AppUsage {
                    app,
                    ..Default::default()
                })
                .events = events;
        }
    }

    let mut top_apps: Vec<AppUsage> = by_app.values().cloned().collect();
    top_apps.sort_by(|a, b| b.frames.cmp(&a.frames).then(b.events.cmp(&a.events)));
    top_apps.truncate(12);

    let distinct_apps = by_app.len() as i64;

    let first_ts: Option<String> = conn
        .query_row(
            "SELECT MIN(timestamp) FROM frames WHERE datetime(timestamp) >= datetime('now', ?1)",
            [&modifier],
            |row| row.get(0),
        )
        .unwrap_or(None);
    let last_ts: Option<String> = conn
        .query_row("SELECT MAX(timestamp) FROM frames", [], |row| row.get(0))
        .unwrap_or(None);

    // Daily breakdown, limited to the apps that made the top list so the table
    // stays readable.
    let top_names: Vec<String> = top_apps.iter().take(8).map(|a| a.app.clone()).collect();
    let mut daily_map: BTreeMap<String, DayRow> = BTreeMap::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT date(timestamp, 'localtime') AS day,
                        COALESCE(NULLIF(app_name, ''), 'unknown') AS app,
                        COUNT(*)
                   FROM frames
                  WHERE datetime(timestamp) >= datetime('now', ?1)
                  GROUP BY 1, 2
                  ORDER BY 1 DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([&modifier], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;

        for (day, app, frames) in rows.flatten() {
            let entry = daily_map.entry(day.clone()).or_insert(DayRow {
                date: day,
                total: 0,
                apps: Vec::new(),
            });
            entry.total += frames;
            if top_names.contains(&app) {
                entry.apps.push(DayApp { app, frames });
            }
        }
    }
    let mut daily: Vec<DayRow> = daily_map.into_values().collect();
    for row in &mut daily {
        row.apps.sort_by(|a, b| b.frames.cmp(&a.frames));
    }

    let mut hourly = vec![0i64; 24];
    {
        let mut stmt = conn
            .prepare(
                "SELECT CAST(strftime('%H', timestamp, 'localtime') AS INTEGER), COUNT(*)
                   FROM frames
                  WHERE datetime(timestamp) >= datetime('now', ?1)
                  GROUP BY 1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([&modifier], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| e.to_string())?;
        for (hour, frames) in rows.flatten() {
            if (0..24).contains(&hour) {
                hourly[hour as usize] = frames;
            }
        }
    }

    Ok(Analytics {
        days,
        total_frames,
        total_events,
        total_transcriptions,
        distinct_apps,
        first_ts,
        last_ts,
        top_apps,
        daily,
        hourly,
    })
}
