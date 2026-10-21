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

export interface FlowEntity {
  kind: string;
  value: string;
}

export interface FlowStep {
  key: string;
  label: string;
  app: string;
  support: number;
  occurrences: number;
  position: number;
  firstSeen: string | null;
  snapshot: string | null;
  sample: string;
  entities: FlowEntity[];
  /** Identifier that arrived here from another app. */
  handoff: string | null;
  /** Successor step key → count. */
  next: [string, number][];
}

export interface FlowRun {
  startedAt: string;
  minutes: number;
  steps: string[];
  matched: boolean;
}

export interface FlowHandoff {
  entity: string;
  from: string;
  to: string;
  count: number;
}

export interface FlowVariant {
  steps: string[];
  cases: number;
  avgMinutes: number;
  standard: boolean;
}

export interface FlowReport {
  days: number;
  gapSeconds: number;
  cases: number;
  activities: number;
  events: number;
  avgSteps: number;
  avgMinutes: number;
  linearity: number;
  /** The standard path, in order, with evidence per step. */
  standard: FlowStep[];
  variants: FlowVariant[];
  runs: FlowRun[];
  handoffs: FlowHandoff[];
  suppressedApps: string[];
}

export interface TableSize {
  name: string;
  bytes: number;
}

export interface ColumnBytes {
  label: string;
  bytes: number;
  rows: number;
}

export interface DayMedia {
  date: string;
  bytes: number;
  files: number;
}

export interface StorageReport {
  dataDir: string;
  dbBytes: number;
  walBytes: number;
  mediaBytes: number;
  totalBytes: number;
  frameFiles: number;
  frameBytes: number;
  audioFiles: number;
  audioBytes: number;
  tables: TableSize[];
  columns: ColumnBytes[];
  mediaByDay: DayMedia[];
  a11yJsonBytes: number;
  a11yFrames: number;
  framesRows: number;
  mediaOver30dBytes: number;
  dbstat: boolean;
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
  autostart: boolean;
  retentionDays: number;
  pluginsEnabled: Record<string, boolean>;
  llmModel: string;
}

export type RecorderPhase = "stopped" | "starting" | "running" | "backoff" | "failed";

export interface RecorderStatus {
  phase: RecorderPhase;
  desired: boolean;
  pid: number | null;
  port: number;
  message: string | null;
  restarts: number;
  retentionDays: number;
  managed: boolean;
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
