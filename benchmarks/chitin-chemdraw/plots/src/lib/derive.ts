import { rankGroup, type PlotData } from "../../shared/types";
import { quantile } from "./geometry";

export interface Curve {
  group: number;
  n: number;
  coordinates: [number, number][];
}
export interface Box {
  group: number;
  n: number;
  min: number;
  q1: number;
  median: number;
  q3: number;
  p95: number;
  max: number;
  outliers: number[];
}
export interface Derived {
  curves: Curve[];
  boxes: Box[];
}

/** Exact empirical survival. Repeated values are grouped, never jittered or sampled. */
export function derive(data: PlotData): Derived {
  const curves: Curve[] = [];
  const boxes: Box[] = [];
  for (let group = 0; group < 4; group++) {
    const points = data.points.filter((point) => rankGroup(point.rank) === group);
    const lengths = points.flatMap((point) => (point.length === null ? [] : [point.length])).sort((a, b) => a - b);
    const coordinates: [number, number][] = [];
    for (let i = 0; i < lengths.length; i++) {
      if (i === 0 || lengths[i] !== lengths[i - 1])
        coordinates.push([lengths[i], (lengths.length - i) / lengths.length]);
    }
    curves.push({ group, n: lengths.length, coordinates });
    const angles = points.flatMap((point) => (point.angle === null ? [] : [point.angle])).sort((a, b) => a - b);
    if (angles.length) {
      const q1 = quantile(angles, 0.25),
        q3 = quantile(angles, 0.75);
      const inside = angles.filter((value) => value >= q1 - 1.5 * (q3 - q1) && value <= q3 + 1.5 * (q3 - q1));
      boxes.push({
        group,
        n: angles.length,
        q1,
        q3,
        median: quantile(angles, 0.5),
        p95: quantile(angles, 0.95),
        min: inside[0] ?? angles[0],
        max: inside.at(-1) ?? angles.at(-1)!,
        outliers: angles.filter((value) => value < q1 - 1.5 * (q3 - q1) || value > q3 + 1.5 * (q3 - q1)),
      });
    }
  }
  return { curves, boxes };
}
