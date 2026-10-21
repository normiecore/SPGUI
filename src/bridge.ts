/**
 * The only place the UI talks to Rust.
 *
 * In `npm run dev` (a plain browser, no Tauri runtime) every call falls back to
 * deterministic fixtures so the interface can be developed and reviewed without
 * launching the desktop shell. Anything you see there is sample data.
 */
import { convertFileSrc, invoke } from "@tauri-apps/api/core";

import { mock } from "./mock";
import type {
  Analytics,
  AppInfo,
  FlowReport,
  PluginInfo,
  PluginRun,
  RecorderStatus,
  SearchHit,
  Settings,
  Status,
  StorageReport,
  TimelinePage,
} from "./types";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const call = async <T>(command: string, args?: Record<string, unknown>, fallback?: () => T): Promise<T> => {
  if (!isTauri) {
    if (!fallback) throw new Error(`No browser fallback for ${command}`);
    return fallback();
  }
  return invoke<T>(command, args);
};

export const api = {
  appInfo: () => call<AppInfo>("app_info", {}, () => mock.appInfo()),

  status: (port = 3030) => call<Status>("get_status", { port }, () => mock.status()),

  timeline: (days: number, limit = 80) =>
    call<TimelinePage>("get_timeline", { days, limit }, () => mock.timelinePage(days, limit)),

  search: (query: string, days: number, kinds: string[], limit = 50) =>
    call<SearchHit[]>("search_memory", { query, days, kinds, limit }, () => mock.search(query, days, kinds)),

  analytics: (days: number) => call<Analytics>("get_analytics", { days }, () => mock.analytics(days)),

  flows: (days: number) => call<FlowReport>("get_flows", { days }, () => mock.flows(days)),

  storage: () => call<StorageReport>("get_storage", {}, () => mock.storage()),

  recorderStatus: () => call<RecorderStatus>("recorder_status", {}, () => mock.recorder()),
  recorderStart: () => call<RecorderStatus>("recorder_start", {}, () => mock.recorder()),
  recorderStop: () =>
    call<RecorderStatus>("recorder_stop", {}, () => ({ ...mock.recorder(), phase: "stopped" as const, desired: false })),
  recorderRestart: () => call<RecorderStatus>("recorder_restart", {}, () => mock.recorder()),

  setRetention: (days: number) =>
    call<RecorderStatus>("set_retention_days", { days }, () => ({ ...mock.recorder(), retentionDays: days })),

  setAutostart: (enabled: boolean) => call<boolean>("set_autostart", { enabled }, () => enabled),

  getSettings: () => call<Settings>("get_settings", {}, () => mock.settings()),

  saveSettings: (settings: Settings) =>
    call<Settings>("save_settings", { settings }, () => settings),

  plugins: () => call<PluginInfo[]>("list_plugins", {}, () => mock.plugins()),

  refreshPlugins: () => call<PluginInfo[]>("refresh_plugins", {}, () => mock.plugins()),

  setPluginEnabled: (id: string, enabled: boolean) =>
    call<PluginInfo[]>("set_plugin_enabled", { id, enabled }, () => mock.togglePlugin(id, enabled)),

  runPlugin: (id: string) =>
    call<PluginRun>("run_plugin", { id }, () => mock.runPlugin(id)),

  seedExamplePlugin: () => call<string>("seed_example_plugin", {}, () => "plugins/daily-digest"),

  openPath: (path: string) => call<void>("open_path", { path }, () => undefined),
};

/** Screenpipe stores JPEG snapshots on disk; the asset protocol serves them. */
export function thumbnailUrl(path: string | null): string | null {
  if (!path) return null;
  if (!isTauri) return null;
  try {
    return convertFileSrc(path);
  } catch {
    return null;
  }
}
