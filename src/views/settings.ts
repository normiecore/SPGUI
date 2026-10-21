import { api } from "../bridge";
import type { ViewDef } from "../context";
import type { Settings } from "../types";
import { esc, fmtBytes, notice, relTime } from "../ui";

export const settingsView: ViewDef = {
  id: "settings",
  title: "Settings",
  sub: "Where the data lives, what Deja ignores, and how plugins are wired up.",
  showRange: false,

  async render(ctx) {
    const settings: Settings = { ...ctx.settings };
    const info = ctx.appInfo;
    const status = ctx.status;
    const rec = await api.recorderStatus().catch(() => null);

    ctx.view.innerHTML = `
      <div class="card">
        <div class="section-title" style="margin-top:0">Recording source</div>
        <div class="field">
          <label for="set-data-dir">Screenpipe data folder</label>
          <input class="input" id="set-data-dir" value="${esc(settings.dataDir)}"
                 placeholder="${esc(info.screenpipeDir || "~/.screenpipe")}" />
          <span class="hint">Leave blank to use Screenpipe's default (<code>~/.screenpipe</code>). Deja only ever reads
          <code>db.sqlite</code> and never writes to it.</span>
        </div>

        <div class="field">
          <label for="set-days">Default time range</label>
          <select class="input" id="set-days">
            <option value="1"${settings.daysDefault === 1 ? " selected" : ""}>Last 24 hours</option>
            <option value="7"${settings.daysDefault === 7 ? " selected" : ""}>Last 7 days</option>
            <option value="30"${settings.daysDefault === 30 ? " selected" : ""}>Last 30 days</option>
          </select>
        </div>

        <div class="field">
          <label for="set-excluded">Excluded apps</label>
          <input class="input" id="set-excluded" value="${esc(settings.excludedApps.join(", "))}"
                 placeholder="deja.exe, 1password.exe" />
          <span class="hint">Comma separated process names. Sessions from these apps are hidden from the timeline.</span>
        </div>

        <div class="field">
          <label for="set-theme">Theme</label>
          <select class="input" id="set-theme">
            <option value="dark"${settings.theme !== "light" ? " selected" : ""}>Dark</option>
            <option value="light"${settings.theme === "light" ? " selected" : ""}>Light</option>
          </select>
        </div>

        <div class="field">
          <label for="set-llm">Model for the agent layer <span class="pill">reserved</span></label>
          <input class="input" id="set-llm" value="${esc(settings.llmModel)}" placeholder="e.g. an OpenRouter model id" />
          <span class="hint">Nothing calls a model yet. The field exists so the agent layer has somewhere to read it
          from once the perception provider lands.</span>
        </div>

        <div class="row">
          <button class="btn primary" id="save-settings" type="button">Save settings</button>
          <button class="btn" id="open-data" type="button">Open app data</button>
          <button class="btn" id="open-plugins" type="button">Open plugins</button>
        </div>
      </div>

      <div class="card" style="margin-top:12px">
        <div class="section-title" style="margin-top:0">Recorder</div>
        ${
          rec
            ? `<div class="row" style="margin-bottom:10px">
                 <span class="pill ${rec.phase === "running" ? "ok" : rec.phase === "failed" ? "bad" : "warn"}">${esc(
                   rec.phase,
                 )}</span>
                 <span class="pill">pid ${rec.pid ?? "—"}</span>
                 <span class="pill">port ${rec.port}</span>
                 <span class="pill">${rec.restarts} restarts</span>
                 ${rec.managed ? `<span class="pill">managed by Deja</span>` : `<span class="pill">external</span>`}
               </div>
               ${rec.message ? notice(rec.message, rec.phase === "failed") : ""}
               <div class="row" style="margin-top:10px">
                 <button class="btn" data-rec="start" type="button" ${rec.desired ? "disabled" : ""}>Start</button>
                 <button class="btn" data-rec="stop" type="button" ${rec.desired ? "" : "disabled"}>Stop</button>
                 <button class="btn" data-rec="restart" type="button">Restart</button>
               </div>
               <div class="field" style="margin-top:14px">
                 <label for="set-retention">Media retention (days)</label>
                 <div class="row">
                   <input class="input" id="set-retention" type="number" min="0" max="3650"
                          value="${rec.retentionDays}" style="max-width:120px" />
                   <button class="btn" id="apply-retention" type="button">Apply &amp; restart</button>
                 </div>
                 <span class="hint">0 keeps everything. Screenpipe drops screenshot and audio files older
                 than this and keeps the text — the cheapest way to control disk use.</span>
               </div>
               <div class="field">
                 <label for="set-autostart">Start with Windows</label>
                 <div class="row">
                   <button class="switch${settings.autostart ? " on" : ""}" id="toggle-autostart"
                           type="button" title="Toggle start at login"></button>
                   <span class="hint">Launches into the tray at login so recording starts with your day.</span>
                 </div>
               </div>`
            : notice("Recorder status unavailable.", true)
        }
        ${
          status?.online
            ? `<dl class="kv" style="margin-top:12px">
                 <dt>Capture</dt><dd>${esc(status.captureState ?? "recording")} · Screenpipe ${esc(status.version ?? "")}</dd>
                 <dt>Frames</dt><dd>${status.framesCaptured.toLocaleString()} captured, ${status.framesWritten.toLocaleString()} written</dd>
                 <dt>Transcriptions</dt><dd>${status.transcriptions.toLocaleString()} · ${esc(status.transcriptionEngine ?? "—")}</dd>
                 <dt>Last frame</dt><dd>${esc(relTime(status.lastFrameTs))}</dd>
                 <dt>Monitors</dt><dd>${esc(status.monitors.join(", ") || "—")}</dd>
                 <dt>Uptime</dt><dd>${Math.round(status.uptimeSeconds / 60)} min</dd>
               </dl>`
            : ""
        }
      </div>

      <div class="card" style="margin-top:12px">
        <div class="section-title" style="margin-top:0">Paths</div>
        <dl class="kv">
          <dt>Screenpipe data</dt><dd>${esc(status?.dataDir || info.screenpipeDir)}</dd>
          <dt>Database</dt><dd>${esc(status?.dbPath || info.dbPath)} ${info.dbExists ? `<span class="pill ok">found</span>` : `<span class="pill bad">missing</span>`}</dd>
          <dt>Deja data</dt><dd>${esc(info.dataDir)}</dd>
          <dt>Plugins</dt><dd>${esc(info.pluginsDir)}</dd>
          <dt>Settings file</dt><dd>${esc(info.settingsPath)}</dd>
          <dt>Database</dt><dd>${esc(fmtBytes(status?.dbBytes ?? 0))} — full disk breakdown on the Storage tab</dd>
        </dl>
      </div>

      <div class="card" style="margin-top:12px">
        <div class="section-title" style="margin-top:0">Privacy</div>
        <div style="color:var(--text-dim);font-size:12.5px;line-height:1.6">
          Deja has no network client and no telemetry. It reads Screenpipe's SQLite file read-only and shells out only
          to plugins you enable. Screenpipe itself controls retention, audio capture and PII redaction — configure those
          in Screenpipe.
        </div>
      </div>`;

    ctx.view.querySelectorAll<HTMLElement>("[data-rec]").forEach((button) => {
      button.addEventListener("click", async () => {
        const action = button.dataset.rec;
        try {
          if (action === "start") await api.recorderStart();
          else if (action === "stop") await api.recorderStop();
          else await api.recorderRestart();
          ctx.toast(`Recorder: ${action}`);
          ctx.reload();
        } catch (err) {
          ctx.toast(err instanceof Error ? err.message : String(err), true);
        }
      });
    });

    ctx.view.querySelector("#apply-retention")?.addEventListener("click", async () => {
      const input = ctx.view.querySelector("#set-retention") as HTMLInputElement | null;
      const days = Math.max(0, Number(input?.value ?? 0) || 0);
      try {
        await api.setRetention(days);
        ctx.settings.retentionDays = days;
        ctx.toast(days ? `Keeping ${days} days of media` : "Keeping all media");
        ctx.reload();
      } catch (err) {
        ctx.toast(err instanceof Error ? err.message : String(err), true);
      }
    });

    ctx.view.querySelector("#toggle-autostart")?.addEventListener("click", async () => {
      const next = !ctx.settings.autostart;
      try {
        await api.setAutostart(next);
        ctx.settings.autostart = next;
        ctx.toast(`Start with Windows ${next ? "enabled" : "disabled"}`);
        ctx.reload();
      } catch (err) {
        ctx.toast(err instanceof Error ? err.message : String(err), true);
      }
    });

    ctx.view.querySelector("#save-settings")?.addEventListener("click", async () => {
      const dataDir = (ctx.view.querySelector("#set-data-dir") as HTMLInputElement).value.trim();
      const days = Number((ctx.view.querySelector("#set-days") as HTMLSelectElement).value) || 7;
      const excluded = (ctx.view.querySelector("#set-excluded") as HTMLInputElement).value
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      const theme = (ctx.view.querySelector("#set-theme") as HTMLSelectElement).value;
      const llmModel = (ctx.view.querySelector("#set-llm") as HTMLInputElement).value.trim();

      const next: Settings = { ...settings, dataDir, daysDefault: days, excludedApps: excluded, theme, llmModel };

      try {
        await api.saveSettings(next);
        ctx.settings.dataDir = dataDir;
        ctx.settings.daysDefault = days;
        ctx.settings.excludedApps = excluded;
        ctx.settings.theme = theme;
        ctx.settings.llmModel = llmModel;
        document.documentElement.dataset.theme = theme === "light" ? "light" : "dark";
        ctx.toast("Settings saved");
        ctx.reload();
      } catch (err) {
        ctx.toast(err instanceof Error ? err.message : String(err), true);
      }
    });

    ctx.view.querySelector("#open-data")?.addEventListener("click", () => void api.openPath(ctx.appInfo.dataDir));
    ctx.view.querySelector("#open-plugins")?.addEventListener("click", () => void api.openPath(ctx.appInfo.pluginsDir));
  },
};
