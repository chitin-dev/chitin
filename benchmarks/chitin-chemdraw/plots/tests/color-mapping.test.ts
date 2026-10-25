import assert from "node:assert/strict";
import { test } from "node:test";
import {
  aggregateColor,
  colorBucket,
  colorFraction,
  colorScale,
  colorValue,
  COLOR_STEPS,
  bucketColor,
  MISSING_COLOR,
} from "../shared/color-mapping";
import { compactPoint } from "../server/corpus";
import type { CaseRecord, PlotPoint, TurnPoint } from "../shared/types";

test("unknown values stay unknown while true zero contacts remain measured", () => {
  assert.equal(colorValue({ contacts: 0 } as PlotPoint, "contacts"), 0);
  assert.equal(colorValue({} as PlotPoint, "contacts"), null);
  assert.equal(colorValue({ contacts: NaN } as PlotPoint, "contacts"), null);
  const scale = colorScale([null, 0, 10, 100], true);
  assert.deepEqual([scale.measured, scale.missing], [3, 1]);
  assert.equal(colorFraction(0, scale), 0);
  assert.equal(colorFraction(100, scale), 1);
  assert.ok(colorFraction(10, scale) > 0.5);
  assert.equal(colorBucket(null, scale), COLOR_STEPS);
  assert.equal(bucketColor(COLOR_STEPS), MISSING_COLOR);
});

test("constant and absent color domains remain finite", () => {
  for (const values of [[], [null], [0, 0], [12, 12]]) {
    const scale = colorScale(values, false);
    assert.ok(Number.isFinite(colorFraction(scale.min, scale)));
    assert.ok(Number.isFinite(scale.min) && Number.isFinite(scale.max));
  }
});

test("cell mean and maximum are distinct and exclude missing measurements", () => {
  assert.equal(aggregateColor([0, null, 10], "mean"), 5);
  assert.equal(aggregateColor([0, null, 10], "max"), 10);
  assert.equal(aggregateColor([null, NaN], "mean"), null);
});

test("turn color uses its own selected cycle rather than another cycle in the molecule", () => {
  const molecule = { cycleSize: 36, contacts: 7 } as PlotPoint;
  const turn = { cycle_size: 12 } as TurnPoint;
  assert.equal(colorValue(molecule, "cycleSize", turn), 12);
  assert.equal(colorValue(molecule, "cycleSize"), 36);
  assert.equal(colorValue(molecule, "contacts", turn), 7);
});

test("server compaction exposes independent counts without fabricating unavailable data", () => {
  const observation = (violations: number | null, state = "measured") => ({ state, violations, value: 0 });
  const record = {
    case: { row: 1, id: "test" },
    status: "success",
    features: { atoms: 10, maximum_block_rank: 2 },
    metrics: {
      profile: "planar",
      measurements: {
        nonbonded_atom_proximity: observation(0),
        atom_edge_proximity: observation(3),
        label_bond_overlap: observation(null, "not_implemented"),
      },
      macrocycle_geometry: { samples: [{ cycle_size: 12 }, { cycle_size: 30 }] },
    },
  } as unknown as CaseRecord;
  const point = compactPoint(record)!;
  assert.equal(point.contacts, 0);
  assert.equal(point.atomEdgeContacts, 3);
  assert.equal(point.labelBondContacts, null);
  assert.equal(point.edgeContacts, null);
  assert.equal(point.cycleSize, 30);
});
