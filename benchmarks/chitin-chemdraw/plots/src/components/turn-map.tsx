import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { rankGroup, type CenterClass, type PlotPoint, type TurnPoint, type TurnSelection } from "../../shared/types";
import { TURN_BINS, turnCell, turnMatches } from "../../shared/turns";
import { Button } from "./ui/button";
import { SelectField } from "./ui/select";
import { ColorLegend } from "./color-legend";
import {
  COLOR_KEYS,
  COLOR_METRICS,
  MISSING_COLOR,
  aggregateColor,
  colorFraction,
  colorScale,
  colorValue,
  palette,
  type ColorMetric,
} from "../../shared/color-mapping";

/** Fixed-size canvas density drawing; every coincident sample remains selectable. */
export function TurnMap(props: {
  points: TurnPoint[];
  molecules: PlotPoint[];
  groups: boolean[];
  historical: boolean;
  selected: TurnSelection | null;
  onSelect: (selection: TurnSelection) => void;
}) {
  const [category, setCategory] = createSignal<CenterClass | "all">("path");
  const [size, setSize] = createSignal("all");
  const [color, setColor] = createSignal<ColorMetric | "density">("density");
  const [aggregate, setAggregate] = createSignal<"mean" | "max">("mean");
  const [query, setQuery] = createSignal("");
  const [cell, setCell] = createSignal<number | null>(null);
  const [browse, setBrowse] = createSignal(false);
  const [page, setPage] = createSignal(0);
  const molecules = createMemo(() => new Map(props.molecules.map((p) => [p.row, p])));
  const filtered = createMemo(() => {
    const q = query().trim().toLowerCase();
    return props.points.filter(
      (p) =>
        props.groups[rankGroup(molecules().get(p.row)?.rank ?? p.rank)] &&
        turnMatches(p, category(), size()) &&
        (!q || (/^\d+$/.test(q) ? String(p.row) === q : molecules().get(p.row)?.id.toLowerCase().includes(q))),
    );
  });
  const bins = createMemo(() => {
    const result = new Map<number, TurnPoint[]>();
    for (const point of filtered()) {
      const key = turnCell(...point.turns);
      const bin = result.get(key);
      if (bin) bin.push(point);
      else result.set(key, [point]);
    }
    return result;
  });
  const values = createMemo(() => {
    const result = new Map<number, number | null>();
    if (color() === "density") return result;
    for (const [key, points] of bins()) {
      result.set(
        key,
        aggregateColor(
          points.map((point) => colorValue(molecules().get(point.row), color() as ColorMetric, point)),
          aggregate(),
        ),
      );
    }
    return result;
  });
  const scale = createMemo(() =>
    colorScale(
      filtered().map((point) =>
        color() === "density" ? null : colorValue(molecules().get(point.row), color() as ColorMetric, point),
      ),
      color() !== "density" && COLOR_METRICS[color() as ColorMetric].log,
    ),
  );
  const choices = createMemo(() =>
    browse() ? filtered() : cell() !== null ? (bins().get(cell()!) ?? []) : query().trim() ? filtered() : [],
  );
  createEffect(() => {
    query();
    category();
    size();
    props.groups;
    setCell(null);
  });
  createEffect(() => {
    filtered();
    cell();
    browse();
    setPage(0);
  });
  createEffect(() => {
    props.points;
    setCell(null);
    setBrowse(false);
    setQuery("");
  });
  let host!: HTMLDivElement, canvas!: HTMLCanvasElement;
  let observer: ResizeObserver | undefined,
    frame = 0;
  let width = 600;
  const height = 430;
  const margin = { left: 60, right: 20, top: 20, bottom: 55 };
  const plotWidth = () => Math.max(1, width - margin.left - margin.right);
  const plotHeight = () => height - margin.top - margin.bottom;
  const x = (value: number) => margin.left + ((value + 180) / 360) * plotWidth();
  const y = (value: number) => height - margin.bottom - ((value + 180) / 360) * plotHeight();
  function paint() {
    frame = 0;
    const context = canvas?.getContext("2d");
    if (!context) return;
    const ratio = Math.min(devicePixelRatio || 1, 2);
    canvas.width = Math.round(width * ratio);
    canvas.height = height * ratio;
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, width, height);
    context.fillStyle = "#f5f7fa";
    context.fillRect(x(-180), y(180), plotWidth(), plotHeight());
    const maximum = Math.max(1, ...[...bins().values()].map((v) => v.length));
    for (const [key, points] of bins()) {
      const col = key % TURN_BINS,
        row = Math.floor(key / TURN_BINS);
      const alpha = 0.15 + (0.85 * Math.log1p(points.length)) / Math.log1p(maximum);
      const value = values().get(key);
      context.fillStyle =
        color() === "density"
          ? `rgba(37,99,164,${alpha})`
          : value == null
            ? MISSING_COLOR
            : palette(colorFraction(value, scale()));
      context.fillRect(
        x(col * 5 - 180),
        y((row + 1) * 5 - 180),
        plotWidth() / TURN_BINS + 0.2,
        plotHeight() / TURN_BINS + 0.2,
      );
    }
    context.font = "11px system-ui";
    context.textAlign = "center";
    for (const tick of [-180, -120, -60, 0, 60, 120, 180]) {
      context.beginPath();
      context.strokeStyle = tick === 0 ? "#94a3b8" : "#dce3eb";
      context.moveTo(x(tick), y(-180));
      context.lineTo(x(tick), y(180));
      context.moveTo(x(-180), y(tick));
      context.lineTo(x(180), y(tick));
      context.stroke();
      context.fillStyle = "#475569";
      context.textAlign = "center";
      context.fillText(`${tick}°`, x(tick), height - margin.bottom + 20);
      context.textAlign = "right";
      context.fillText(`${tick}°`, margin.left - 9, y(tick) + 4);
    }
    const selected = props.selected;
    const point = selected && filtered().find((p) => p.row === selected.row && p.sample === selected.sample);
    if (point) {
      context.beginPath();
      context.arc(x(point.turns[0]), y(point.turns[1]), 6, 0, Math.PI * 2);
      context.lineWidth = 2;
      context.strokeStyle = "#f07820";
      context.stroke();
    }
    context.fillStyle = "#334155";
    context.textAlign = "center";
    context.fillText("Signed turn τᵢ", margin.left + plotWidth() / 2, height - 10);
    context.save();
    context.translate(15, margin.top + plotHeight() / 2);
    context.rotate(-Math.PI / 2);
    context.fillText("Signed turn τᵢ₊₁", 0, 0);
    context.restore();
  }
  function schedule() {
    if (!frame) frame = requestAnimationFrame(paint);
  }
  createEffect(() => {
    bins();
    values();
    scale();
    color();
    props.selected;
    schedule();
  });
  onMount(() => {
    observer = new ResizeObserver((entries) => {
      width = Math.max(240, entries[0].contentRect.width);
      schedule();
    });
    observer.observe(host);
    schedule();
  });
  onCleanup(() => {
    observer?.disconnect();
    cancelAnimationFrame(frame);
  });
  function pick(event: MouseEvent) {
    const rect = canvas.getBoundingClientRect();
    const px = event.clientX - rect.left,
      py = event.clientY - rect.top;
    if (px < margin.left || px > width - margin.right || py < margin.top || py > height - margin.bottom) return;
    const key = turnCell(
      ((px - margin.left) / plotWidth()) * 360 - 180,
      ((height - margin.bottom - py) / plotHeight()) * 360 - 180,
    );
    setCell(key);
    setBrowse(false);
    const first = bins().get(key)?.[0];
    if (first) props.onSelect({ row: first.row, sample: first.sample });
  }
  return (
    <div class="turn-map">
      <div class="turn-controls">
        <SelectField
          label="Turn center class"
          options={["path", "junction", "linear", "small_ring", "all"] as const}
          value={category()}
          onChange={setCategory}
          text={(v) => (v === "path" ? "Ordinary path pairs" : v.replaceAll("_", " "))}
        />
        <SelectField
          label="Selected cycle size"
          options={["all", "9–15", "16–31", "≥32"]}
          value={size()}
          onChange={setSize}
          text={(v) => (v === "all" ? "All selected large cycles" : `${v} atoms`)}
        />
      </div>
      <div class="color-controls">
        <SelectField
          label="Color turns by"
          options={["density", ...COLOR_KEYS] as (ColorMetric | "density")[]}
          value={color()}
          onChange={setColor}
          text={(value) => (value === "density" ? "Color: sample density" : `Color: ${COLOR_METRICS[value].label}`)}
        />
        <Show when={color() !== "density"}>
          <SelectField
            label="Turn cell aggregation"
            options={["mean", "max"] as const}
            value={aggregate()}
            onChange={setAggregate}
            text={(value) => (value === "mean" ? "Cell mean" : "Cell maximum")}
          />
        </Show>
      </div>
      <Show when={color() !== "density"}>
        <ColorLegend
          label={color() === "cycleSize" ? "Cycle size (atoms)" : COLOR_METRICS[color() as ColorMetric].label}
          scale={scale()}
          note={aggregate() === "mean" ? "Cell mean · sample-weighted" : "Cell maximum"}
        />
      </Show>
      <div ref={host} class="turn-canvas-host" hidden={!props.points.length}>
        <canvas
          ref={canvas}
          style={{ width: "100%", height: `${height}px` }}
          aria-label="Large-cycle adjacent signed-turn density map"
          onClick={pick}
        />
      </div>
      <Show
        when={props.points.length}
        fallback={
          <p class="metric-empty">{props.historical ? "Not recorded. Rerun benchmark." : "No valid turn pairs."}</p>
        }
      >
        <p class="chart-note">
          {filtered().length.toLocaleString()} pairs · 5° bins ·{" "}
          {color() === "density" ? "log count" : color() === "cycleSize" ? "per-cycle values" : "molecule-level values"}{" "}
          · Click to inspect
        </p>
        <Show when={color() !== "density" && cell() !== null}>
          <p class="chart-note">
            Selected cell {aggregate()}: {values().get(cell()!)?.toPrecision(4) ?? "not measured"}
          </p>
        </Show>
        <div class="point-search">
          <input
            class="input"
            aria-label="Find large-cycle molecule"
            placeholder="Find CNP ID or source row…"
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
          />
          <Button variant="outline" size="sm" onClick={() => setBrowse((v) => !v)}>
            {browse() ? "Hide turn list" : "Browse all turns"}
          </Button>
        </div>
        <Show when={browse() || query().trim() || cell() !== null}>
          <p class="chart-note">{choices().length.toLocaleString()} matches</p>
          <div class="point-list">
            <For each={choices().slice(page() * 12, page() * 12 + 12)}>
              {(point) => (
                <button
                  class="point-row"
                  classList={{ selected: props.selected?.row === point.row && props.selected?.sample === point.sample }}
                  onClick={() => props.onSelect({ row: point.row, sample: point.sample })}
                >
                  <strong>{molecules().get(point.row)?.id ?? point.row}</strong>
                  <span>path {point.sample + 1}</span>
                  <span>
                    {point.turns.map((v) => v.toFixed(1) + "°").join(" / ")} · {point.cycle_size} atoms
                  </span>
                </button>
              )}
            </For>
          </div>
          <div class="point-pagination">
            <Button variant="ghost" size="sm" disabled={page() === 0} onClick={() => setPage((v) => v - 1)}>
              Previous turns
            </Button>
            <span>
              {choices().length ? page() + 1 : 0} / {Math.ceil(choices().length / 12)}
            </span>
            <Button
              variant="ghost"
              size="sm"
              disabled={(page() + 1) * 12 >= choices().length}
              onClick={() => setPage((v) => v + 1)}
            >
              Next turns
            </Button>
          </div>
        </Show>
      </Show>
    </div>
  );
}
