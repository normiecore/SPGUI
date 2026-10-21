import { api } from "../bridge";
import type { ViewDef } from "../context";
import type { SearchHit } from "../types";
import { emptyState, esc, fmtDate, fmtTime, highlighted, kindPill, notice } from "../ui";

const KINDS = [
  { id: "frame", label: "Screen" },
  { id: "event", label: "Events" },
  { id: "audio", label: "Audio" },
];

let lastQuery = "";
let activeKinds = KINDS.map((k) => k.id);
let lastHits: SearchHit[] = [];
let searched = false;

function hitRow(hit: SearchHit): string {
  return `<article class="card hit">
    <div class="hit-kind">${kindPill(hit.kind)}</div>
    <div class="hit-body">
      <div class="hit-title">
        <span class="hit-app">${esc(hit.app)}</span>
        ${hit.title ? `<span style="color:var(--text-faint)">${esc(hit.title)}</span>` : ""}
        <span class="hit-ts">${esc(fmtDate(hit.ts))} ${esc(fmtTime(hit.ts))}</span>
      </div>
      <div class="hit-snippet">${highlighted(hit.snippet || "(no excerpt)")}</div>
      ${hit.url ? `<div class="hit-url">${esc(hit.url)}</div>` : ""}
    </div>
  </article>`;
}

async function runSearch(ctx: Parameters<ViewDef["render"]>[0]) {
  const input = document.querySelector<HTMLInputElement>("#search-input");
  const results = document.querySelector<HTMLElement>("#search-results");
  if (!input || !results) return;

  lastQuery = input.value.trim();
  if (!lastQuery) {
    lastHits = [];
    results.innerHTML = emptyState("Search your screen memory", "Type a word or phrase. Screen text, UI events and audio transcripts are all searched locally.");
    return;
  }

  results.innerHTML = `<div class="empty"><h3>Searching…</h3></div>`;
  try {
    lastHits = await api.search(lastQuery, ctx.days, activeKinds, 60);
    searched = true;
  } catch (err) {
    results.innerHTML = notice(err instanceof Error ? err.message : String(err), true);
    return;
  }

  if (!lastHits.length) {
    results.innerHTML = emptyState(
      `No matches for “${lastQuery}”`,
      "Try a shorter or more distinctive word — OCR text can split words across lines. Widening the range also helps.",
    );
    return;
  }

  const counts = lastHits.reduce<Record<string, number>>((acc, hit) => {
    acc[hit.kind] = (acc[hit.kind] ?? 0) + 1;
    return acc;
  }, {});

  results.innerHTML = `<div class="row" style="margin:8px 0 10px">
      <span class="pill accent">${lastHits.length} matches</span>
      ${Object.entries(counts)
        .map(([kind, count]) => `<span class="pill">${esc(kind)} ${count}</span>`)
        .join("")}
    </div>
    <div class="stack">${lastHits.map(hitRow).join("")}</div>`;
}

export const searchView: ViewDef = {
  id: "search",
  title: "Search",
  sub: "Full-text search over screen OCR, UI events and audio, straight from Screenpipe's FTS5 indexes.",
  showRange: true,

  async render(ctx) {
    ctx.view.innerHTML = `<div class="searchbar">
        <input class="input" id="search-input" type="search" placeholder="Search everything you've seen…  (Ctrl+K)"
               autocomplete="off" spellcheck="false" value="${esc(lastQuery)}" />
        <button class="btn primary" id="search-go" type="button">Search</button>
      </div>
      <div class="row" style="margin-bottom:12px">
        ${KINDS.map(
          (kind) =>
            `<button class="chip${activeKinds.includes(kind.id) ? " active" : ""}" data-kind="${kind.id}" type="button">${esc(
              kind.label,
            )}</button>`,
        ).join("")}
      </div>
      <div id="search-results"></div>`;

    const input = document.querySelector<HTMLInputElement>("#search-input");
    const results = document.querySelector<HTMLElement>("#search-results");
    if (!input || !results) return;

    input.addEventListener("keydown", (event) => {
      if (event.key === "Enter") void runSearch(ctx);
    });
    document.querySelector("#search-go")?.addEventListener("click", () => void runSearch(ctx));
    document.querySelectorAll<HTMLElement>("[data-kind]").forEach((chip) => {
      chip.addEventListener("click", () => {
        const kind = chip.dataset.kind;
        if (!kind) return;
        activeKinds = activeKinds.includes(kind)
          ? activeKinds.filter((k) => k !== kind)
          : [...activeKinds, kind];
        if (!activeKinds.length) activeKinds = KINDS.map((k) => k.id);
        chip.classList.toggle("active");
        if (lastQuery) void runSearch(ctx);
      });
    });

    if (lastQuery) {
      input.focus();
      await runSearch(ctx);
    } else if (searched) {
      results.innerHTML = emptyState("Search your screen memory", "Type a word or phrase to search locally.");
    } else {
      results.innerHTML = emptyState(
        "Search your screen memory",
        "Every captured frame's text, every click and keystroke label, and every audio transcript is indexed locally. Nothing is sent anywhere.",
      );
    }
  },
};
