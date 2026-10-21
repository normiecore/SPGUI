# Deja vs Screenpipe's own app (Studio)

Written after reading screenpipe's docs, its database schema, and its MCP server
on a live install — not from screenshots. Numbers are measured on this machine
(Windows 11, Screenpipe 0.4.52, ~5 h of capture).

## First, the honest framing

Screenpipe ships **two** desktop things, which is easy to conflate:

| Thing | Licence | What it is |
| --- | --- | --- |
| `screenpipe` CLI | MIT | Recorder + REST API on `:3030`. Fully free. |
| Desktop app, built from source | MIT | Tauri + Next.js: timeline, chat, scheduled tasks, connections, settings. Free if you build it. |
| Commercial desktop app (`screenpi.pe`) | Paid, 7-day trial | Same app, prebuilt, auto-updates, support, team sync, enterprise auth. |

**Studio** is the workflow-discovery surface advertised on screenpipe.com: it
shows recurring work across apps ("Weekly operations report — ERP → Spreadsheet
→ Email — 3 example runs · 1 exception") and turns that into SOPs and agents.

So Deja is not "the free version of a paid app" — the app is MIT either way.
Deja's reason to exist is different: **read-only, tiny, no LLM required, and
explicit about what it cannot do.**

## Feature comparison

| Capability | Screenpipe app | Deja v0.2 |
| --- | --- | --- |
| Visual timeline | ✅ DVR-style scrub with screenshots | ✅ session cards, previews, last-frame thumbnail |
| Full-size screenshot viewer | ✅ | ❌ (inline thumbnail only) |
| Keyword search | ✅ FTS5 | ✅ FTS5 across frames / events / audio |
| Natural-language / chat over history | ✅ | ❌ (no LLM in the loop) |
| Process map / workflow discovery | ✅ Studio "Discover" | ✅ **Flows** — directly-follows graph, variants, standard path |
| Storage accounting | ❌ | ✅ **Storage** — per-table, per-column, per-day, with the a11y-tree finding |
| Analytics (apps, hours, days) | partial | ✅ |
| Scheduled tasks / automations | ✅ pipes (`~/.screenpipe/pipes`) | ❌ plugins are manual-run only |
| Integrations (Slack, Notion, Obsidian…) | ✅ dozens | ❌ (plugin contract could reach them) |
| Meeting detection, speakers, summaries | ✅ | ❌ (transcripts are searchable, nothing more) |
| Tags / favourites | ✅ | ❌ |
| Audio playback | ✅ | ❌ |
| Video export of a time range | ✅ | ❌ |
| MCP server for AI clients | ✅ 35 tools | ❌ — and doesn't need one (see below) |
| Tray icon, autostart | ✅ | ✅ v0.3 (hide-on-close, `--hidden` login start) |
| Recorder supervision (start, health watch, restart with backoff) | ✅ | ✅ v0.3 |
| Permission onboarding | ✅ guided | n/a on Windows (no screen-recording permission) |
| Network client | optional cloud paths | none at all |
| Install size | ~100 MB+ (Next.js + WebView2 app) | **7.6 MB binary**, 2.7 MB installer |

## What Deja does that the app does not

**1. Storage accounting.** Nothing in either product tells you where the bytes
went. Deja measures it (see below). This is the only view in Deja that needed a
new SQLite capability (`dbstat`) and a filesystem walk.

**2. A process map with no model in the loop.** Studio's discovery is AI-driven.
Deja's `Flows` view is plain event-log arithmetic: cases → directly-follows
edges → variants → the standard path and its exceptions. It runs instantly, costs
nothing, and is reproducible. It is also weaker: it sees *apps*, not *actions*.

**3. Read-only by construction.** One `SQLITE_OPEN_READ_ONLY` connection per
query, no writes, no network client. You can run it against a database you don't
own.

## Storage: where the space actually goes

Measured on a 5-hour capture:

| Component | Size |
| --- | --- |
| `db.sqlite` | 77 MB |
| `db.sqlite-wal` | 14 MB |
| media (`data/`: 80 JPEG frames + 159 audio clips) | 60 MB |
| **total** | **151 MB** |

Inside the database, `dbstat` gives:

| Table | Bytes |
| --- | --- |
| `frames` | 46 MB |
| `elements` | 17 MB |
| `elements` indexes (8 of them) | ~9 MB |

And inside `frames`, per column:

| Column | Bytes | Rows |
| --- | --- | --- |
| `accessibility_tree_json` | **39 MB** | 380 |
| `text_json` | 2.6 MB | 380 |
| `full_text` | 1.6 MB | 380 |
| `elements.text` | 7 MB | 65 k |

**The finding: 87% of the database's text is `frames.accessibility_tree_json`,
at ~102 KB per frame, and it is duplicated** — the same structure is flattened
into `elements` (which has its own FTS index) and into `full_text` (which search
and the timeline read). Deja never touches that column.

That makes it the single biggest lever you have:

- `--retention-days N` on the recorder drops media older than N days and keeps text.
  Screenpipe's own log line says `local retention disabled (--retention-days 0)`
  by default, so nothing is being pruned today.
- Nulling `accessibility_tree_json` on old frames would cut the DB by roughly 80%
  for those rows. Deja **does not** do that — it is read-only by design. It
  reports the number so you can decide.

### Overhead Deja removed from itself

- The status probe used to walk the whole `data/` tree (50 k+ `stat` calls) every
  **15 seconds**. It now reads file metadata only; the walk happens once, on demand,
  in the Storage view.
- The timeline pulled `full_text` (avg 4.2 KB, up to megabytes) for every frame in
  the window to show a two-line preview. It now `SUBSTR(..., 1, 400)` in SQL.

## Plugging into Claude / ChatGPT — verified, not assumed

Screenpipe already ships an MCP server. I ran it here and completed a real
handshake plus a live query:

```
INIT: {"name":"screenpipe","version":"0.20.3"}
TOOLS: list-workflows, get-workflow, screenpipe-skills, search-content,
       activity-summary, search-elements, frame-context, export-video,
       send-notification, health-check, control-recording, list-pipes,
       create-pipe, run-pipe, … (35 total)
CALL search-content("deja") → "Results: 2/74"
```

| Client | Works? | How |
| --- | --- | --- |
| **Claude Code** | ✅ today | `claude mcp add screenpipe --transport stdio -- npx -y screenpipe-mcp` |
| **Claude Desktop** | ✅ today | `%AppData%\Claude\claude_desktop_config.json` → `mcpServers.screenpipe` |
| Cursor / Codex / Cline / Warp / Msty | ✅ today | same stdio server |
| **ChatGPT** | ⚠️ only remotely | ChatGPT cannot reach `localhost`. Needs a public HTTPS MCP endpoint (Streamable HTTP/SSE) or OpenAI's Secure MCP Tunnel. Developer Mode; Plus/Pro read-only, Business/Enterprise for write. |

**Consequence for Deja: do not build an MCP server.** It would duplicate 35
working tools that already read the same database. Claude already has your
memory; Deja's job is the interface, not another endpoint.

## Gaps worth closing, ranked

1. **Chat over history** — the single biggest missing feature. Needs an LLM key
   and a retrieval path (start with FTS5 + the FTS `snippet()` already in place).
2. **Screenshot viewer** — click a session, see the full frame, arrow through
   neighbours. Small, high value; the asset protocol is already enabled.
3. **Scheduled plugins** — the runner exists; it needs a cron field in the
   manifest and the `node-cron` equivalent in Rust.
4. **Action-level flows** — mine `ui_events` element names, not just app names,
   so a flow reads "click Save → switch to Brave → paste" instead of "Code → Brave".
   This is also the bridge to SOPs.
5. **Meeting view** — speakers and meeting rows already exist in the schema.

*(Tray, autostart, recorder supervision and retention shipped in v0.3 — they were
the difference between a demo and something you can reboot into.)*

## The narrow claim Deja can defend

> Open-source Screenpipe is a recorder with a great API and a paid app on top.
> Deja is a 7.6 MB read-only lens on that database that tells you the truth about
> your storage, shows the process you actually repeat, and never phones home.

Anything more ambitious — chat, agents, SOPs — is either already in Screenpipe's
app or needs the perception provider first. Say so rather than shipping a
half-agent.
