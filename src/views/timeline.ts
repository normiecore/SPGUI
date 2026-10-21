import { api, thumbnailUrl } from "../bridge";
import type { ViewDef } from "../context";
import type { TimelineSession } from "../types";
import { emptyState, esc, fmtDate, fmtDuration, fmtTime, notice } from "../ui";

function sessionCard(session: TimelineSession): string {
  const thumb = thumbnailUrl(session.thumbnail);
  const events = session.events
    .slice(0, 5)
    .map(
      (event) =>
        `<span class="event-chip" title="${esc(event.detail ?? event.label)}">${esc(event.label)}</span>`,
    )
    .join("");

  return `<article class="card session">
    <div class="session-head">
      <span class="session-app">${esc(session.app)}</span>
      <span class="session-window" title="${esc(session.window)}">${esc(session.window || "—")}</span>
      <span class="session-time">${esc(fmtDate(session.startedAt))} · ${esc(fmtTime(session.startedAt))}–${esc(
        fmtTime(session.endedAt),
      )}</span>
    </div>
    ${session.preview ? `<div class="session-preview">${esc(session.preview)}</div>` : ""}
    <div class="session-meta">
      <span class="pill">${esc(fmtDuration(session.durationSeconds))}</span>
      <span class="pill">${session.frameCount} frames</span>
      <span class="pill">${session.eventCount} events</span>
      ${events}
    </div>
    ${thumb ? `<img class="session-thumb" src="${esc(thumb)}" alt="Screen capture from ${esc(session.app)}" loading="lazy" />` : ""}
  </article>`;
}

export const timelineView: ViewDef = {
  id: "timeline",
  title: "Timeline",
  sub: "Every interaction across every app, grouped into sessions from Screenpipe's local database.",
  showRange: true,

  async render(ctx) {
    const page = await api.timeline(ctx.days, 80);
    const sessions = page.sessions;

    if (!sessions.length) {
      ctx.view.innerHTML =
        notice(
          "No frames in this window. Screenpipe records only while it is running — start it, use your computer for a minute, then refresh.",
        ) +
        emptyState(
          "Nothing recorded yet",
          "Deja reads Screenpipe's database directly. If Screenpipe is running and you still see this, widen the time range.",
        );
      return;
    }

    const shown = sessions.length;
    const truncated = page.totalSessions > shown;

    ctx.view.innerHTML = `<div class="row" style="margin-bottom:10px">
        <span class="pill accent">${page.totalSessions} sessions</span>
        <span class="pill">${page.totalFrames} frames</span>
        <span class="pill">${page.totalEvents} events</span>
        <span class="pill">${esc(ctx.days === 1 ? "last 24 hours" : `last ${ctx.days} days`)}</span>
        ${
          truncated
            ? `<span class="pill warn">showing the newest ${shown} — narrow the range to see the rest</span>`
            : ""
        }
      </div>
      <div class="stack">${sessions.map(sessionCard).join("")}</div>`;
  },
};
