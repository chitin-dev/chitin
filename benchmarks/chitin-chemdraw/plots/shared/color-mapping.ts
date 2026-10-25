import type { PlotPoint, TurnPoint } from "./types";

/** Independent color channels never change plotted coordinates or point eligibility. */
export const COLOR_METRICS = {
  contacts: { label: "Nonbonded atom contacts", log: true },
  atomEdgeContacts: { label: "Atom–edge contacts", log: true },
  edgeContacts: { label: "Edge contacts", log: true },
  labelBondContacts: { label: "Label–bond contacts", log: true },
  labelContacts: { label: "Label–label contacts", log: true },
  cycleSize: { label: "Cycle size", log: false },
  macrocycleAngle: { label: "Macrocycle angle RMSE (°)", log: false },
  cavity: { label: "Maximum cavity radius / L", log: false },
  footprint: { label: "Normalized footprint", log: false },
} as const;
export type ColorMetric = keyof typeof COLOR_METRICS;
export const COLOR_KEYS = Object.keys(COLOR_METRICS) as ColorMetric[];
export const MISSING_COLOR = "#a4adb8";
export const COLOR_STEPS = 32;

/** Turns carry their own cycle size; molecule-level quantities are joined by source row. */
export function colorValue(point: PlotPoint | undefined, metric: ColorMetric, turn?: TurnPoint): number | null {
  const value = metric === "cycleSize" && turn ? turn.cycle_size : point?.[metric];
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}

export interface ColorScale {
  min: number;
  max: number;
  log: boolean;
  measured: number;
  missing: number;
}

/** Domains use the filtered dataset, not the viewport, so zooming does not recolor data. */
export function colorScale(values: Iterable<number | null>, log: boolean): ColorScale {
  let min = Infinity,
    max = -Infinity,
    measured = 0,
    missing = 0;
  for (const value of values) {
    if (value === null || !Number.isFinite(value) || value < 0) {
      missing++;
      continue;
    }
    min = Math.min(min, value);
    max = Math.max(max, value);
    measured++;
  }
  return { min: measured ? min : 0, max: measured ? max : 0, log, measured, missing };
}

export function colorFraction(value: number, scale: ColorScale): number {
  if (scale.max <= scale.min) return 0.5;
  const transform = (n: number) => (scale.log ? Math.log1p(n) : n);
  return Math.max(
    0,
    Math.min(1, (transform(value) - transform(scale.min)) / (transform(scale.max) - transform(scale.min))),
  );
}

/** Bounded palette permits batched canvas fills even for very large point clouds. */
export function colorBucket(value: number | null, scale: ColorScale): number {
  return value === null
    ? COLOR_STEPS
    : Math.min(COLOR_STEPS - 1, Math.floor(colorFraction(value, scale) * COLOR_STEPS));
}

export function palette(fraction: number): string {
  const stops = [
    [68, 1, 84],
    [59, 82, 139],
    [33, 145, 140],
    [94, 201, 98],
    [253, 231, 37],
  ];
  const position = Math.max(0, Math.min(1, fraction)) * (stops.length - 1);
  const index = Math.min(stops.length - 2, Math.floor(position));
  const mix = position - index;
  return `rgb(${stops[index].map((v, channel) => Math.round(v + (stops[index + 1][channel] - v) * mix)).join(",")})`;
}

export function bucketColor(bucket: number): string {
  return bucket === COLOR_STEPS ? MISSING_COLOR : palette(bucket / (COLOR_STEPS - 1));
}

/** Missing observations never become zeros or enter aggregate denominators. */
export function aggregateColor(values: Iterable<number | null>, mode: "mean" | "max"): number | null {
  let sum = 0,
    count = 0,
    maximum = -Infinity;
  for (const value of values) {
    if (value === null || !Number.isFinite(value) || value < 0) continue;
    sum += value;
    maximum = Math.max(maximum, value);
    count++;
  }
  return count ? (mode === "mean" ? sum / count : maximum) : null;
}
