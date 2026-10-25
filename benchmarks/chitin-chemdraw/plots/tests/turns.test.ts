import assert from "node:assert/strict";
import { test } from "node:test";
import { compactTurns } from "../server/corpus";
import { turnCell, turnMatches, TURN_BINS } from "../shared/turns";
import type { CaseRecord, TurnPoint } from "../shared/types";

const point: TurnPoint = { row: 1, sample: 3, turns: [12, 12], classes: ["path", "path"], cycle_size: 30, rank: 3 };
test("density cells are bounded and preserve coincident records", () => {
  assert.equal(turnCell(-180, -180), 0);
  assert.equal(turnCell(180, 180), TURN_BINS * TURN_BINS - 1);
  const bins = new Map<number, TurnPoint[]>();
  for (const row of [1, 2]) {
    const p = { ...point, row };
    const key = turnCell(...p.turns);
    bins.set(key, [...(bins.get(key) ?? []), p]);
  }
  assert.deepEqual(
    bins.get(turnCell(12, 12))?.map((p) => p.row),
    [1, 2],
  );
});
test("junction and linear strata are not ordinary path pairs", () => {
  assert.equal(turnMatches(point, "path", "16–31"), true);
  assert.equal(turnMatches({ ...point, classes: ["linear", "path"] }, "path", "all"), false);
  assert.equal(turnMatches({ ...point, classes: ["junction", "path"] }, "junction", "all"), true);
  assert.equal(turnMatches(point, "all", "≥32"), false);
});
test("historical turn samples stay absent while source sample identities survive compaction", () => {
  const record = { case: { row: 1 }, status: "success", metrics: { profile: "planar" } } as unknown as CaseRecord;
  assert.deepEqual(compactTurns(record), []);
  record.metrics!.macrocycle_geometry = {
    samples: [
      { atoms: [0, 1, 2, 3], turns: [NaN, 2], classes: ["path", "path"], cycle_size: 30, block_rank: 3 },
      { atoms: [1, 2, 3, 4], turns: [12, 12], classes: ["path", "path"], cycle_size: 30, block_rank: 3 },
    ],
    selected_large_cycles: 1,
    excluded_blocks: 0,
    unique_angles: 30,
    valid_angles: 30,
    scene: null,
  };
  assert.deepEqual(
    compactTurns(record).map((p) => p.sample),
    [1],
  );
  record.metrics!.profile = "perspective";
  assert.deepEqual(compactTurns(record), []);
});
