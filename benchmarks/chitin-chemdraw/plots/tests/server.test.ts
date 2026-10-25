import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, isAbsolute } from "node:path";
import { committedLines, compactPoint } from "../server/corpus";
import type { CaseRecord } from "../shared/types";
import { runDirectory, rowNumber, plotsRoot } from "../server/paths";
import { RenderQueue, sourceRevision, watchSources, renderSvg } from "../server/renderer";
import type { Corpus } from "../server/corpus";

test("external layout runs cannot silently substitute the current internal generator", async () => {
  const corpus = {
    offsets: new Map([[1, { offset: 0, length: 0 }]]),
    manifest: { layout_engine: "rdkit_coordgen" },
  } as unknown as Corpus;
  await assert.rejects(renderSvg(corpus, 1, "current"), /snapshots only/);
});

test("JSONL offsets use UTF8 bytes and ignore uncommitted trailing records", async () => {
  const directory = await mkdtemp(join(tmpdir(), "chitin-plots-test-"));
  try {
    const path = join(directory, "cases.jsonl");
    const first = '{"id":"分子"}\n';
    const bytes = Buffer.from(first + '{"id":"second"}\n' + '{"id":');
    await writeFile(path, bytes);
    const records = [];
    for await (const line of committedLines(path, bytes.length)) records.push(line);
    assert.equal(records.length, 2);
    assert.equal(records[1].offset, Buffer.byteLength(first));
    assert.equal(JSON.parse(records[0].bytes.toString()).id, "分子");
    assert.equal(JSON.parse(records[1].bytes.toString()).id, "second");
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("artifact paths reject traversal and malformed rows", () => {
  for (const id of ["../secret", "a/b", "%2e%2e", "a..b", ""]) assert.throws(() => runDirectory(id));
  for (const row of ["0", "-1", "01", "1.svg", "9007199254740992"]) assert.throws(() => rowNumber(row));
  assert.equal(rowNumber("123"), 123);
  assert.ok(isAbsolute(plotsRoot));
  assert.ok(isAbsolute(runDirectory("coconut-10k")));
});

test("legacy measurements stay unknown while v2 measured zeroes stay numeric", () => {
  const legacy = {
    case: { row: 1, id: "test", smiles: "CCC" },
    status: "success",
    features: { atoms: 3, maximum_block_rank: 0 },
    metrics: { profile: "planar", measurements: {} },
  } as unknown as CaseRecord;
  assert.equal(compactPoint(legacy)?.footprint, null);
  assert.equal(compactPoint(legacy)?.cavity, null);
  legacy.metrics!.measurements.normalized_footprint = {
    state: "measured",
    value: 0,
    eligible: 3,
    violations: null,
    reason: null,
  };
  assert.equal(compactPoint(legacy)?.footprint, 0);
  legacy.metrics!.measurements.maximum_ring_cavity_radius = {
    state: "not_implemented",
    value: null,
    eligible: 2,
    violations: null,
    reason: "search budget",
  };
  assert.equal(compactPoint(legacy)?.cavity, null);
});

test("source edits invalidate preview revisions without rewriting historical results", async () => {
  const directory = await mkdtemp(join(tmpdir(), "chitin-plots-watch-"));
  const original = sourceRevision();
  let notify!: () => void;
  const changed = new Promise<void>((resolve) => {
    notify = resolve;
  });
  const stop = watchSources(notify, [directory]);
  try {
    await writeFile(join(directory, "layout.rs"), "// watcher fixture\n");
    await Promise.race([
      changed,
      new Promise<never>((_, reject) => {
        const timer = setTimeout(() => reject(new Error("Source event missing")), 3000);
        timer.unref();
      }),
    ]);
    assert.notEqual(sourceRevision(), original);
  } finally {
    stop();
    await rm(directory, { recursive: true, force: true });
  }
});

test("preview jobs deduplicate and transfer only two reserved concurrency slots", async () => {
  const queue = new RenderQueue();
  let active = 0,
    maximum = 0,
    calls = 0;
  const action = async () => {
    calls++;
    active++;
    maximum = Math.max(maximum, active);
    await new Promise<void>((resolve) => setImmediate(resolve));
    active--;
  };
  const first = queue.run("same", action);
  await Promise.all([
    first,
    queue.run("same", action),
    ...Array.from({ length: 8 }, (_, i) => queue.run(String(i), action)),
  ]);
  assert.equal(calls, 9);
  assert.equal(maximum, 2);
  await assert.rejects(
    queue.run("error", async () => {
      throw new Error("worker failed");
    }),
  );
  await queue.run("after-error", action);
});
