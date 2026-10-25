/** Optional real-browser regression: use an already-running loopback plots host and ChromeDriver. */
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { cacheRoot } from "../server/paths";
import { turnCell } from "../shared/turns";
import type { TurnPoint, CaseRecord } from "../shared/types";
import { turnMatches } from "../shared/turns";

const driver = process.env.WEBDRIVER_URL ?? "http://127.0.0.1:9519";
const origin = process.env.CHITIN_PLOTS_URL ?? "http://127.0.0.1:5177";
const run = process.env.CHITIN_TURN_RUN ?? "macrocycle-angles-001";
async function command(path: string, body?: unknown): Promise<any> {
  const response = await fetch(driver + path, {
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
        args: ["--headless=new", "--window-size=1700,1350", "--disable-dev-shm-usage"],
      },
      "goog:loggingPrefs": { browser: "ALL" },
    },
  },
});
const root = `/session/${session.sessionId}`;
const execute = (script: string, args: unknown[] = []) => command(`${root}/execute/sync`, { script, args });
async function until(script: string): Promise<any> {
  const end = Date.now() + 30000;
  do {
    const value = await execute(script);
    if (value) return value;
    await new Promise((r) => setTimeout(r, 100));
  } while (Date.now() < end);
  throw new Error(`Browser condition missing: ${script}`);
}
try {
  await command(`${root}/url`, { url: `${origin}/?run=${encodeURIComponent(run)}` });
  await until("return document.querySelector('.turn-map canvas') && !document.querySelector('.loading-line')");
  const { points }: { points: TurnPoint[] } = await fetch(`${origin}/api/runs/${run}/turns`).then((r) => r.json());
  assert.ok(points.length > 0);
  const plot = await fetch(`${origin}/api/runs/${run}/data`).then((r) => r.json());
  // Exercise every display/raw-value mismatch, including returning to defaults.
  async function choose(label: string, text: string) {
    const trigger = await command(`${root}/element`, { using: "css selector", value: `button[aria-label='${label}']` });
    const element = trigger["element-6066-11e4-a52e-4f735466cecf"];
    await command(`${root}/element/${element}/click`, {});
    await until("return document.querySelector('[role=listbox]')");
    const option = await execute(
      "return [...document.querySelectorAll('[role=option]')].find(e=>e.textContent.replaceAll('✓','').trim()===arguments[0])",
      [text],
    );
    assert.ok(option, `Missing option ${text}`);
    await command(`${root}/element/${option["element-6066-11e4-a52e-4f735466cecf"]}/click`, {});
    assert.ok((await command(`${root}/element/${element}/text`)).includes(text), `Selection did not change to ${text}`);
  }
  for (const category of ["all", "junction", "linear", "small_ring", "path"] as const) {
    const text = category === "path" ? "Ordinary path pairs" : category.replaceAll("_", " ");
    await choose("Turn center class", text);
    const count = points.filter((p) => turnMatches(p, category, "all")).length;
    const note = await execute("return document.querySelector('.turn-map .chart-note').textContent");
    assert.ok(note.startsWith(count.toLocaleString() + " exact observations"), `${text}: wrong filtered count`);
  }
  for (const size of ["9–15", "16–31", "≥32", "all"]) {
    await choose("Selected cycle size", size === "all" ? "All selected large cycles" : `${size} atoms`);
    const count = points.filter((p) => turnMatches(p, "path", size)).length;
    const note = await execute("return document.querySelector('.turn-map .chart-note').textContent");
    assert.ok(note.startsWith(count.toLocaleString() + " exact observations"), `${size}: wrong filtered count`);
  }
  console.log("All five center classes and four cycle-size choices passed, with exact filtered counts.");
  const target = plot.points.find((p: { id: string }) => p.id === "CNP0343194.5");
  assert.ok(target, "Crowded multi-macrocycle regression fixture is required");
  const input = await command(`${root}/element`, {
    using: "css selector",
    value: "input[aria-label='Find large-cycle molecule']",
  });
  await command(`${root}/element/${input["element-6066-11e4-a52e-4f735466cecf"]}/value`, { text: target.id });
  await until("return document.querySelectorAll('.turn-map .point-row').length === 12");
  const filtered = points.filter((p) => p.row === target.row && p.classes.every((c) => c === "path"));
  const bins = new Map<number, TurnPoint[]>();
  for (const p of filtered) {
    const key = turnCell(...p.turns);
    bins.set(key, [...(bins.get(key) ?? []), p]);
  }
  const [key, tied] = [...bins].sort((a, b) => b[1].length - a[1].length)[0];
  await execute("document.querySelector('.turn-map canvas').scrollIntoView({block:'center'})");
  const rect = await execute(
    "const r=document.querySelector('.turn-map canvas').getBoundingClientRect();return {left:r.left,top:r.top,width:r.width,height:r.height}",
  );
  const x = rect.left + 60 + (((key % 72) + 0.5) / 72) * (rect.width - 80);
  const y = rect.top + rect.height - 55 - ((Math.floor(key / 72) + 0.5) / 72) * (rect.height - 75);
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
  await until(
    "return document.querySelector('[aria-label=\"Selected large-cycle path\"]') && document.querySelector('.inspector svg polyline')",
  );
  const record: CaseRecord = await fetch(`${origin}/api/runs/${run}/records/${target.row}`).then((r) => r.json());
  const actual = await execute(
    "return {line:document.querySelector('.inspector svg polyline').getAttribute('points'),heading:document.querySelector('[aria-label=\"Selected large-cycle path\"] strong').textContent,choices:document.querySelector('.turn-map .point-search').nextElementSibling.textContent}",
  );
  const sample = tied[0];
  const geometry = record.metrics!.macrocycle_geometry!;
  const source = geometry.samples[sample.sample];
  assert.equal(actual.line, source.atoms.map((atom) => geometry.scene!.positions[atom].join(",")).join(" "));
  assert.ok(actual.heading.includes(source.atoms.join(" → ")));
  assert.ok(actual.choices.includes(`${tied.length.toLocaleString()} matching observations`));
  const href = await execute("return document.querySelector('.inspector svg image').getAttribute('href')");
  const svg = await fetch(new URL(href, origin)).then((r) => r.text());
  assert.ok(svg.includes("<svg"));
  await execute("document.querySelector('.turn-map').closest('.plot-panel').scrollIntoView({block:'start'})");
  await mkdir(cacheRoot, { recursive: true });
  await writeFile(join(cacheRoot, "turn-map-smoke.png"), Buffer.from(await command(`${root}/screenshot`), "base64"));
  const logs = await command(`${root}/log`, { type: "browser" });
  assert.deepEqual(
    logs.filter((r: { level: string; message: string }) => r.level === "SEVERE" && !r.message.includes("favicon.ico")),
    [],
  );
  console.log(
    JSON.stringify({
      samples: points.length,
      fixture: target.id,
      coincident: tied.length,
      selectedAtoms: source.atoms,
      screenshot: join(cacheRoot, "turn-map-smoke.png"),
    }),
  );
} finally {
  await fetch(`${driver}${root}`, { method: "DELETE" });
}
