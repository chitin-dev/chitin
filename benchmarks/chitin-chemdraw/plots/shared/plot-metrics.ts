import type { PlotPoint } from "./types";

/** Shared selectors keep canvas eligibility, search and point labels on the same domain. */
export type ScatterMetric = "contacts" | "cavity" | "roundness" | "footprint";
export const SCATTER_METRICS = {
  contacts: {
    label: "Atom proximity",
    axis: "Close nonbonded atom pairs · symlog",
    unit: "pairs",
    linear: false,
    ticks: [0, 1, 2, 5, 10, 20, 50, 100, 200, 500, 1000],
  },
  cavity: {
    label: "Maximum ring cavity radius",
    axis: "Maximum cavity radius / L · symlog",
    unit: "L radius",
    linear: false,
    ticks: [0, 0.1, 0.2, 0.5, 1, 2, 5, 10, 20, 50, 100],
  },
  roundness: {
    label: "Ring isoperimetric ratio",
    axis: "Maximum ring Q = 4πA / P² · linear",
    unit: "Q",
    linear: true,
    ticks: [0, 0.2, 0.4, 0.6, 0.8, 1],
  },
  footprint: {
    label: "Normalized footprint",
    axis: "Component hull area / (N L²) · symlog",
    unit: "area / atom",
    linear: false,
    ticks: [0, 0.1, 0.2, 0.5, 1, 2, 5, 10, 20, 50, 100],
  },
} as const;

/** Missing historical fields are unknown, while genuinely measured zeroes stay selectable. */
export function scatterValue(point: PlotPoint, metric: ScatterMetric): number | null {
  const value = point[metric];
  return typeof value === "number" && Number.isFinite(value) && value >= 0 ? value : null;
}
export function scatterLabel(point: PlotPoint, metric: ScatterMetric): string {
  const value = scatterValue(point, metric);
  return value === null
    ? "not measured"
    : `${metric === "contacts" ? value : Number(value.toPrecision(4))} ${SCATTER_METRICS[metric].unit}`;
}

/** Bare integers address exact source rows; identifier fragments retain substring search. */
export function matchesPointQuery(point: PlotPoint, query: string): boolean {
  const normalized = query.trim().toLowerCase();
  return (
    !normalized ||
    (/^[0-9]+$/.test(normalized) ? String(point.row) === normalized : point.id.toLowerCase().includes(normalized))
  );
}
