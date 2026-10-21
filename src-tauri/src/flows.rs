//! Process mining over your own screen history — with a semantic step.
//!
//! Screenpipe's own `activity_*` ledger is empty out of the box and its semantic
//! parsers (`screenpipe-semantic`, 49 app targets) are opt-in and off by
//! default. So Deja derives the layer itself, app-agnostically:
//!
//!   * **item**   — one captured frame: app, window, text, snapshot
//!   * **entity** — an identifier found in that text (id, amount, email, url)
//!   * **step**   — consecutive items in the same app+window, collapsed; labelled
//!                  by window title or by the entity that appeared
//!   * **case**   — a burst of steps split at 2 min idle, 90 min, or 24 steps
//!   * **handoff**— the same identifier later appearing in a *different* app
//!
//! The point is to read "Invoice INV-1842 → tracker" instead of "brave.exe →
//! excel.exe". No model is involved; it is string work over what was captured.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, FixedOffset};
use rusqlite::Connection;
use serde::Serialize;

use crate::memory::days_modifier;

pub const GAP_SECONDS: i64 = 120;
const MAX_CASE_SECONDS: i64 = 90 * 60;
const MAX_CASE_STEPS: usize = 24;
const MAX_ROWS: i64 = 12_000;

/// Window chrome and shell surfaces that say nothing about what you were doing.
const NOISE_APPS: &[&str] = &[
    "lockapp.exe",
    "shellhost.exe",
    "pickerhost.exe",
    "applicationframehost.exe",
    "searchhost.exe",
    "startmenuexperiencehost.exe",
    "textinputhost.exe",
    "shellexperiencehost.exe",
    "dwm.exe",
    "sihost.exe",
    "deja.exe",
];

const CHROME: &[&str] = &[
    "minimize",
    "maximize",
    "restore",
    "close",
    "hide sidebar",
    "open settings",
    "layout editor",
    "hud mode",
    "tab search",
    "minimize tabs",
];

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    pub kind: String,
    pub value: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub key: String,
    pub label: String,
    pub app: String,
    pub support: u64,
    pub occurrences: u64,
    pub position: f64,
    pub first_seen: Option<String>,
    pub snapshot: Option<String>,
    pub sample: String,
    pub entities: Vec<Entity>,
    /// Entity that arrived here from another app.
    pub handoff: Option<String>,
    /// Where this step went next, by count — the branch points.
    pub next: Vec<(String, u64)>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub started_at: String,
    pub minutes: f64,
    pub steps: Vec<String>,
    pub matched: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Handoff {
    pub entity: String,
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
    pub linearity: f64,
    pub standard: Vec<Step>,
    pub variants: Vec<FlowVariant>,
    pub runs: Vec<Run>,
    pub handoffs: Vec<Handoff>,
    pub suppressed_apps: Vec<String>,
}

// ── text and entity work ─────────────────────────────────────────────────────

fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len().min(600));
    let mut tokens = 0;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if CHROME.iter().any(|c| lower == *c) {
            continue;
        }
        for word in trimmed.split_whitespace() {
            out.push_str(word);
            out.push(' ');
            tokens += 1;
        }
        if tokens > 60 || out.len() > 420 {
            break;
        }
    }

    out.trim().to_string()
}

fn strip(token: &str) -> &str {
    token.trim_matches(|c: char| {
        matches!(c, ',' | ';' | '|' | '(' | ')' | '[' | ']' | '<' | '>' | '"' | '\'' | '·' | '—')
    })
}

/// Classify one whitespace token as an identifier worth tracking.
fn classify(token: &str) -> Option<Entity> {
    let raw = strip(token);
    if raw.len() < 4 || raw.len() > 80 {
        return None;
    }

    let lower = raw.to_ascii_lowercase();
    if (lower.starts_with("http://") || lower.starts_with("https://")) && raw.len() > 10 {
        return Some(Entity {
            kind: "url".into(),
            value: raw.to_string(),
        });
    }

    if raw.contains('@') && raw.contains('.') && !raw.ends_with('.') {
        return Some(Entity {
            kind: "email".into(),
            value: lower,
        });
    }

    // Currency: $1,234.56 / €900 / £12
    let first = raw.chars().next().unwrap_or(' ');
    if matches!(first, '$' | '€' | '£') {
        let digits: String = raw.chars().skip(1).filter(|c| c.is_ascii_digit()).collect();
        // "$4.1" is OCR dandruff from a truncated number, not an amount.
        if digits.len() < 3 {
            return None;
        }
        let value: String = raw
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
            .collect();
        return Some(Entity {
            kind: "amount".into(),
            value: format!("{first}{value}"),
        });
    }

    // Alphanumeric identifiers: INV-1842, PO-1048, #4471
    let (prefix, suffix) = match raw.split_once('-') {
        Some((a, b)) => (a, b),
        None => return None,
    };

    let prefix_ok = (2..=8).contains(&prefix.len())
        && prefix.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    let suffix_ok = (1..=10).contains(&suffix.len())
        && suffix.chars().all(|c| c.is_ascii_digit());

    if prefix_ok && suffix_ok {
        return Some(Entity {
            kind: "id".into(),
            value: raw.to_string(),
        });
    }

    None
}

fn entities_in(text: &str, limit: usize) -> Vec<Entity> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out = Vec::new();

    for token in text.split_whitespace() {
        let Some(entity) = classify(token) else { continue };
        let dedupe = format!("{}:{}", entity.kind, entity.value);
        if seen.insert(dedupe) {
            out.push(entity);
        }
        if out.len() >= limit {
            break;
        }
    }

    out
}

fn looks_generic(window: &str, app: &str) -> bool {
    let window = window.trim();
    if window.is_empty() {
        return true;
    }
    let app_stem = app.split('.').next().unwrap_or(app);
    window.eq_ignore_ascii_case(app_stem) || window.eq_ignore_ascii_case(app)
}

fn label_for(app: &str, window: &str, text: &str, entities: &[Entity]) -> String {
    if let Some(entity) = entities.first() {
        let base = if looks_generic(window, app) {
            app.split('.').next().unwrap_or(app).to_string()
        } else {
            window.trim().to_string()
        };
        return truncate(&format!("{base} · {}", entity.value), 64);
    }

    if !looks_generic(window, app) {
        return truncate(window.trim(), 64);
    }

    // No window title and no identifier: a 64-character slice of OCR text is not
    // a step name, it is noise with a font. The sample below carries the detail.
    let _ = text;
    app.split('.').next().unwrap_or(app).to_string()
}

fn truncate(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

// ── mining ──────────────────────────────────────────────────────────────────

struct Item {
    at: DateTime<FixedOffset>,
    app: String,
    window: String,
    text: String,
    snapshot: Option<String>,
    entities: Vec<Entity>,
}

struct StepInstance {
    key: String,
    label: String,
    app: String,
    at: DateTime<FixedOffset>,
    snapshot: Option<String>,
    sample: String,
    entities: Vec<Entity>,
}

struct Case {
    steps: Vec<StepInstance>,
    start: DateTime<FixedOffset>,
    end: DateTime<FixedOffset>,
}

/// A step is an app, not an app+window pair.
///
/// Keying on the window title fragmented one working session into 27 "steps"
/// because every browser tab and editor file is a different title. The window
/// still labels the step; it just does not define it.
fn step_key(app: &str) -> String {
    app.trim().to_ascii_lowercase()
}

/// Strip URLs, file paths and capture placeholders before a string is used as a
/// label or a sample — they are long, unstable, and say nothing about the work.
fn scrub_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for token in text.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        if lower.starts_with("http")
            || lower.starts_with("file:")
            || lower.starts_with("c:\\")
            || lower.starts_with("\\\\")
            || lower.contains("url_with_credentials")
            || lower.contains(".exe")
            || lower.contains(".dll")
        {
            continue;
        }
        out.push_str(token);
        out.push(' ');
    }
    out.trim().to_string()
}

pub fn compute(conn: &Connection, days: i64, exclude: &[String]) -> Result<FlowReport, String> {
    let modifier = days_modifier(days);

    let mut skipped: BTreeSet<String> = NOISE_APPS.iter().map(|s| s.to_string()).collect();
    for app in exclude {
        if !app.trim().is_empty() {
            skipped.insert(app.trim().to_ascii_lowercase());
        }
    }

    let mut stmt = conn
        .prepare(
            "SELECT timestamp, COALESCE(app_name, ''), COALESCE(window_name, ''),
                    COALESCE(full_text, ''), snapshot_path
               FROM frames
              WHERE datetime(timestamp) >= datetime('now', ?1)
                AND app_name IS NOT NULL AND app_name <> ''
              ORDER BY timestamp DESC
              LIMIT ?2",
        )
        .map_err(|e| format!("flow query failed to prepare: {e}"))?;

    let rows = stmt
        .query_map(rusqlite::params![modifier, MAX_ROWS], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(|e| format!("flow query failed: {e}"))?;

    let mut items: Vec<Item> = Vec::new();
    let mut suppressed: BTreeSet<String> = BTreeSet::new();

    for (ts, app, window, full_text, snapshot) in rows.flatten() {
        let Ok(at) = DateTime::parse_from_rfc3339(&ts) else { continue };
        let lower = app.to_ascii_lowercase();
        if skipped.contains(&lower) {
            suppressed.insert(app.clone());
            continue;
        }

        let text = scrub_text(&clean(&full_text));
        let entities = entities_in(&text, 4);
        items.push(Item {
            at,
            app,
            window,
            text,
            snapshot,
            entities,
        });
    }

    items.reverse(); // oldest first
    let events = items.len() as u64;

    // ── cases ───────────────────────────────────────────────────────────────
    let mut cases: Vec<Case> = Vec::new();
    let mut current: Vec<StepInstance> = Vec::new();
    let mut case_start: Option<DateTime<FixedOffset>> = None;
    let mut last: Option<DateTime<FixedOffset>> = None;
    let mut last_key = String::new();

    let flush = |steps: &mut Vec<StepInstance>, start: Option<DateTime<FixedOffset>>, end: Option<DateTime<FixedOffset>>, cases: &mut Vec<Case>| {
        if let (Some(start), Some(end)) = (start, end) {
            if steps.len() >= 2 {
                cases.push(Case {
                    steps: std::mem::take(steps),
                    start,
                    end,
                });
                return;
            }
        }
        steps.clear();
    };

    for item in items {
        let key = step_key(&item.app);
        let new_case = match (last, case_start) {
            (Some(prev), Some(start)) => {
                (item.at - prev).num_seconds() > GAP_SECONDS
                    || (item.at - start).num_seconds() > MAX_CASE_SECONDS
                    || current.len() >= MAX_CASE_STEPS
            }
            _ => true,
        };

        if new_case {
            flush(&mut current, case_start, last, &mut cases);
            case_start = Some(item.at);
            last_key.clear();
        }

        if key == last_key {
            // Same step continuing: extend it, keep the freshest evidence.
            if let Some(step) = current.last_mut() {
                step.sample = truncate(&item.text, 200);
                if item.snapshot.is_some() {
                    step.snapshot = item.snapshot.clone();
                }
                for entity in &item.entities {
                    if !step.entities.iter().any(|e| e.value == entity.value) {
                        step.entities.push(entity.clone());
                    }
                }
            }
        } else {
            let label = label_for(&item.app, &item.window, &item.text, &item.entities);
            current.push(StepInstance {
                key: key.clone(),
                label,
                app: item.app.clone(),
                at: item.at,
                snapshot: item.snapshot,
                sample: truncate(&item.text, 200),
                entities: item.entities,
            });
            last_key = key;
        }

        last = Some(item.at);
    }
    flush(&mut current, case_start, last, &mut cases);

    // ── aggregate ───────────────────────────────────────────────────────────
    let mut step_stats: BTreeMap<String, Step> = BTreeMap::new();
    let mut successor: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut variant_stats: BTreeMap<Vec<String>, (u64, f64)> = BTreeMap::new();
    let mut handoff_stats: BTreeMap<(String, String, String), u64> = BTreeMap::new();
    let mut total_steps = 0u64;
    let mut total_minutes = 0f64;
    let mut run_list: Vec<Run> = Vec::new();

    for case in &cases {
        let minutes = (case.end - case.start).num_seconds() as f64 / 60.0;
        total_steps += case.steps.len() as u64;
        total_minutes += minutes;

        let keys: Vec<String> = case.steps.iter().map(|s| s.key.clone()).collect();
        variant_stats.entry(keys.clone()).or_insert((0, 0.0));
        if let Some(entry) = variant_stats.get_mut(&keys) {
            entry.0 += 1;
            entry.1 += minutes;
        }

        for (index, step) in case.steps.iter().enumerate() {
            let record = step_stats.entry(step.key.clone()).or_insert_with(|| Step {
                key: step.key.clone(),
                label: step.label.clone(),
                app: step.app.clone(),
                support: 0,
                occurrences: 0,
                position: 0.0,
                first_seen: Some(step.at.to_rfc3339()),
                snapshot: step.snapshot.clone(),
                sample: step.sample.clone(),
                entities: Vec::new(),
                handoff: None,
                next: Vec::new(),
            });

            record.support += 1;
            record.occurrences += 1;
            record.position += index as f64;

            for entity in &step.entities {
                if !record.entities.iter().any(|e| e.value == entity.value) {
                    if record.entities.len() < 3 {
                        record.entities.push(entity.clone());
                    }
                }
            }

            if index + 1 < case.steps.len() {
                *successor
                    .entry(step.key.clone())
                    .or_default()
                    .entry(case.steps[index + 1].key.clone())
                    .or_insert(0) += 1;
            }
        }

        // Handoffs: an identifier introduced here reappearing in another app.
        let mut first_app: BTreeMap<String, &str> = BTreeMap::new();
        for step in &case.steps {
            for entity in &step.entities {
                match first_app.get(&entity.value) {
                    None => {
                        first_app.insert(entity.value.clone(), step.app.as_str());
                    }
                    Some(from) if *from != step.app.as_str() => {
                        let key = (
                            entity.value.clone(),
                            from.to_string(),
                            step.app.clone(),
                        );
                        *handoff_stats.entry(key).or_insert(0) += 1;
                    }
                    _ => {}
                }
            }
        }

        run_list.push(Run {
            started_at: case.start.to_rfc3339(),
            minutes,
            steps: keys,
            matched: false,
        });
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

    // Standard path: the most common variant's steps, in order.
    let standard_keys: Vec<String> = variants
        .first()
        .map(|v| v.steps.clone())
        .unwrap_or_default();

    for run in &mut run_list {
        run.matched = run.steps == standard_keys;
    }

    let mut standard: Vec<Step> = Vec::new();
    for key in &standard_keys {
        if let Some(mut step) = step_stats.get(key).cloned() {
            step.next = successor
                .get(key)
                .map(|map| {
                    let mut list: Vec<(String, u64)> =
                        map.iter().map(|(k, v)| (k.clone(), *v)).collect();
                    list.sort_by(|a, b| b.1.cmp(&a.1));
                    list
                })
                .unwrap_or_default();
            standard.push(step);
        }
    }

    // An identifier that shows up in a third of all steps is context (a project
    // name, a chat subject), not a handoff — "INV-1842" was appearing because it
    // was in the conversation text on screen, not because invoice work happened.
    let step_total = step_stats.len().max(1);
    let mut spread: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for step in step_stats.values() {
        for entity in &step.entities {
            spread
                .entry(entity.value.clone())
                .or_default()
                .insert(step.key.clone());
        }
    }
    let ubiquitous: BTreeSet<String> = spread
        .into_iter()
        .filter(|(_, keys)| keys.len() * 4 > step_total)
        .map(|(value, _)| value)
        .collect();

    let handoffs: Vec<Handoff> = {
        let mut list: Vec<Handoff> = handoff_stats
            .into_iter()
            .filter(|((entity, _, _), count)| *count >= 2 && !ubiquitous.contains(entity))
            .map(|((entity, from, to), count)| Handoff {
                entity,
                from,
                to,
                count,
            })
            .collect();
        list.sort_by(|a, b| b.count.cmp(&a.count));
        list.truncate(12);
        list
    };

    // Only identifiers that demonstrably moved between apps survive on a step.
    // Anything else is screen text that happened to look like an id — our own
    // conversation about "INV-1842" was showing up as a handoff.
    let moved: BTreeSet<&str> = handoffs.iter().map(|h| h.entity.as_str()).collect();

    for step in &mut standard {
        step.entities.retain(|entity| {
            !ubiquitous.contains(&entity.value) && moved.contains(entity.value.as_str())
        });
        if let Some(handoff) = handoffs.iter().find(|h| h.to.eq_ignore_ascii_case(&step.app)) {
            step.handoff = Some(handoff.entity.clone());
        }
    }

    let activities = step_stats.len() as u64;

    Ok(FlowReport {
        days,
        gap_seconds: GAP_SECONDS,
        cases: case_count,
        activities,
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
        standard,
        variants,
        runs: run_list,
        handoffs,
        suppressed_apps: suppressed.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_identifiers() {
        assert_eq!(classify("INV-1842").map(|e| e.kind), Some("id".into()));
        assert_eq!(classify("PO-1048,").map(|e| e.value), Some("PO-1048".into()));
        assert_eq!(classify("$4,900.00").map(|e| e.kind), Some("amount".into()));
        assert_eq!(
            classify("alder@packaging.com").map(|e| e.kind),
            Some("email".into())
        );
        assert_eq!(classify("https://x.test/a").map(|e| e.kind), Some("url".into()));
        assert!(classify("Hello").is_none());
        assert!(classify("2026-10-21").is_none(), "dates are not identifiers");
    }

    #[test]
    fn strips_window_chrome() {
        let text = "Minimize\nMaximize\nClose\nInspector: invoice INV-1842";
        let cleaned = clean(text);
        assert!(!cleaned.to_ascii_lowercase().contains("maximize"));
        assert!(cleaned.contains("INV-1842"));
    }

    #[test]
    fn labels_prefer_the_entity() {
        let entities = entities_in("Invoice INV-1842 from Alder Packaging", 4);
        let label = label_for("OUTLOOK.EXE", "Inbox", "Invoice INV-1842", &entities);
        assert!(label.contains("INV-1842"), "got {label}");
    }

    #[test]
    fn generic_window_falls_back_to_the_app_not_ocr() {
        // A window titled the same as its app says nothing, and a slice of OCR is
        // not a step name — fall back to the app.
        let label = label_for("Hermes.exe", "Hermes", "deja flows redraw", &[]);
        assert_eq!(label, "Hermes");
    }

    #[test]
    fn scrubs_paths_and_urls() {
        let scrubbed = scrub_text("open https://x.test/a C:\\Users\\jagua\\x.exe now");
        assert_eq!(scrubbed, "open now");
    }

    #[test]
    fn handoffs_need_a_different_app() {
        // Same app twice is not a handoff; a new app carrying the identifier is.
        let mut map: BTreeMap<String, &str> = BTreeMap::new();
        map.insert("INV-1".into(), "a.exe");
        assert_eq!(map.get("INV-1"), Some(&"a.exe"));
        assert_ne!(map.get("INV-1"), Some(&"b.exe"));
    }
}
