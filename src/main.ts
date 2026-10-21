import { api, isTauri } from "./bridge";
import { RANGES, type ViewContext, type ViewDef } from "./context";
import type { AppInfo, Settings, Status } from "./types";
import { $, $$, esc, fmtBytes, notice, relTime, toast } from "./ui";
import { analyticsView } from "./views/analytics";
import { flowsView } from "./views/flows";
import { pluginsView } from "./views/plugins";
import { searchView } from "./views/search";
import { settingsView } from "./views/settings";
import { storageView } from "./views/storage";
import { timelineView } from "./views/timeline";

const views: ViewDef[] = [
  timelineView,
  searchView,
  analyticsView,
  flowsView,
  storageView,
  pluginsView,
  settingsView,
];

const defaultSettings: Settings = {
  dataDir: "",
  daysDefault: 7,
  excludedApps: [],
  theme: "dark",
  pluginsEnabled: {},
  llmModel: "",
};

let days = 7;
let status: Status | null = null;
let settings: Settings = { ...defaultSettings };
let appInfo: AppInfo = {
  name: "Deja",
  version: "0.1.0",
  dataDir: "",
  pluginsDir: "",
  settingsPath: "",
  screenpipeDir: "",
  dbPath: "",
  dbExists: false,
};

const currentView = (): ViewDef => {
  const id = location.hash.replace(/^#\/?/, "") || "timeline";
  return views.find((v) => v.id === id) ?? views[0];
};

const rangeChips = (): string =>
  `<div class="chips" id="range-chips">${RANGES.map(
    (r) =>
      `<button class="chip${r.days === days ? " active" : ""}" data-days="${r.days}" type="button">${esc(
        r.label,
      )}</button>`,
  ).join("")}</div><div id="view-actions" class="row"></div>`;

const ctx: ViewContext = {
  get days() {
    return days;
  },
  setDays(next: number) {
    days = next;
    void render(true);
  },
  get status() {
    return status;
  },
  get settings() {
    return settings;
  },
  get appInfo() {
    return appInfo;
  },
  get view() {
    return $("#view") as HTMLElement;
  },
  get actions() {
    return ($("#view-actions") as HTMLElement) ?? ($("#topbar-actions") as HTMLElement);
  },
  setHeader(title: string, sub: string) {
    const titleNode = $("#view-title");
    const subNode = $("#view-sub");
    if (titleNode) titleNode.textContent = title;
    if (subNode) subNode.textContent = sub;
  },
  toast,
  reload() {
    void render(true);
  },
};

async function render(withSpinner = false) {
  const view = currentView();
  const viewNode = $("#view");
  const actionsNode = $("#topbar-actions");
  if (!viewNode || !actionsNode) return;

  $$(".nav-item").forEach((node) => {
    node.classList.toggle("active", node.getAttribute("data-view") === view.id);
  });

  ctx.setHeader(view.title, view.sub);
  actionsNode.innerHTML = view.showRange ? rangeChips() : `<div id="view-actions" class="row"></div>`;

  if (withSpinner) {
    viewNode.innerHTML = `<div class="empty"><h3>Loading…</h3></div>`;
  }

  try {
    await view.render(ctx);
  } catch (err) {
    viewNode.innerHTML = notice(err instanceof Error ? err.message : String(err), true);
  }
}

async function renderStatus() {
  const dot = $("#status-dot");
  const title = $("#status-title");
  const sub = $("#status-sub");
  const storage = $("#storage-line");
  if (!dot || !title || !sub || !storage) return;

  if (!status) {
    dot.className = "dot";
    title.textContent = "Checking…";
    sub.textContent = "Screenpipe";
    return;
  }

  const online = status.online;
  dot.className = `dot ${online ? "ok" : "bad"}`;
  title.textContent = online ? "Recording" : "Offline";
  sub.textContent = online
    ? `Screenpipe ${status.version ?? ""}${status.captureState ? ` · ${status.captureState}` : ""}`.trim()
    : "Screenpipe not detected";

  const parts = [`${fmtBytes(status.dbBytes)} db`, `${status.framesCaptured} frames`];
  if (status.lastFrameTs) parts.push(`last ${relTime(status.lastFrameTs)}`);
  storage.textContent = parts.join(" · ");
}

async function refreshStatus() {
  try {
    status = await api.status();
  } catch {
    status = null;
  }
  await renderStatus();
}

function bindShell() {
  $$(".nav-item").forEach((node) => {
    node.addEventListener("click", () => {
      const view = node.getAttribute("data-view");
      if (view) location.hash = `#/${view}`;
    });
  });

  $("#topbar-actions")?.addEventListener("click", (event) => {
    const target = (event.target as HTMLElement).closest("[data-days]");
    if (!target) return;
    const next = Number(target.getAttribute("data-days"));
    if (Number.isFinite(next) && next !== days) {
      days = next;
      void render(true);
    }
  });

  window.addEventListener("hashchange", () => void render(true));

  window.addEventListener("keydown", (event) => {
    if (event.ctrlKey && event.key.toLowerCase() === "k") {
      event.preventDefault();
      location.hash = "#/search";
      window.setTimeout(() => ($("#search-input") as HTMLInputElement | null)?.focus(), 60);
      return;
    }
    if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) return;
    const index = Number(event.key);
    if (index >= 1 && index <= views.length) {
      location.hash = `#/${views[index - 1].id}`;
    }
  });
}

async function boot() {
  bindShell();

  const [info, loaded] = await Promise.all([
    api.appInfo().catch(() => appInfo),
    api.getSettings().catch(() => defaultSettings),
  ]);
  appInfo = info;
  settings = { ...defaultSettings, ...loaded };
  days = settings.daysDefault || 7;
  document.documentElement.dataset.theme = settings.theme === "light" ? "light" : "dark";

  if (!location.hash) location.hash = "#/timeline";

  await refreshStatus();
  await render(true);
  window.setInterval(() => void refreshStatus(), 15_000);

  if (!isTauri) {
    const banner = document.createElement("div");
    banner.className = "notice";
    banner.innerHTML = `<span aria-hidden="true">ℹ</span><span>Browser preview — the data below is sample data. Run <code>npm run tauri dev</code> for the real thing.</span>`;
    $("#view")?.prepend(banner);
  }
}

void boot();
