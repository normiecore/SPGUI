/**
 * Fixtures for `npm run dev` in a plain browser (no Tauri runtime).
 * Everything here is synthetic — the real numbers come from Rust.
 */
import type {
  Analytics,
  AppInfo,
  FlowReport,
  PluginInfo,
  PluginRun,
  SearchHit,
  Settings,
  Status,
  StorageReport,
  TimelineSession,
} from "./types";

const APPS = [
  { app: "brave.exe", windows: ["GitHub · normiecore/SPGUI", "Screenpipe docs", "Hacker News"] },
  { app: "Code.exe", windows: ["deja — src-tauri/src/memory", "design-notes.md"] },
  { app: "Hermes.exe", windows: ["Hermes", "Task: Deja v1"] },
  { app: "explorer.exe", windows: ["Projects", "Downloads"] },
  { app: "OUTLOOK.EXE", windows: ["Inbox — 12 unread", "Re: sprint scope"] },
];

const SNIPPETS = [
  "Deja reads Screenpipe's SQLite database read-only, so a long-running window can never hold a write lock against the recorder.",
  "Frames are captured on a 10 second cadence; dedup skips identical content so the timeline stays readable.",
  "Timeline sessions are grouped by app and window with a five minute gap threshold.",
  "Full-text search runs across three FTS5 indexes: frames, UI events and audio transcriptions.",
  "Plugins are child processes that receive JSON on stdin and print results on stdout.",
];

/** Deterministic PRNG so the fixture UI does not jump between reloads. */
function rng(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state * 1664525 + 1013904223) >>> 0;
    return state / 0xffffffff;
  };
}

export function mockTimeline(days: number): TimelineSession[] {
  const random = rng(42 + days);
  const now = Date.now();
  const sessions: TimelineSession[] = [];

  for (let i = 0; i < 14; i += 1) {
    const pick = APPS[Math.floor(random() * APPS.length)];
    const start = new Date(now - i * (37 + Math.floor(random() * 40)) * 60_000);
    const minutes = 4 + Math.floor(random() * 38);
    const end = new Date(start.getTime() + minutes * 60_000);
    const events = Array.from({ length: 2 + Math.floor(random() * 4) }, (_, k) => ({
      id: i * 100 + k,
      ts: new Date(start.getTime() + k * 40_000).toISOString(),
      kind: k % 3 === 0 ? "app_switch" : "click",
      label: k % 3 === 0 ? "app_switch · brave.exe" : `click · Button "Run ${k + 1}"`,
      detail: null,
    }));

    sessions.push({
      app: pick.app,
      window: pick.windows[Math.floor(random() * pick.windows.length)],
      url: pick.app === "brave.exe" ? "https://github.com/normiecore/SPGUI" : null,
      startedAt: start.toISOString(),
      endedAt: end.toISOString(),
      durationSeconds: minutes * 60,
      frameCount: 6 + Math.floor(random() * 40),
      eventCount: events.length,
      preview: SNIPPETS[Math.floor(random() * SNIPPETS.length)],
      thumbnail: null,
      events,
    });
  }

  return sessions.filter((s) => Date.now() - new Date(s.startedAt).getTime() <= days * 86_400_000);
}

export function mockAnalytics(days: number): Analytics {
  const sessions = mockTimeline(days);
  const byApp = new Map<string, { app: string; frames: number; events: number }>();

  for (const s of sessions) {
    const row = byApp.get(s.app) ?? { app: s.app, frames: 0, events: 0 };
    row.frames += s.frameCount;
    row.events += s.eventCount;
    byApp.set(s.app, row);
  }

  const topApps = [...byApp.values()].sort((a, b) => b.frames - a.frames);
  const random = rng(7 + days);
  const hourly = Array.from({ length: 24 }, (_, h) =>
    h >= 8 && h <= 19 ? Math.floor(random() * 40) : Math.floor(random() * 6),
  );

  const daily = Array.from({ length: Math.min(days, 7) }, (_, d) => {
    const date = new Date(Date.now() - d * 86_400_000).toISOString().slice(0, 10);
    const apps = topApps.slice(0, 4).map((a) => ({
      app: a.app,
      frames: Math.floor(a.frames / Math.min(days, 7)) + Math.floor(random() * 8),
    }));
    return { date, total: apps.reduce((sum, a) => sum + a.frames, 0), apps };
  });

  return {
    days,
    totalFrames: topApps.reduce((sum, a) => sum + a.frames, 0),
    totalEvents: topApps.reduce((sum, a) => sum + a.events, 0),
    totalTranscriptions: 24,
    distinctApps: topApps.length,
    firstTs: sessions.at(-1)?.startedAt ?? null,
    lastTs: sessions[0]?.endedAt ?? null,
    topApps,
    daily,
    hourly,
  };
}

export function mockSearch(query: string, days: number, kinds: string[]): SearchHit[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const wanted = kinds.length ? kinds : ["frame", "event", "audio"];
  const hits: SearchHit[] = [];

  for (const session of mockTimeline(days)) {
    for (const [index, text] of [session.preview, session.window].entries()) {
      if (!text.toLowerCase().includes(q)) continue;
      if (!wanted.includes("frame")) continue;
      hits.push({
        kind: "frame",
        id: index,
        ts: session.startedAt,
        app: session.app,
        title: session.window,
        url: session.url,
        snippet: text.replace(new RegExp(q, "ig"), (m) => `«${m}»`),
        score: 1 + index,
      });
    }
    if (wanted.includes("event")) {
      for (const event of session.events) {
        if (!event.label.toLowerCase().includes(q)) continue;
        hits.push({
          kind: "event",
          id: event.id,
          ts: event.ts,
          app: session.app,
          title: session.window,
          url: null,
          snippet: event.label,
          score: 2,
        });
      }
    }
  }

  if (wanted.includes("audio") && "meeting".includes(q)) {
    hits.push({
      kind: "audio",
      id: 1,
      ts: new Date().toISOString(),
      app: "Microphone (Realtek(R) Audio)",
      title: "",
      url: null,
      snippet: "…so the «meeting» notes go into the digest plugin…",
      score: 3,
    });
  }

  return hits.slice(0, 50);
}

export const mock = {
  appInfo: (): AppInfo => ({
    name: "Deja",
    version: "0.1.0",
    dataDir: "%APPDATA%\\com.normiecore.deja",
    pluginsDir: "%APPDATA%\\com.normiecore.deja\\plugins",
    settingsPath: "%APPDATA%\\com.normiecore.deja\\settings.json",
    screenpipeDir: "%USERPROFILE%\\.screenpipe",
    dbPath: "%USERPROFILE%\\.screenpipe\\db.sqlite",
    dbExists: true,
  }),

  status: (): Status => ({
    online: true,
    error: null,
    version: "0.4.52",
    message: "all systems are functioning normally.",
    framesCaptured: 1184,
    framesWritten: 1184,
    captureState: "recording",
    transcriptionEngine: "Parakeet",
    transcriptions: 42,
    lastFrameTs: new Date().toISOString(),
    uptimeSeconds: 5400,
    monitors: ["Display 65537 (1920x1080)"],
    storageBytes: 1_284_000_000,
    dbBytes: 24_600_000,
    dbPath: "%USERPROFILE%\\.screenpipe\\db.sqlite",
    dataDir: "%USERPROFILE%\\.screenpipe",
  }),

  timelinePage: (days: number, limit: number) => {
    const all = mockTimeline(days);
    return {
      sessions: all.slice(0, limit),
      totalSessions: all.length,
      totalFrames: all.reduce((sum, s) => sum + s.frameCount, 0),
      totalEvents: all.reduce((sum, s) => sum + s.eventCount, 0),
    };
  },

  timeline: (days: number) => mockTimeline(days),
  search: (query: string, days: number, kinds: string[]) => mockSearch(query, days, kinds),
  analytics: (days: number) => mockAnalytics(days),

  flows: (days: number): FlowReport => {
    const steps = [
      { app: "brave.exe", occurrences: 62, cases: 34, avgPosition: 0.2 },
      { app: "Code.exe", occurrences: 55, cases: 30, avgPosition: 1.6 },
      { app: "Hermes.exe", occurrences: 48, cases: 28, avgPosition: 2.4 },
      { app: "OUTLOOK.EXE", occurrences: 22, cases: 14, avgPosition: 3.1 },
    ];
    const edges = [
      { from: "brave.exe", to: "Code.exe", count: 31 },
      { from: "Code.exe", to: "Hermes.exe", count: 27 },
      { from: "Hermes.exe", to: "OUTLOOK.EXE", count: 12 },
      { from: "Hermes.exe", to: "Code.exe", count: 9 },
      { from: "OUTLOOK.EXE", to: "brave.exe", count: 6 },
    ];
    const variants = [
      {
        steps: ["brave.exe", "Code.exe", "Hermes.exe"],
        cases: 21,
        avgMinutes: 18,
        standard: true,
      },
      { steps: ["Code.exe", "Hermes.exe", "OUTLOOK.EXE"], cases: 9, avgMinutes: 12, standard: false },
      { steps: ["brave.exe", "Code.exe"], cases: 5, avgMinutes: 7, standard: false },
    ];
    return {
      days,
      gapSeconds: 120,
      cases: 34,
      activities: 4,
      events: 187,
      avgSteps: 5.4,
      avgMinutes: 16.4,
      linearity: 0.62,
      steps,
      edges,
      variants,
      startSteps: [
        ["brave.exe", 22],
        ["Code.exe", 9],
        ["Hermes.exe", 3],
      ],
      endSteps: [
        ["Hermes.exe", 24],
        ["OUTLOOK.EXE", 7],
        ["Code.exe", 3],
      ],
    };
  },

  storage: (): StorageReport => ({
    dataDir: "%USERPROFILE%\\.screenpipe",
    dbBytes: 68_382_720,
    walBytes: 15_728_640,
    mediaBytes: 56_623_104,
    totalBytes: 140_734_464,
    frameFiles: 61,
    frameBytes: 31_457_280,
    audioFiles: 142,
    audioBytes: 25_165_824,
    tables: [
      { name: "frames", bytes: 41_226_240 },
      { name: "elements", bytes: 14_422_016 },
      { name: "idx_elements_frame_source_role", bytes: 2_187_264 },
      { name: "idx_elements_source_role_text", bytes: 2_174_976 },
      { name: "elements_fts_data", bytes: 831_488 },
    ],
    columns: [
      { label: "frames.accessibility_tree_json", bytes: 34_313_319, rows: 292 },
      { label: "frames.full_text", bytes: 1_542_820, rows: 292 },
      { label: "elements.text", bytes: 6_842_113, rows: 65_069 },
      { label: "frames.text_json", bytes: 2_571_515, rows: 292 },
    ],
    mediaByDay: [
      { date: "2026-10-21", bytes: 56_623_104, files: 203 },
      { date: "2026-10-20", bytes: 12_004_992, files: 81 },
    ],
    a11yJsonBytes: 34_313_319,
    a11yFrames: 292,
    framesRows: 369,
    mediaOver30dBytes: 0,
    dbstat: true,
  }),

  settings: (): Settings => ({
    dataDir: "",
    daysDefault: 7,
    excludedApps: ["deja.exe"],
    theme: "dark",
    pluginsEnabled: {},
    llmModel: "",
  }),

  plugins: (): PluginInfo[] => [
    {
      id: "screenpipe",
      name: "Screenpipe capture",
      version: "0.1.0",
      description:
        "Records screen frames, UI events and audio through the bundled open-source Screenpipe engine.",
      kind: "capture",
      capabilities: ["capture:frames", "capture:ui-events", "capture:audio"],
      dir: "",
      enabled: true,
      status: "active",
      problem: null,
      builtin: true,
    },
    {
      id: "windows-uia",
      name: "Windows UI Automation",
      version: "0.1.0",
      description:
        "Reads the foreground window and accessibility tree via UI Automation. Needed before Deja can decide when to act.",
      kind: "perception",
      capabilities: ["read:foreground", "read:a11y-tree"],
      dir: "",
      enabled: false,
      status: "planned",
      problem: null,
      builtin: true,
    },
    {
      id: "sendinput",
      name: "Windows input executor",
      version: "0.1.0",
      description:
        "Replays clicks and keystrokes with SendInput. Requires the perception provider and explicit user approval.",
      kind: "executor",
      capabilities: ["act:click", "act:keys"],
      dir: "",
      enabled: false,
      status: "planned",
      problem: null,
      builtin: true,
    },
    {
      id: "daily-digest",
      name: "Daily digest",
      version: "0.1.0",
      description:
        "Writes a Markdown summary of the day's activity — top apps, busiest hours, and a sample of recent windows — into <app data>/exports.",
      kind: "digest",
      capabilities: ["read:analytics", "write:exports"],
      dir: "%APPDATA%\\com.normiecore.deja\\plugins\\daily-digest",
      enabled: true,
      status: "ready",
      problem: null,
      builtin: false,
    },
  ],

  togglePlugin: (id: string, enabled: boolean): PluginInfo[] =>
    mock.plugins().map((p) => (p.id === id ? { ...p, enabled } : p)),

  runPlugin: (id: string): PluginRun => ({
    id,
    ok: true,
    exitCode: 0,
    stdout: `Digest written to %APPDATA%\\com.normiecore.deja\\exports\\deja-digest-${new Date()
      .toISOString()
      .slice(0, 10)}.md\n`,
    stderr: "",
    durationMs: 180,
  }),
};
