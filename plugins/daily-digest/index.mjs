#!/usr/bin/env node
/**
 * Deja reference plugin.
 *
 * Contract:
 *   stdin  — JSON payload from the host ({ generatedAt, settings, analytics })
 *   stdout — anything you like (shown in the Plugins tab)
 *   env    — DEJA_DATA_DIR, DEJA_PLUGIN_DIR, DEJA_FRAMES_DB
 *
 * Writes exports/deja-digest-<date>.md and prints where it went.
 */
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const readStdin = async () => {
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  return Buffer.concat(chunks).toString("utf8");
};

const bar = (value, max, width = 28) => {
  const filled = Math.round((value / Math.max(max, 1)) * width);
  return "█".repeat(filled) + "░".repeat(Math.max(0, width - filled));
};

const bytes = (n) => {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(value < 10 && unit > 0 ? 1 : 0)} ${units[unit]}`;
};

const main = async () => {
  const raw = await readStdin();
  let payload = {};
  try {
    payload = raw.trim() ? JSON.parse(raw) : {};
  } catch {
    console.error("daily-digest: stdin was not valid JSON");
    process.exit(2);
  }

  const analytics = payload.analytics ?? {};
  const settings = payload.settings ?? {};
  const days = analytics.days ?? settings.daysDefault ?? 7;
  const dataDir = process.env.DEJA_DATA_DIR ?? process.cwd();
  const exportsDir = join(dataDir, "exports");
  mkdirSync(exportsDir, { recursive: true });

  const today = new Date();
  const stamp = today.toISOString().slice(0, 10);
  const lines = [];

  lines.push(`# Deja digest — ${stamp}`, "");
  lines.push(`Window: last ${days} day${days === 1 ? "" : "s"} · generated ${today.toLocaleString()}`, "");

  lines.push("## Totals", "");
  lines.push(`- Frames captured: **${analytics.totalFrames ?? 0}**`);
  lines.push(`- UI events: **${analytics.totalEvents ?? 0}**`);
  lines.push(`- Audio transcriptions: **${analytics.totalTranscriptions ?? 0}**`);
  lines.push(`- Apps seen: **${analytics.distinctApps ?? 0}**`);

  const top = analytics.topApps ?? [];
  if (top.length) {
    const max = Math.max(...top.map((a) => a.frames ?? 0), 1);
    lines.push("", "## Top apps", "");
    for (const app of top.slice(0, 10)) {
      lines.push(`- \`${app.app}\` ${bar(app.frames ?? 0, max)} ${app.frames ?? 0} frames · ${app.events ?? 0} events`);
    }
  }

  const hourly = analytics.hourly ?? [];
  if (hourly.some((h) => h > 0)) {
    const peak = Math.max(...hourly, 1);
    lines.push("", "## Busiest hours", "");
    hourly.forEach((count, hour) => {
      if (count > 0) {
        lines.push(`- ${String(hour).padStart(2, "0")}:00 ${bar(count, peak, 20)} ${count}`);
      }
    });
  }

  const daily = analytics.daily ?? [];
  if (daily.length) {
    lines.push("", "## By day", "");
    lines.push("| Date | Frames | Top app |", "| --- | --- | --- |");
    for (const day of daily.slice(0, 14)) {
      const lead = (day.apps ?? [])[0];
      lines.push(`| ${day.date} | ${day.total} | ${lead ? `${lead.app} (${lead.frames})` : "—"} |`);
    }
  }

  if (payload.status) {
    const s = payload.status;
    lines.push("", "## Recorder", "");
    lines.push(`- Screenpipe: ${s.online ? "online" : "offline"}${s.version ? ` v${s.version}` : ""}`);
    lines.push(`- Storage: ${bytes(s.storageBytes ?? 0)} (database ${bytes(s.dbBytes ?? 0)})`);
  }

  lines.push("", "---", "", "_Generated locally by Deja. Nothing left this machine._", "");

  const outPath = join(exportsDir, `deja-digest-${stamp}.md`);
  writeFileSync(outPath, lines.join("\n"), "utf8");
  console.log(`Digest written to ${outPath}`);
};

main().catch((err) => {
  console.error(err?.stack ?? String(err));
  process.exit(1);
});
