import { api } from "../bridge";
import type { ViewDef } from "../context";
import { esc, fmtBytes, fmtNumber, notice } from "../ui";

function bars(rows: { label: string; bytes: number; sub?: string }[]): string {
  const max = Math.max(...rows.map((r) => r.bytes), 1);
  return rows
    .map(
      (row) => `<div class="bar-row">
        <span class="bar-app" title="${esc(row.label)}">${esc(row.label)}</span>
        <div class="bar-track"><div class="bar-fill" style="width:${Math.max(
          2,
          Math.round((row.bytes / max) * 100),
        )}%"></div></div>
        <span class="bar-num">${esc(fmtBytes(row.bytes))}</span>
      </div>`,
    )
    .join("");
}

export const storageView: ViewDef = {
  id: "storage",
  title: "Storage",
  sub: "What the recorder actually costs on disk, measured — not estimated.",
  showRange: false,

  async render(ctx) {
    const report = await api.storage();

    if (!report.totalBytes) {
      ctx.view.innerHTML = notice("No Screenpipe data folder found at " + report.dataDir, true);
      return;
    }

    const columnTotal = report.columns.reduce((sum, c) => sum + c.bytes, 0) || 1;
    const a11yShare = Math.round((report.a11yJsonBytes / columnTotal) * 100);
    const reclaimable = report.a11yJsonBytes + report.mediaOver30dBytes;
    const perDay = report.mediaByDay.slice(0, 14);

    ctx.view.innerHTML = `
      <div class="stat-grid">
        <div class="card stat"><div class="stat-label">On disk</div>
          <div class="stat-value">${esc(fmtBytes(report.totalBytes))}</div>
          <div class="stat-sub">${esc(report.dataDir)}</div></div>
        <div class="card stat"><div class="stat-label">Database</div>
          <div class="stat-value">${esc(fmtBytes(report.dbBytes))}</div>
          <div class="stat-sub">+ ${esc(fmtBytes(report.walBytes))} write-ahead log</div></div>
        <div class="card stat"><div class="stat-label">Media</div>
          <div class="stat-value">${esc(fmtBytes(report.mediaBytes))}</div>
          <div class="stat-sub">${report.frameFiles} frames · ${report.audioFiles} audio clips</div></div>
        <div class="card stat"><div class="stat-label">Duplicated text</div>
          <div class="stat-value">${a11yShare}%</div>
          <div class="stat-sub">of DB text is the raw a11y tree</div></div>
      </div>

      <div class="card">
        <div class="section-title" style="margin-top:0">Database, by table${report.dbstat ? "" : " (dbstat unavailable)"}</div>
        ${
          report.tables.length
            ? bars(report.tables.map((t) => ({ label: t.name, bytes: t.bytes })))
            : notice("This SQLite build has no dbstat table, so per-table sizes are unavailable.")
        }
      </div>

      <div class="card" style="margin-top:12px">
        <div class="section-title" style="margin-top:0">Where the text bytes go</div>
        ${bars(report.columns.map((c) => ({ label: c.label, bytes: c.bytes })))}
        <div style="color:var(--text-dim);font-size:12.5px;line-height:1.6;margin-top:12px">
          <strong>${esc(fmtBytes(report.a11yJsonBytes))}</strong> sits in
          <code>frames.accessibility_tree_json</code> across ${fmtNumber(report.a11yFrames)} frames
          (${esc(fmtBytes(report.framesRows ? report.a11yJsonBytes / report.framesRows : 0))} per frame, average).
          The same structure is already flattened into <code>elements</code> and
          <code>full_text</code>, which is what search and the timeline actually read — so this column is
          the single biggest storage lever you have.
        </div>
      </div>

      <div class="card" style="margin-top:12px">
        <div class="section-title" style="margin-top:0">Daily media</div>
        <table class="grid">
          <thead><tr><th>Date</th><th style="text-align:right">Files</th><th style="text-align:right">Size</th></tr></thead>
          <tbody>
            ${
              perDay.length
                ? perDay
                    .map(
                      (day) =>
                        `<tr><td>${esc(day.date)}</td><td class="num">${fmtNumber(day.files)}</td><td class="num">${esc(
                          fmtBytes(day.bytes),
                        )}</td></tr>`,
                    )
                    .join("")
                : `<tr><td colspan="3">No media files yet</td></tr>`
            }
          </tbody>
        </table>
      </div>

      <div class="notice" style="margin-top:12px">
        <span aria-hidden="true">ℹ</span>
        <span>Deja never writes to this folder. Roughly <strong>${esc(
          fmtBytes(reclaimable),
        )}</strong> could be reclaimed with a retention policy — Screenpipe accepts
        <code>--retention-days N</code> (media older than N days is dropped, text kept) and
        <code>--retention-mode</code> for how aggressively to prune. Set it in whatever launches the
        recorder; Deja only reports.</span>
      </div>`;
  },
};
