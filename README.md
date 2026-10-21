# Deja

**Local screen memory for [Screenpipe](https://github.com/mediar-ai/screenpipe) on Windows.**

Deja is a small Rust + WebView2 desktop app that reads the open-source Screenpipe
recorder's SQLite database and gives it a real interface: a timeline of what you
actually did, full-text search over everything you have seen, per-app analytics,
and a plugin system that can turn that history into something useful.

Nothing leaves your machine. Deja has no network client.

> "Deja" — *you've seen this before.*

---

## What it does

| View | What you get |
| --- | --- |
| **Timeline** | Screenpipe's frames and UI events stitched into work sessions — app, window, time range, duration, frame/event counts, a text preview and a screenshot thumbnail. |
| **Search** | One query across three FTS5 indexes: frame OCR text, UI-event labels, and audio transcripts. Window filters (24h / 7d / 30d) and per-source toggles. |
| **Analytics** | Frames / events / transcriptions / apps seen, top apps by volume, hourly distribution, and a daily breakdown table. |
| **Flows** | Process mining over your own history: a directly-follows map of the apps you move between, the **standard path** you repeat, and every variant (exception) with its case count. |
| **Storage** | Measured disk usage — per table, per text column, per day of media — plus what a retention policy would actually reclaim. |
| **Plugins** | Built-in providers (capture, perception, executor) plus user plugins that run as child processes with JSON on stdin. |
| **Settings** | Screenpipe data folder override, default range, excluded apps, theme, and every path Deja touches. |

## Requirements

- **Windows 10/11** (WebView2 runtime — present on Windows 11, installed by the
  installer on Windows 10)
- **Screenpipe** installed and recording. Deja reads
  `%USERPROFILE%\.screenpipe\db.sqlite`; it does not ship its own recorder.
- Rust (stable) and Node 20+ **only if you build from source**.

## Install

Download the `.msi` or `-setup.exe` from [Releases](../../releases), run it, and
start Screenpipe. Deja picks up the database on launch.

## Running it all day

Deja is built to be left alone for an eight-hour workday:

- **It owns the recorder.** On launch it spawns `npx screenpipe record` if the
  API is not already answering, watches `/health`, and restarts with backoff
  (5s → 60s) if it dies. Stop/start/restart are in Settings, and the tray has a
  restart item. Quitting Deja stops the recorder — including one it adopted
  after a crash, which it finds by port rather than by a handle it no longer holds.
- **Closing the window hides it.** The app lives in the tray; quitting is an
  explicit tray action. A second launch focuses the running instance instead of
  fighting it for the WebView2 profile.
- **Start with Windows** is on by default (Settings → Recorder). The login item
  launches with `--hidden`, so it lands in the tray without stealing focus.
- **Media retention** is passed to the recorder (`--retention-days N`). Set it
  once and disk use stops growing without bound; text is kept.
- **Logs**: `%APPDATA%\com.normiecore.deja\deja.log` (rotated at 5 MB) and
  `screenpipe.log` for the recorder's own output.

## Build from source

```bash
git clone https://github.com/normiecore/SPGUI.git
cd SPGUI
npm install
npm run tauri dev          # dev build with hot reload
npm run tauri build        # MSI + NSIS installer in src-tauri/target/release/bundle
```

Frontend-only preview (sample data, no Tauri runtime):

```bash
npm install
npm run dev                # http://localhost:1420
```

### Tests

```bash
npm run test:rust          # query parsing, manifest parsing, snippet logic
npm run build              # tsc type-check + vite build
```

## How it works

```
Screenpipe (Rust, separate process)
  ├── records frames + UI events + audio
  └── writes %USERPROFILE%\.screenpipe\db.sqlite
                    │
                    │  read-only SQLite (rusqlite, WAL, no writes)
                    ▼
Deja core (Rust, Tauri)
  ├── memory/timeline.rs    frames + events → sessions (5 min gap)
  ├── memory/search.rs      FTS5 across frames / ui_events / audio
  ├── memory/analytics.rs   aggregates + hourly distribution
  ├── plugins.rs            manifest discovery + child-process runner
  └── status.rs             Screenpipe /health probe + disk footprint
                    │
                    ▼
WebView2 UI (TypeScript, no framework, no runtime deps)
```

Deja never writes to Screenpipe's database. Every query opens a short-lived
read-only connection, so long-running views cannot hold a lock against the
recorder.

## Plugins

A plugin is a folder in `%APPDATA%\com.normiecore.deja\plugins\<id>\`:

```
plugin.json
index.mjs        # or any executable on PATH
```

```json
{
  "id": "daily-digest",
  "name": "Daily digest",
  "kind": "digest",
  "capabilities": ["read:analytics", "write:exports"],
  "defaultEnabled": true,
  "runtime": { "command": "node", "args": ["index.mjs"] }
}
```

Deja spawns the command, writes a JSON payload to stdin, and shows stdout/stderr
in the Plugins view. Runs are capped at 60 seconds. Environment provided:
`DEJA_DATA_DIR`, `DEJA_PLUGIN_DIR`, `DEJA_FRAMES_DB`.

A reference plugin (`plugins/daily-digest`) is seeded on first run and writes a
Markdown digest into `<app data>\exports`.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the provider seams and the
plugin contract in detail.

## Status and roadmap

Deja v0.1 is the **observation** half: it reads, indexes and presents what
Screenpipe captured.

- [x] Read-only SQLite layer with session grouping
- [x] FTS5 search across frames / events / audio
- [x] Analytics
- [x] Process mining: cases → directly-follows graph → variants
- [x] Storage accounting (dbstat + media walk) with retention estimates
- [x] Plugin runner + reference plugin
- [x] Recorder supervision, tray, start at login, media retention
- [ ] **Perception provider** — Windows UI Automation (foreground window, a11y
      tree) so Deja can tell what is happening *now*, not just what was captured
- [ ] **Executor** — SendInput click/keystroke replay, approval-gated
- [ ] Rules and habits derived from repeated sessions
- [ ] Startup / shutdown control for the Screenpipe process itself
- [ ] Tray icon + background mode

The first two are the gate for anything agent-like: without cheap, reliable
perception and execution there is no agent, only a viewer. They are listed as
`planned` providers in the Plugins view rather than pretending to exist.

## Plug it into Claude (or Cursor, Codex, Cline, Warp)

You don't need Deja for this — Screenpipe ships an MCP server with 35 tools that
reads the same database. Verified working against this install:

```bash
# Claude Code
claude mcp add screenpipe --transport stdio --scope user -- npx -y screenpipe-mcp
```

Claude Desktop: add to `%AppData%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "screenpipe": { "command": "npx", "args": ["-y", "screenpipe-mcp"], "transport": "stdio" }
  }
}
```

**ChatGPT cannot use this as-is**: it only connects to remote HTTPS MCP servers,
never `localhost`. You need a tunnel (OpenAI Secure MCP Tunnel, Cloudflare, ngrok)
in front of an HTTP MCP endpoint.

Deja deliberately ships **no** MCP server of its own — that would duplicate what
already works. See [docs/COMPARISON.md](docs/COMPARISON.md) for the full
comparison with Screenpipe's own app, the storage findings, and the honest gap list.

## Privacy

- No telemetry, no analytics, no cloud sync, no network calls.
- Deja reads a local SQLite file and writes only to its own app-data folder.
- Screen text is untrusted input and is escaped before rendering.
- Retention, audio capture and PII redaction are Screenpipe's settings — configure
  them there.

## License

MIT. See [LICENSE](LICENSE).
