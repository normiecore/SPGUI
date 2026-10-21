import type { AppInfo, Settings, Status } from "./types";

/** What every view gets handed by the shell. */
export interface ViewContext {
  /** Active time window, in days. */
  days: number;
  setDays(days: number): void;
  status: Status | null;
  settings: Settings;
  appInfo: AppInfo;
  /** Container the view renders into. */
  view: HTMLElement;
  /** Top-right control strip. */
  actions: HTMLElement;
  setHeader(title: string, sub: string): void;
  toast(message: string, bad?: boolean): void;
  /** Re-run the current view. */
  reload(): void;
}

export interface ViewDef {
  id: string;
  title: string;
  sub: string;
  /** Show the 24h / 7 / 30 day selector in the top-right. */
  showRange?: boolean;
  render(ctx: ViewContext): Promise<void>;
}

export const RANGES = [
  { days: 1, label: "24h" },
  { days: 7, label: "7 days" },
  { days: 30, label: "30 days" },
];
