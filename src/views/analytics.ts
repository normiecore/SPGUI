import { api } from "../bridge";
import type { ViewDef } from "../context";
import { emptyState, esc, fmtBytes, fmtNumber, notice } from "../ui";

const stat = (label: string, value: string, sub?: string): string =>
  `<div class="card stat">
     <div class="stat-label">${esc(label)}</div>
     <div class="stat-value">${esc(value)}</div>
     ${sub ? `<div class="stat-sub">${esc(sub)}</div>` : ""}
   </div>`;

export const analyticsView: ViewDef = {
  id: "analytics",
  title: "Analytics",
  sub: "Where the time went: per-app volume, daily breakdown and hourly distribution.",
  showRange: true,

  async render(ctx) {
    const data = await api.analytics(ctx.days);
    const status = ctx.status;

    if (!data.totalFrames) {
      ctx.view.innerHTML =
        notice("No frames in this window. Screenpipe must be running to record anything.", true) +
        emptyState("No data yet", "Start Screenpipe, use your computer for a minute, then refresh this view.");
      return;
    }

    const maxFrames = Math.max(...data.topApps.map((a) => a.frames), 1);
    const maxHour = Math.max(...data.hourly, 1);
    const rangeLabel = ctx.days === 1 ? "24 hours" : `${ctx.days} days`;

    const bars = data.topApps
      .slice(0, 10)
      .map(
        (app) => `<div class="bar-row">
          <span class="bar-app" title="${esc(app.app)}">${esc(app.app)}</span>
          <div class="bar-track"><div class="bar-fill" style="width:${Math.max(
            2,
            Math.round((app.frames / maxFrames) * 100),
          )}%"></div></div>
          <span class="bar-num">${fmtNumber(app.frames)}</span>
        </div>`,
      )
      .join("");

    const hours = data.hourly
      .map((count, hour) => {
        const intensity = count / maxHour;
        const opacity = count ? 0.18 + intensity * 0.82 : 0.06;
        return `<div class="hour" title="${String(hour).padStart(2, "0")}:00 — ${count} frames" style="background:rgba(124,156,255,${opacity.toFixed(
          2,
        )})"></div>`;
      })
      .join("");

    const hourLabels = data.hourly
      .map((_, hour) => `<span>${hour % 3 === 0 ? String(hour).padStart(2, "0") : ""}</span>`)
      .join("");

    const dailyRows = data.daily
      .map(
        (day) => `<tr>
          <td>${esc(day.date)}</td>
          <td class="num">${fmtNumber(day.total)}</td>
          <td>${day.apps
            .slice(0, 3)
            .map((app) => `${esc(app.app)} <span style="color:var(--text-faint)">${app.frames}</span>`)
            .join(" · ") || "—"}</td>
        </tr>`,
      )
      .join("");

    ctx.view.innerHTML = `
      <div class="stat-grid">
        ${stat("Frames", fmtNumber(data.totalFrames), rangeLabel)}
        ${stat("UI events", fmtNumber(data.totalEvents), "clicks, keys, app switches")}
        ${stat("Transcriptions", fmtNumber(data.totalTranscriptions), status?.transcriptionEngine ?? "audio")}
        ${stat("Apps seen", fmtNumber(data.distinctApps), "distinct processes")}
        ${stat("On disk", fmtBytes(status?.storageBytes ?? 0), `${fmtBytes(status?.dbBytes ?? 0)} database`)}
      </div>

      <div class="card">
        <div class="section-title" style="margin-top:0">Top apps by frames</div>
        <div class="bars">${bars}</div>
      </div>

      <div class="card" style="margin-top:12px">
        <div class="section-title" style="margin-top:0">Hourly distribution (local time)</div>
        <div class="hour-grid">${hours}</div>
        <div class="hour-labels">${hourLabels}</div>
      </div>

      <div class="card" style="margin-top:12px">
        <div class="section-title" style="margin-top:0">Daily breakdown</div>
        <table class="grid">
          <thead><tr><th>Date</th><th style="text-align:right">Frames</th><th>Top apps</th></tr></thead>
          <tbody>${dailyRows || `<tr><td colspan="3">No rows</td></tr>`}</tbody>
        </table>
      </div>`;
  },
};
