import { api } from "../bridge";
import type { ViewDef } from "../context";
import type { FlowReport } from "../types";
import { emptyState, esc, fmtNumber, notice } from "../ui";

const W = 780;
const NODE_W = 138;
const NODE_H = 42;
const PAD = 14;
const ROW = 74;

function label(app: string): string {
  return app.replace(/\.exe$/i, "");
}

function short(app: string, max = 16): string {
  const name = label(app);
  return name.length > max ? `${name.slice(0, max - 1)}…` : name;
}

/** Left-to-right map: x by mean position in the case, y by that same order. */
function graph(report: FlowReport): string {
  const nodes = report.steps.slice(0, 9);
  if (nodes.length < 2) return "";

  const maxPos = Math.max(...nodes.map((n) => n.avgPosition), 0.001);
  const order = [...nodes].sort((a, b) => a.avgPosition - b.avgPosition);
  const rowOf = new Map(order.map((n, index) => [n.app, index]));

  const pos = new Map(
    nodes.map((n) => {
      const x = PAD + (n.avgPosition / maxPos) * (W - NODE_W - PAD * 2);
      const y = 18 + (rowOf.get(n.app) ?? 0) * ROW;
      return [n.app, { x, y }];
    }),
  );

  const height = order.length * ROW + 30;
  const maxEdge = Math.max(...report.edges.map((e) => e.count), 1);

  const edges = report.edges
    .filter((e) => pos.has(e.from) && pos.has(e.to) && e.count > 1)
    .slice(0, 24)
    .map((edge) => {
      const from = pos.get(edge.from)!;
      const to = pos.get(edge.to)!;
      const forward = to.x >= from.x;
      const x1 = forward ? from.x + NODE_W : from.x;
      const x2 = forward ? to.x : to.x + NODE_W;
      const y1 = from.y + NODE_H / 2;
      const y2 = to.y + NODE_H / 2;
      const mid = (x1 + x2) / 2;
      const width = 1.2 + 5 * (edge.count / maxEdge);
      const opacity = 0.35 + 0.5 * (edge.count / maxEdge);
      return `<path d="M ${x1} ${y1} C ${mid} ${y1}, ${mid} ${y2}, ${x2} ${y2}"
                    fill="none" stroke="#6b8afd" stroke-width="${width.toFixed(1)}"
                    stroke-opacity="${opacity.toFixed(2)}" marker-end="url(#flow-arrow)" />`;
    })
    .join("");

  const boxes = nodes
    .map((node) => {
      const p = pos.get(node.app)!;
      const share = node.cases / Math.max(report.cases, 1);
      return `<g>
        <rect x="${p.x}" y="${p.y}" width="${NODE_W}" height="${NODE_H}" rx="9"
              fill="#171b24" stroke="#313848" stroke-width="1" />
        <rect x="${p.x}" y="${p.y}" width="4" height="${NODE_H}" rx="2"
              fill="#7c9cff" fill-opacity="${(0.35 + share * 0.65).toFixed(2)}" />
        <text x="${p.x + 14}" y="${p.y + 18}" fill="#e7eaf0" font-size="12.5"
              font-family="Segoe UI, sans-serif">${esc(short(node.app))}</text>
        <text x="${p.x + 14}" y="${p.y + 32}" fill="#6b7280" font-size="10.5"
              font-family="ui-monospace, monospace">${node.cases} cases · ${node.occurrences}×</text>
      </g>`;
    })
    .join("");

  return `<svg viewBox="0 0 ${W} ${height}" width="100%" height="${height}" role="img"
               aria-label="Directly-follows process map">
      <defs>
        <marker id="flow-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6"
                markerHeight="6" orient="auto-start-reverse">
          <path d="M 0 0 L 10 5 L 0 10 z" fill="#6b8afd" />
        </marker>
      </defs>
      ${edges}
      ${boxes}
    </svg>`;
}

export const flowsView: ViewDef = {
  id: "flows",
  title: "Flows",
  sub: "Process map over your own history: repeating app sequences, the standard path, and its variants.",
  showRange: true,

  async render(ctx) {
    const report = await api.flows(ctx.days);

    if (!report.cases) {
      ctx.view.innerHTML =
        notice(
          "A case needs at least two different apps inside one burst of activity. Widen the range, or use the machine normally for a while.",
        ) +
        emptyState(
          "No flows yet",
          "Deja groups activity into cases — a case ends after a 2 minute pause — then counts which app followed which.",
        );
      return;
    }

    const stdShare = Math.round(report.linearity * 100);
    const variants = report.variants
      .map(
        (variant) => `<div class="card" style="padding:11px 13px">
          <div class="row" style="gap:6px">
            ${variant.standard ? `<span class="pill accent">standard</span>` : `<span class="pill">variant</span>`}
            <span class="pill">${variant.cases} cases</span>
            <span class="pill">${variant.avgMinutes.toFixed(1)} min avg</span>
            <span class="spacer"></span>
          </div>
          <div class="row" style="gap:5px;margin-top:8px;flex-wrap:wrap">
            ${variant.steps
              .map(
                (step, index) =>
                  `${index ? `<span style="color:var(--text-faint)">→</span>` : ""}
                   <span class="event-chip" title="${esc(step)}">${esc(short(step, 22))}</span>`,
              )
              .join("")}
          </div>
        </div>`,
      )
      .join("");

    const chipRow = (pairs: [string, number][]) =>
      pairs
        .slice(0, 4)
        .map(
          ([app, count]) =>
            `<span class="pill">${esc(short(app, 18))} <span style="color:var(--text-faint)">${count}</span></span>`,
        )
        .join("");

    ctx.view.innerHTML = `
      <div class="stat-grid">
        <div class="card stat"><div class="stat-label">Cases</div>
          <div class="stat-value">${fmtNumber(report.cases)}</div>
          <div class="stat-sub">split at ${report.gapSeconds}s idle or 24 apps</div></div>
        <div class="card stat"><div class="stat-label">Activities</div>
          <div class="stat-value">${fmtNumber(report.activities)}</div>
          <div class="stat-sub">distinct apps in the flow</div></div>
        <div class="card stat"><div class="stat-label">Standard path</div>
          <div class="stat-value">${stdShare}%</div>
          <div class="stat-sub">of cases follow the top variant</div></div>
        <div class="card stat"><div class="stat-label">Avg case</div>
          <div class="stat-value">${report.avgMinutes.toFixed(1)}m</div>
          <div class="stat-sub">${report.avgSteps.toFixed(1)} steps on average</div></div>
      </div>

      <div class="card">
        <div class="section-title" style="margin-top:0">Process map</div>
        ${graph(report)}
        <div class="row" style="margin-top:6px">
          <span class="pill">node = app</span>
          <span class="pill">edge width = how often one app followed the other</span>
        </div>
      </div>

      <div class="section-title">Variants — the standard path and its exceptions</div>
      <div class="stack">${variants}</div>

      <div class="row" style="margin-top:14px">
        <span class="pill">starts: ${chipRow(report.startSteps)}</span>
        <span class="pill">ends: ${chipRow(report.endSteps)}</span>
      </div>

      ${
        report.cases < 10
          ? `<div class="notice" style="margin-top:14px"><span aria-hidden="true">⚠</span>
             <span>Only ${report.cases} cases in this window — the variants are a sketch until
             there is more history.</span></div>`
          : ""
      }

      <div class="notice" style="margin-top:14px">
        <span aria-hidden="true">ℹ</span>
        <span>Mined from app switches and captured frames. A "case" is one burst of work; steps are
        the apps you were in, with consecutive repeats collapsed. Nothing is inferred about what you
        did inside an app yet — that needs the perception provider.</span>
      </div>`;
  },
};
