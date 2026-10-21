/** Small DOM/format helpers shared by every view. */

export const $ = <T extends Element = HTMLElement>(selector: string, root: ParentNode = document): T | null =>
  root.querySelector<T>(selector);

export const $$ = <T extends Element = HTMLElement>(selector: string, root: ParentNode = document): T[] =>
  [...root.querySelectorAll<T>(selector)];

/** Escape everything that reaches innerHTML. Screen text is untrusted input. */
export function esc(value: unknown): string {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

export function fmtBytes(bytes: number): string {
  if (!bytes) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value < 10 && unit > 0 ? 1 : 0)} ${units[unit]}`;
}

export function fmtNumber(value: number): string {
  return new Intl.NumberFormat(undefined).format(value ?? 0);
}

export function fmtDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "—";
  if (seconds < 60) return `${Math.round(seconds)}s`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest ? `${hours}h ${rest}m` : `${hours}h`;
}

export function fmtTime(iso: string | null): string {
  if (!iso) return "—";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "—";
  return date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

export function fmtDate(iso: string | null): string {
  if (!iso) return "—";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "—";
  return date.toLocaleDateString(undefined, { weekday: "short", day: "numeric", month: "short" });
}

export function relTime(iso: string | null): string {
  if (!iso) return "—";
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "—";
  const diff = Math.round((Date.now() - then) / 1000);
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.round(diff / 60)}m ago`;
  if (diff < 86_400) return `${Math.round(diff / 3600)}h ago`;
  return `${Math.round(diff / 86_400)}d ago`;
}

export function emptyState(title: string, body: string): string {
  return `<div class="empty"><h3>${esc(title)}</h3><p>${esc(body)}</p></div>`;
}

export function notice(body: string, bad = false): string {
  return `<div class="notice${bad ? " bad" : ""}"><span aria-hidden="true">${bad ? "⚠" : "ℹ"}</span><span>${esc(
    body,
  )}</span></div>`;
}

export function kindPill(kind: string): string {
  const cls = kind === "frame" ? "accent" : kind === "audio" ? "warn" : "";
  const label = kind === "frame" ? "screen" : kind === "event" ? "event" : kind === "audio" ? "audio" : kind;
  return `<span class="pill ${cls}">${esc(label)}</span>`;
}

export function statusPill(status: string): string {
  const map: Record<string, string> = { active: "ok", ready: "ok", planned: "warn", error: "bad", "needs-runtime": "warn" };
  return `<span class="pill ${map[status] ?? ""}">${esc(status)}</span>`;
}

/** Turn FTS `«match»` markers into <mark>, with everything else escaped. */
export function highlighted(text: string): string {
  const parts = String(text ?? "").split(/«([^»]*)»/g);
  return parts
    .map((part, index) => (index % 2 === 1 ? `<mark>${esc(part)}</mark>` : esc(part)))
    .join("");
}

let toastTimer: number | undefined;

export function toast(message: string, bad = false): void {
  const node = $("#toast");
  if (!node) return;
  node.textContent = message;
  node.classList.toggle("bad", bad);
  node.classList.add("show");
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => node.classList.remove("show"), 3200);
}
