/** Mirrors the serde structs in src-tauri/src (all `camelCase` on the wire). */

export interface Status {
  online: boolean;
  error: string | null;
  version: string | null;
  message: string | null;
  framesCaptured: number;
  framesWritten: number;
  captureState: string | null;
  transcriptionEngine: string | null;
  transcriptions: number;
  lastFrameTs: string | null;
  uptimeSeconds: number;
  monitors: string[];
  storageBytes: number;
  dbBytes: number;
  dbPath: string;
  dataDir: string;
}

export interface TimelineEvent {
  id: number;
  ts: string;
  kind: string;
  label: string;
  detail: string | null;
}

export interface TimelineSession {
  app: string;
  window: string;
  url: string | null;
  startedAt: string;
  endedAt: string;
  durationSeconds: number;
  frameCount: number;
  eventCount: number;
  preview: string;
  thumbnail: string | null;
  events: TimelineEvent[];
}

export interface TimelinePage {
  sessions: TimelineSession[];
  totalSessions: number;
  totalFrames: number;
  totalEvents: number;
}

export interface SearchHit {
  kind: "frame" | "event" | "audio";
  id: number;
  ts: string;
  app: string;
  title: string;
  url: string | null;
  snippet: string;
  score: number;
}

export interface AppUsage {
  app: string;
  frames: number;
  events: number;
}

export interface DayApp {
  app: string;
  frames: number;
}

export interface DayRow {
  date: string;
  total: number;
  apps: DayApp[];
}

export interface Analytics {
  days: number;
  totalFrames: number;
  totalEvents: number;
  totalTranscriptions: number;
  distinctApps: number;
  firstTs: string | null;
  lastTs: string | null;
  topApps: AppUsage[];
  daily: DayRow[];
  hourly: number[];
}

export interface PluginInfo {
  id: string;
  name: string;
  version: string;
  description: string;
  kind: string;
  capabilities: string[];
  dir: string;
  enabled: boolean;
  status: "ready" | "needs-runtime" | "error" | "active" | "planned";
  problem: string | null;
  builtin: boolean;
}

export interface PluginRun {
  id: string;
  ok: boolean;
  exitCode: number | null;
  stdout: string;
  stderr: string;
  durationMs: number;
}

export interface Settings {
  dataDir: string;
  daysDefault: number;
  excludedApps: string[];
  theme: string;
  pluginsEnabled: Record<string, boolean>;
  llmModel: string;
}

export interface AppInfo {
  name: string;
  version: string;
  dataDir: string;
  pluginsDir: string;
  settingsPath: string;
  screenpipeDir: string;
  dbPath: string;
  dbExists: boolean;
}
