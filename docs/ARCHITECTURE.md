# Architecture

Deja is two halves that never share memory: a Rust core that owns all data
access, and a WebView2 frontend that only ever sees JSON.

```
┌─────────────────────────────────────────────────────────────┐
│ WebView2 (TypeScript, src/)                                 │
│   bridge.ts ──invoke()──▶ Tauri commands                     │
│   views/*    ◀──JSON────────┘                               │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────▼───────────────────────────────┐
│ Rust core (src-tauri/src/)                                  │
│   commands.rs   the entire IPC surface (13 commands)        │
│   memory/       read-only Screenpipe SQLite                 │
│   plugins.rs    manifest discovery + child-process runner    │
│   status.rs     /health probe + disk footprint              │
│   settings.rs   JSON settings in app data                   │
└─────────────────────────────────────────────────────────────┘
                              │ read-only
┌─────────────────────────────▼───────────────────────────────┐
│ Screenpipe (external): %USERPROFILE%\.screenpipe\db.sqlite  │
└─────────────────────────────────────────────────────────────┘
```

## Data access rules

1. **Read-only.** `memory::open()` uses
   `SQLITE_OPEN_READ_ONLY | SQLITE_OPEN_NO_MUTEX` and a 3 s busy timeout. Deja
   never issues `INSERT`, `UPDATE`, `DELETE`, `PRAGMA journal_mode`, or schema
   changes against Screenpipe's file.
2. **Short-lived connections.** Every command opens, queries, and drops its
   connection. No connection is held across an await point or a UI session.
3. **Schema tolerance.** Column reads use `COALESCE` and optional types so a
   Screenpipe upgrade that nulls a column degrades the view instead of erroring.
4. **Timestamps** are RFC 3339 with nanosecond precision and a `+00:00` offset.
   Filtering uses `datetime(timestamp) >= datetime('now', '-N days')` rather than
   string comparison, so precision and format changes cannot silently break it.

## Session grouping

`memory/timeline.rs` merges two streams — frames and UI events — into one
time-ordered list, then walks it:

- a new session starts when the app or window changes, or
- when the gap to the previous item exceeds **5 minutes** (`SESSION_GAP_SECONDS`).

Each session carries frame/event counts, a text preview from the last
non-empty frame, the last snapshot path, and up to six event labels.

## Search

`memory/search.rs` searches three FTS5 external-content tables independently and
merges by `bm25` (lower is better):

| table | columns |
| --- | --- |
| `frames_fts` | `full_text`, `app_name`, `window_name`, `browser_url` |
| `ui_events_fts` | `text_content`, `app_name`, `window_title`, `element_name` |
| `audio_transcriptions_fts` | `transcription`, `device`, `speaker_id` |

User input is **never** passed to FTS5 raw. `fts_query()` strips every character
except alphanumerics, `_` and `-`, wraps each token in double quotes, appends
`*` for prefix matching, and OR-joins them. If the MATCH still fails, each source
falls back to `LIKE '%query%'`.

Snippets come from FTS5's `snippet()` with `«»` markers, which the UI converts to
`<mark>` after escaping everything else.

## Plugin contract

```
%APPDATA%\com.normiecore.deja\plugins\
  daily-digest\
    plugin.json
    index.mjs
```

`plugin.json`:

| field | meaning |
| --- | --- |
| `id` | stable identifier; used for enable/disable and run |
| `name`, `version`, `description` | display only |
| `kind` | `digest`, `export`, `capture`, `perception`, `executor`, `view` |
| `capabilities` | advisory strings, surfaced in the UI |
| `defaultEnabled` | initial state when no stored preference exists |
| `runtime.command` | executable to spawn (resolved via PATH) |
| `runtime.args` | arguments |
| `runtime.env` | extra environment variables |

Host behaviour:

- Runs with `cwd` = the plugin folder.
- stdin: a JSON payload. If the caller passes no input, Deja builds
  `{ generatedAt, settings, analytics, status }`.
- stdout/stderr are captured and shown in the UI (truncated to 220 px of scroll).
- 60 s cap; on timeout the whole process tree is killed with
  `taskkill /PID <pid> /T /F`.
- Enabled state persists in `settings.json` under `pluginsEnabled`.

The example plugin seeds itself on first launch from `plugins/daily-digest` in
the repository (embedded with `include_str!`, so it survives packaging).

## Providers (the agent seam)

`plugins::builtins()` lists what the architecture expects to exist:

| provider | kind | status |
| --- | --- | --- |
| `screenpipe` | capture | active — the recorder itself |
| `windows-uia` | perception | planned — foreground window + a11y tree |
| `sendinput` | executor | planned — approval-gated input replay |

They are intentionally visible in the UI as `planned`. Anything resembling an
agent needs a cheap, reliable perception loop and a safe executor; until those
exist, Deja is a viewer and says so.

## Frontend conventions

- No framework, no runtime dependencies (`@tauri-apps/api` is the only import).
- `bridge.ts` is the single IPC boundary. In a plain browser it returns fixtures
  from `mock.ts` so the UI can be developed without the desktop shell.
- All interpolated text goes through `esc()`. `highlighted()` is the only place
  markup is produced from data, and it escapes before inserting `<mark>`.
- Views are `ViewDef` objects (`src/context.ts`) with a `render(ctx)` function;
  the shell owns navigation, the range selector and status polling.

## Failure modes

| situation | behaviour |
| --- | --- |
| Screenpipe not running | status pill goes red; views show a notice, not a stack trace |
| `db.sqlite` missing | every data command returns a message naming the expected path |
| old database locked | 3 s busy timeout, then a readable error |
| malformed plugin manifest | listed with `status: error` and the parse problem |
| plugin hangs | killed after 60 s, error shown in the plugin card |
