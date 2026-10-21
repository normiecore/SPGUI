//! Process mining over your own screen history.
//!
//! Screenpipe ships an `activity_*` ledger (tasks → intervals → actions) for this
//! purpose, but nothing populates it out of the box. So Deja derives the same
//! shape directly from the captures:
//!
//!   * **case**  — a burst of activity with no gap longer than `GAP_SECONDS`
//!   * **step**  — the app that was in front (consecutive repeats collapsed)
//!   * **edge**  — one step directly followed by another
//!
//! From those we report what a process map needs: the directly-follows graph,
//! the standard variant (the most common path) and its exceptions.

use std::collections::BTreeMap;

use chrono::{DateTime, FixedOffset};
use rusqlite::Connection;
use serde::Serialize;

use crate::memory::days_modifier;

/// A pause longer than this starts a new case.
pub const GAP_SECONDS: i64 = 120;
/// Cases longer than this are split, so an all-day session cannot dominate.
const MAX_CASE_SECONDS: i64 = 90 * 60;
/// Continuous capture rarely goes idle for two minutes, so cases are also
/// bounded by length — otherwise five hours of work becomes one giant case and
/// "what typically follows what" has nothing to compare against.
const MAX_CASE_STEPS: usize = 24;
const MAX_ROWS: i64 = 40_000;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FlowStep {
    pub app: String,
    pub occurrences: u64,
    /// How many cases contain this step.
    pub cases: u64,
    /// Mean position in the case, 0-based. Drives the graph layout.
    pub avg_position: f64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FlowEdge {
    pub from: String,
    pub to: String,
    pub count: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FlowVariant {
    pub steps: Vec<String>,
    pub cases: u64,
    pub avg_minutes: f64,
    /// True for the most common path — the "standard process".
    pub standard: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FlowReport {
    pub days: i64,
    pub gap_seconds: i64,
    pub cases: u64,
    pub activities: u64,
    pub events: u64,
    pub avg_steps: f64,
    pub avg_minutes: f64,
    /// Share of cases that follow the standard variant (0..1).
    pub linearity: f64,
    pub steps: Vec<FlowStep>,
    pub edges: Vec<FlowEdge>,
    pub variants: Vec<FlowVariant>,
    pub start_steps: Vec<(String, u64)>,
    pub end_steps: Vec<(String, u64)>,
}

struct Case {
    steps: Vec<String>,
    start: DateTime<FixedOffset>,
    end: DateTime<FixedOffset>,
}

fn parse(raw: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(raw).ok()
}

/// Collapse repeated neighbours and drop empty/unknown labels.
fn normalize(seq: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for app in seq {
        let app = app.trim().to_string();
        if app.is_empty() || app.eq_ignore_ascii_case("unknown") {
            continue;
        }
        if out.last().map(|last| last == &app).unwrap_or(false) {
            continue;
        }
        out.push(app);
    }
    out
}

pub fn compute(conn: &Connection, days: i64, exclude: &[String]) -> Result<FlowReport, String> {
    let modifier = days_modifier(days);
    let skipped: Vec<String> = exclude.iter().map(|a| a.to_ascii_lowercase()).collect();

    let mut stmt = conn
        .prepare(
            "SELECT ts, app FROM (
                 SELECT timestamp AS ts, app_name AS app
                   FROM frames
                  WHERE datetime(timestamp) >= datetime('now', ?1)
                    AND app_name IS NOT NULL AND app_name <> ''
                 UNION ALL
                 SELECT timestamp AS ts, app_name AS app
                   FROM ui_events
                  WHERE datetime(timestamp) >= datetime('now', ?1)
                    AND app_name IS NOT NULL AND app_name <> ''
             )
             ORDER BY ts
             LIMIT ?2",
        )
        .map_err(|e| format!("flow query failed to prepare: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![modifier, MAX_ROWS], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("flow query failed: {e}"))?;

    // ── build cases ─────────────────────────────────────────────────────────
    let mut cases: Vec<Case> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut case_start: Option<DateTime<FixedOffset>> = None;
    let mut last: Option<DateTime<FixedOffset>> = None;
    let mut events = 0u64;
    // Distinct apps seen in the case so far — the cap counts real transitions,
    // not raw events, or a burst in one app would fill a case by itself.
    let mut distinct = 0usize;
    let mut last_raw = String::new();

    for (ts, app) in rows.flatten() {
        let Some(at) = parse(&ts) else { continue };
        let lowered = app.to_ascii_lowercase();
        if skipped.iter().any(|s| !s.is_empty() && s == &lowered) {
            continue;
        }
        events += 1;

        let new_case = match (last, case_start) {
            (Some(prev), Some(start)) => {
                (at - prev).num_seconds() > GAP_SECONDS
                    || (at - start).num_seconds() > MAX_CASE_SECONDS
                    || distinct >= MAX_CASE_STEPS
            }
            _ => true,
        };

        if new_case {
            if let (Some(start), Some(end)) = (case_start, last) {
                if !current.is_empty() {
                    cases.push(Case {
                        steps: normalize(std::mem::take(&mut current)),
                        start,
                        end,
                    });
                }
            }
            case_start = Some(at);
            distinct = 0;
            last_raw.clear();
        }

        if app != last_raw {
            distinct += 1;
            last_raw = app.clone();
        }

        current.push(app);
        last = Some(at);
    }
    if let (Some(start), Some(end)) = (case_start, last) {
        if !current.is_empty() {
            cases.push(Case {
                steps: normalize(current),
                start,
                end,
            });
        }
    }

    // A single-step case carries no flow; keep only real sequences.
    let cases: Vec<Case> = cases.into_iter().filter(|c| c.steps.len() >= 2).collect();

    // ── aggregate ──────────────────────────────────────────────────────────
    let mut step_stats: BTreeMap<String, (u64, u64, f64)> = BTreeMap::new();
    let mut edge_counts: BTreeMap<(String, String), u64> = BTreeMap::new();
    let mut variant_stats: BTreeMap<Vec<String>, (u64, f64)> = BTreeMap::new();
    let mut start_steps: BTreeMap<String, u64> = BTreeMap::new();
    let mut end_steps: BTreeMap<String, u64> = BTreeMap::new();
    let mut total_steps = 0u64;
    let mut total_minutes = 0f64;
    let mut seen_in_case: Vec<String> = Vec::new();

    for case in &cases {
        total_steps += case.steps.len() as u64;
        total_minutes += (case.end - case.start).num_seconds() as f64 / 60.0;
        seen_in_case.clear();

        for (index, app) in case.steps.iter().enumerate() {
            let entry = step_stats.entry(app.clone()).or_insert((0, 0, 0.0));
            entry.0 += 1;
            entry.2 += index as f64;
            if !seen_in_case.iter().any(|seen| seen == app) {
                seen_in_case.push(app.clone());
                entry.1 += 1;
            }
            if index > 0 {
                *edge_counts
                    .entry((case.steps[index - 1].clone(), app.clone()))
                    .or_insert(0) += 1;
            }
        }

        *start_steps.entry(case.steps[0].clone()).or_insert(0) += 1;
        *end_steps.entry(case.steps[case.steps.len() - 1].clone()).or_insert(0) += 1;

        let entry = variant_stats.entry(case.steps.clone()).or_insert((0, 0.0));
        entry.0 += 1;
        entry.1 += (case.end - case.start).num_seconds() as f64 / 60.0;
    }

    let case_count = cases.len() as u64;

    let mut variants: Vec<FlowVariant> = variant_stats
        .into_iter()
        .map(|(steps, (count, minutes))| FlowVariant {
            steps,
            cases: count,
            avg_minutes: if count > 0 { minutes / count as f64 } else { 0.0 },
            standard: false,
        })
        .collect();
    variants.sort_by(|a, b| b.cases.cmp(&a.cases).then(b.steps.len().cmp(&a.steps.len())));
    variants.truncate(8);
    if let Some(first) = variants.first_mut() {
        first.standard = true;
    }

    let linearity = variants
        .first()
        .map(|v| v.cases as f64 / case_count.max(1) as f64)
        .unwrap_or(0.0);

    let mut steps: Vec<FlowStep> = step_stats
        .into_iter()
        .map(|(app, (occurrences, cases, position_sum))| FlowStep {
            avg_position: if occurrences > 0 {
                position_sum / occurrences as f64
            } else {
                0.0
            },
            app,
            occurrences,
            cases,
        })
        .collect();
    steps.sort_by(|a, b| b.cases.cmp(&a.cases).then(b.occurrences.cmp(&a.occurrences)));

    let mut edges: Vec<FlowEdge> = edge_counts
        .into_iter()
        .map(|((from, to), count)| FlowEdge { from, to, count })
        .collect();
    edges.sort_by(|a, b| b.count.cmp(&a.count));

    let mut start_steps: Vec<(String, u64)> = start_steps.into_iter().collect();
    start_steps.sort_by(|a, b| b.1.cmp(&a.1));
    let mut end_steps: Vec<(String, u64)> = end_steps.into_iter().collect();
    end_steps.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(FlowReport {
        days,
        gap_seconds: GAP_SECONDS,
        cases: case_count,
        activities: steps.len() as u64,
        events,
        avg_steps: if case_count > 0 {
            total_steps as f64 / case_count as f64
        } else {
            0.0
        },
        avg_minutes: if case_count > 0 {
            total_minutes / case_count as f64
        } else {
            0.0
        },
        linearity,
        steps,
        edges,
        variants,
        start_steps,
        end_steps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq(apps: &[&str]) -> Vec<String> {
        apps.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn collapses_repeats_and_drops_unknown() {
        assert_eq!(
            normalize(seq(&["a.exe", "a.exe", "unknown", "b.exe", "a.exe"])),
            seq(&["a.exe", "b.exe", "a.exe"])
        );
    }

    #[test]
    fn builds_a_follows_graph_from_cases() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE frames (timestamp TEXT, app_name TEXT);
             CREATE TABLE ui_events (timestamp TEXT, app_name TEXT);",
        )
        .unwrap();

        let now = chrono::Utc::now();
        let at = |secs: i64| (now + chrono::Duration::seconds(secs)).to_rfc3339();

        // Two copies of the standard path a → b → c, plus one exception a → b → a.
        for (i, path) in [["a", "b", "c"], ["a", "b", "c"], ["a", "b", "a"]].iter().enumerate() {
            for (step, app) in path.iter().enumerate() {
                conn.execute(
                    "INSERT INTO frames (timestamp, app_name) VALUES (?1, ?2)",
                    rusqlite::params![
                        at(-3600 + (i as i64 * 600) + step as i64 * 10),
                        format!("{app}.exe")
                    ],
                )
                .unwrap();
            }
        }

        let report = compute(&conn, 7, &[]).unwrap();
        assert_eq!(report.cases, 3, "three bursts, each separated by > 2 min");
        assert_eq!(report.activities, 3);

        let a_to_b = report
            .edges
            .iter()
            .find(|e| e.from == "a.exe" && e.to == "b.exe")
            .map(|e| e.count)
            .unwrap_or(0);
        assert_eq!(a_to_b, 3, "a → b happens in every case");
        assert!(report.linearity > 0.6, "two of three follow the standard path");
        assert_eq!(report.variants[0].steps, seq(&["a.exe", "b.exe", "c.exe"]));
        assert!(report.variants[0].standard);
    }
}
