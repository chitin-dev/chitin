/** Optional real-browser smoke: start chromedriver, then run node --import tsx tests/browser-smoke.ts. */
import assert from "node:assert/strict";
import { mkdir, stat, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { cacheRoot, runDirectory } from "../server/paths";
import type { PlotData } from "../shared/types";

const driver = process.env.WEBDRIVER_URL ?? "http://127.0.0.1:9516";
const url = process.env.CHITIN_PLOTS_URL ?? "http://127.0.0.1:5174";
const origin = new URL(url).origin;
async function command(path: string, body?: unknown): Promise<any> {
  const response = await fetch(`${driver}${path}`, {
    method: body === undefined ? "GET" : "POST",
    headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const { value } = await response.json();
  if (!response.ok || value?.error) throw new Error(JSON.stringify(value));
  return value;
}
const session = await command("/session", {
  capabilities: {
    alwaysMatch: {
      browserName: "chrome",
      "goog:chromeOptions": {
        binary: "/usr/bin/chromium",
        args: ["--headless=new", "--window-size=1600,1550", "--disable-dev-shm-usage"],
      },
      "goog:loggingPrefs": { browser: "ALL" },
    },
  },
});
const root = `/session/${session.sessionId}`;
const execute = (script: string, args: unknown[] = []) => command(`${root}/execute/sync`, { script, args });
async function until(script: string): Promise<any> {
  const deadline = Date.now() + 30000;
  do {
    const result = await execute(script);
    if (result) return result;
    await new Promise((resolve) => setTimeout(resolve, 100));
  } while (Date.now() < deadline);
  throw new Error(`Browser condition did not resolve: ${script}`);
}
try {
  await command(`${root}/url`, { url });
  const state = await until(
    "return document.querySelector('canvas') && !document.querySelector('.loading-line') && {panels:document.querySelectorAll('.plot-panel').length, text:document.querySelector('.run-context')?.textContent}",
  );
  assert.equal(state.panels, 7);
  assert.equal(await execute("return document.querySelectorAll('.inspector img').length"), 0);
  console.log("Initial charts ready:", state);
  await mkdir(cacheRoot, { recursive: true });
  await writeFile(join(cacheRoot, "overview.png"), Buffer.from(await command(`${root}/screenshot`), "base64"));
  await execute("[...document.querySelectorAll('button')].find(b=>b.textContent.trim()==='Browse all points').click()");
  await until("return document.querySelectorAll('.point-row').length === 12");
  assert.equal(await execute("return document.querySelectorAll('.point-row').length"), 12);
  const selected = await execute(
    "const button=document.querySelector('.point-row'); const label=button.textContent;button.click();return label",
  );
  await until("const img=document.querySelector('.inspector img');return img?.complete && img.naturalWidth > 0");
  console.log("Lazy SVG rendered:", selected);
  const run = await execute("return new URLSearchParams(location.search).get('run')");
  const plot: PlotData = await fetch(`${origin}/api/runs/${encodeURIComponent(run)}/data`).then((response) =>
    response.json(),
  );
  const eligible = plot.points.filter((point) => point.contacts !== null && point.atoms > 0);
  const deferred = eligible.at(-1)!;
  assert.ok(
    !(await stat(join(runDirectory(run), "svg", `${String(deferred.row).padStart(8, "0")}.svg`)).catch(() => null)),
    "Fixture must need an on-demand SVG",
  );
  const input = await command(`${root}/element`, {
    using: "css selector",
    value: "input[placeholder='Find CNP ID or source row…']",
  });
  const inputId = input["element-6066-11e4-a52e-4f735466cecf"];
  await command(`${root}/element/${inputId}/value`, { text: String(deferred.row) });
  await until("return document.querySelectorAll('.point-row').length === 1");
  await execute("document.querySelector('.point-row').click()");
  await until(
    "const img=document.querySelector('.inspector img');return img?.complete && img.naturalWidth > 0 && img.getAttribute('src').includes('/api/cache/')",
  );
  console.log("Deferred historical SVG rendered:", deferred.row);
  // Restore the unfiltered chart and click a known crowded bin through real pointer actions.
  await execute("document.querySelector('[aria-label=\"Close inspector\"]').click()");
  const geometry = await execute(
    "const canvas=document.querySelector('canvas'),r=canvas.getBoundingClientRect();return {left:r.left,top:r.top,width:r.width,height:r.height}",
  );
  const xmin = Math.min(...eligible.map((point) => Math.log10(point.atoms)));
  const xmax = Math.max(...eligible.map((point) => Math.log10(point.atoms)));
  const ymax = Math.max(1, ...eligible.map((point) => Math.log1p(point.contacts! / 0.5)));
  const gap = Math.max(0.08, (xmax - xmin) * 0.05);
  const bins = new Map<string, typeof eligible>();
  for (const point of eligible) {
    const key = `${point.atoms}:${point.contacts}`;
    const bin = bins.get(key) ?? [];
    bin.push(point);
    bins.set(key, bin);
  }
  const tied = [...bins.values()].sort((a, b) => b.length - a.length)[0];
  const x =
    geometry.left + 65 + ((Math.log10(tied[0].atoms) - xmin + gap) / (xmax - xmin + 2 * gap)) * (geometry.width - 83);
  const y =
    geometry.top +
    geometry.height -
    52 -
    ((Math.log1p(tied[0].contacts! / 0.5) + ymax * 0.03) / (ymax * 1.11)) * (geometry.height - 74);
  await command(`${root}/actions`, {
    actions: [
      {
        type: "pointer",
        id: "mouse",
        parameters: { pointerType: "mouse" },
        actions: [
          { type: "pointerMove", duration: 0, x: Math.round(x), y: Math.round(y) },
          { type: "pointerDown", button: 0 },
          { type: "pointerUp", button: 0 },
        ],
      },
    ],
  });
  const nearby = await until(
    "const t=document.querySelector('.point-list-heading')?.textContent;return t?.includes('nearby candidates') && t",
  );
  assert.ok(Number(nearby.replaceAll(",", "").match(/\d+/)?.[0]) >= tied.length);
  console.log("Coincident bin fully accessible:", tied.length, nearby);
  await until("const img=document.querySelector('.inspector img');return img?.complete && img.naturalWidth > 0");
  const points = await execute("return document.querySelector('.point-list-heading')?.textContent");
  console.log("Point browser:", points);
  if (plot.run.protocol === "diagram-quality-v2") {
    const target =
      plot.points.find((point) => point.id === "CNP0551778.1") ?? plot.points.find((point) => point.cavity != null);
    assert.ok(target, "A measured ring fixture is required");
    for (const metric of ["cavity", "roundness", "footprint"] as const) {
      assert.equal(
        await execute("return document.querySelector(`[data-metric='${arguments[0]}'] canvas`) !== null", [metric]),
        true,
      );
      const input = await command(`${root}/element`, {
        using: "css selector",
        value: `[data-metric='${metric}'] input`,
      });
      await command(`${root}/element/${input["element-6066-11e4-a52e-4f735466cecf"]}/value`, {
        text: String(target.row),
      });
      await until(`return document.querySelectorAll("[data-metric='${metric}'] .point-row").length === 1`);
      await execute("document.querySelector(`[data-metric='${arguments[0]}'] .point-row`).click()", [metric]);
      await until(`return document.querySelector('.inspector .eyebrow')?.textContent === 'SOURCE ROW ${target.row}'`);
      await until("const img=document.querySelector('.inspector img');return img?.complete && img.naturalWidth > 0");
      console.log(`${metric} point selection and SVG passed:`, target.id);
    }
    await execute(
      "const details=document.querySelector('.ring-details');details.open=true;document.querySelector('[data-metric=cavity]').closest('.plot-panel').scrollIntoView({block:'start'})",
    );
    assert.ok(await execute("return document.querySelectorAll('.ring-face').length > 0"));
    await writeFile(join(cacheRoot, "ring-metrics.png"), Buffer.from(await command(`${root}/screenshot`), "base64"));
  } else {
    assert.equal(await execute("return document.querySelectorAll('.metric-empty').length"), 3);
    console.log("Legacy run: all three new metrics explicitly unavailable");
  }
  const screenshot = await command(`${root}/screenshot`);
  await mkdir(cacheRoot, { recursive: true });
  await writeFile(join(cacheRoot, "browser-smoke.png"), Buffer.from(screenshot, "base64"));
  const logs = await command(`${root}/log`, { type: "browser" });
  const errors = logs.filter(
    (entry: { level: string; message: string }) => entry.level === "SEVERE" && !entry.message.includes("favicon.ico"),
  );
  assert.deepEqual(errors, []);
  console.log("Browser smoke passed; screenshot:", join(cacheRoot, "browser-smoke.png"));
} catch (error) {
  console.error(
    "Inspector diagnostics:",
    await execute(
      "return {url:location.href,title:document.title,heading:document.querySelector('h1')?.textContent,panels:document.querySelectorAll('.plot-panel').length,alerts:[...document.querySelectorAll('[role=alert]')].map(e=>e.textContent),text:document.querySelector('.inspector')?.textContent,images:[...document.querySelectorAll('.inspector img')].map(img=>({src:img.getAttribute('src'),complete:img.complete,width:img.naturalWidth})),selected:document.querySelector('.point-row.selected')?.textContent}",
    ),
  );
  console.error("Console:", await command(`${root}/log`, { type: "browser" }));
  throw error;
} finally {
  await fetch(`${driver}${root}`, { method: "DELETE" });
}
