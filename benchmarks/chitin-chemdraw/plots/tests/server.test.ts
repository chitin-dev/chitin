import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, isAbsolute } from "node:path";
import { committedLines } from "../server/corpus";
import { runDirectory, rowNumber, plotsRoot } from "../server/paths";
import { RenderQueue, sourceRevision, watchSources } from "../server/renderer";

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
