import assert from "node:assert/strict";
import { test } from "node:test";
import { SpatialGrid, symlog, inverseSymlog, quantile } from "../src/lib/geometry";
import { derive } from "../src/lib/derive";
import type { PlotData, PlotPoint } from "../shared/types";

test("every coincident record remains selectable, with deterministic nearest ties", () => {
  const grid = new SpatialGrid();
  for (let index = 9999; index >= 0; index--) grid.add({ index, x: 12, y: 12 });
  grid.add({ index: 10000, x: 21, y: 12 });
  assert.deepEqual(
    grid.nearest(12, 12),
    Array.from({ length: 10000 }, (_, index) => index),
  );
  assert.deepEqual(grid.inspect(12, 12), { index: 0, count: 10000 });
  assert.deepEqual(grid.inspect(-20, -20), { index: undefined, count: 0 });
});

test("cell boundaries and negative coordinates do not lose hits", () => {
  const grid = new SpatialGrid();
  grid.add({ index: 3, x: -1, y: -1 });
  grid.add({ index: 2, x: 1, y: 1 });
  grid.add({ index: 1, x: 0, y: 0 });
  assert.deepEqual(grid.nearest(0, 0, 2), [1, 2, 3]);
  assert.equal(grid.inspect(0, 0, 2).count, 3);
});

test("display transforms preserve zero and invert positive and negative values", () => {
  for (const value of [-100, -0.001, 0, 0.001, 100]) assert.ok(Math.abs(inverseSymlog(symlog(value)) - value) < 1e-10);
  assert.equal(quantile([0, 10], 0.25), 2.5);
});

test("survival includes exact zeros and ties without treating unmeasured values as zero", () => {
  const points: PlotPoint[] = [0, 0, 0.2, null].map((length, i) => ({
    row: i + 1,
    id: String(i),
    atoms: 10,
    rank: 0,
    contacts: 0,
    length,
    angle: i === 3 ? null : i,
    profile: "planar",
  }));
  const { curves, boxes } = derive({ points } as PlotData);
  assert.equal(curves[0].n, 3);
  assert.deepEqual(curves[0].coordinates, [
    [0, 1],
    [0.2, 1 / 3],
  ]);
  assert.equal(boxes[0].median, 1);
  assert.equal(boxes[0].n, 3);
  assert.equal(curves[3].n, 0);
});
