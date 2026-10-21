import { api } from "../bridge";
import type { ViewDef } from "../context";
import type { PluginInfo, PluginRun } from "../types";
import { emptyState, esc, statusPill } from "../ui";

const outputs = new Map<string, PluginRun | string>();

function card(plugin: PluginInfo): string {
  const runnable = plugin.status === "ready" || plugin.status === "active";
  const output = outputs.get(plugin.id);

  return `<article class="card plugin" data-plugin="${esc(plugin.id)}">
    <div class="plugin-main">
      <div class="plugin-head">
        <span class="plugin-name">${esc(plugin.name)}</span>
        <span class="pill accent">${esc(plugin.kind)}</span>
        ${statusPill(plugin.status)}
        ${plugin.version ? `<span class="pill">v${esc(plugin.version)}</span>` : ""}
        ${plugin.builtin ? `<span class="pill">built-in</span>` : ""}
      </div>
      <div class="plugin-desc">${esc(plugin.description)}</div>
      ${
        plugin.problem
          ? `<div class="notice bad" style="margin-top:8px"><span>⚠</span><span>${esc(plugin.problem)}</span></div>`
          : ""
      }
      <div class="plugin-caps">
        ${plugin.capabilities.map((cap) => `<span class="pill">${esc(cap)}</span>`).join("")}
      </div>
      ${
        output
          ? typeof output === "string"
            ? `<div class="notice bad" style="margin-top:8px"><span>⚠</span><span>${esc(output)}</span></div>`
            : `<pre class="output">${esc(
                `${output.ok ? "✓" : "✗"} exit ${output.exitCode ?? "?"} · ${output.durationMs} ms\n\n${
                  output.stdout || output.stderr || "(no output)"
                }`,
              )}</pre>`
          : ""
      }
    </div>
    <div class="plugin-actions">
      ${
        plugin.builtin
          ? ""
          : `<button class="btn${runnable ? " primary" : ""}" data-run="${esc(plugin.id)}" type="button" ${
              runnable ? "" : "disabled"
            }>Run</button>`
      }
      ${
        plugin.builtin && plugin.status === "active"
          ? `<span class="pill ok">in use</span>`
          : `<button class="switch${plugin.enabled ? " on" : ""}" data-toggle="${esc(plugin.id)}" data-enabled="${
              plugin.enabled ? "1" : "0"
            }" type="button" title="${plugin.enabled ? "Disable" : "Enable"} ${esc(plugin.name)}"></button>`
      }
    </div>
  </article>`;
}

export const pluginsView: ViewDef = {
  id: "plugins",
  title: "Plugins",
  sub: "Providers and plugins. Anything that is not built in runs as a child process: JSON in on stdin, results out on stdout.",
  showRange: false,

  async render(ctx) {
    let plugins = await api.plugins();

    const draw = () => {
      ctx.view.innerHTML = `
        <div class="row" style="margin-bottom:12px">
          <button class="btn" id="refresh-plugins" type="button">Refresh</button>
          <button class="btn" id="open-plugin-dir" type="button">Open plugins folder</button>
          <button class="btn" id="seed-plugin" type="button">Install example plugin</button>
        </div>
        <div class="stack">${
          plugins.length
            ? plugins.map(card).join("")
            : emptyState("No plugins", "Drop a folder with a plugin.json into the plugins directory.")
        }</div>
        <div class="card" style="margin-top:14px">
          <div class="section-title" style="margin-top:0">Plugin contract</div>
          <div style="color:var(--text-dim);font-size:12.5px;line-height:1.6">
            A plugin is a folder under <code>${esc(ctx.appInfo.pluginsDir || "<app data>/plugins")}</code> with a
            <code>plugin.json</code> manifest and an entry point. Deja spawns the command declared in
            <code>runtime.command</code>, writes a JSON payload to stdin, and shows stdout/stderr here. Environment:
            <code>DEJA_DATA_DIR</code>, <code>DEJA_PLUGIN_DIR</code>, <code>DEJA_FRAMES_DB</code>. Runs are capped at 60 seconds.
          </div>
        </div>`;

      ctx.view.querySelectorAll<HTMLElement>("[data-run]").forEach((button) => {
        button.addEventListener("click", async () => {
          const id = button.dataset.run;
          if (!id) return;
          button.setAttribute("disabled", "true");
          outputs.set(id, "Running…");
          draw();
          try {
            const result = await api.runPlugin(id);
            outputs.set(id, result);
            ctx.toast(result.ok ? "Plugin finished" : "Plugin exited with an error", !result.ok);
          } catch (err) {
            outputs.set(id, err instanceof Error ? err.message : String(err));
            ctx.toast("Plugin failed", true);
          }
          draw();
        });
      });

      ctx.view.querySelectorAll<HTMLElement>("[data-toggle]").forEach((button) => {
        button.addEventListener("click", async () => {
          const id = button.dataset.toggle;
          if (!id) return;
          const enabled = button.dataset.enabled !== "1";
          try {
            plugins = await api.setPluginEnabled(id, enabled);
            ctx.toast(`${enabled ? "Enabled" : "Disabled"} ${id}`);
          } catch (err) {
            ctx.toast(err instanceof Error ? err.message : String(err), true);
          }
          draw();
        });
      });

      ctx.view.querySelector("#refresh-plugins")?.addEventListener("click", async () => {
        plugins = await api.refreshPlugins();
        ctx.toast("Plugin list refreshed");
        draw();
      });

      ctx.view.querySelector("#open-plugin-dir")?.addEventListener("click", () => {
        void api.openPath(ctx.appInfo.pluginsDir);
      });

      ctx.view.querySelector("#seed-plugin")?.addEventListener("click", async () => {
        try {
          const dir = await api.seedExamplePlugin();
          plugins = await api.refreshPlugins();
          ctx.toast(`Example installed in ${dir}`);
        } catch (err) {
          ctx.toast(err instanceof Error ? err.message : String(err), true);
        }
        draw();
      });
    };

    draw();
  },
};
