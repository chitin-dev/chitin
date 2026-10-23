/** Streaming byte-offset index: full metric objects/SMILES are fetched only on selection. */
import { createReadStream } from "node:fs";
import { open, readFile, readdir, stat } from "node:fs/promises";
import { join } from "node:path";
import type { CaseRecord, Manifest, PlotData, PlotPoint, RunInfo, TurnPoint } from "../shared/types";
import { resultsRoot, runDirectory } from "./paths";

interface Offset {
  offset: number;
  length: number;
}
export interface Corpus {
  data: PlotData;
  offsets: Map<number, Offset>;
  manifest: Manifest;
  directory: string;
  turns: TurnPoint[];
}
const cache = new Map<string, Promise<Corpus>>();

/** In-progress checkpoints can be between writes; skip them and retry next poll. */
export async function listRuns(): Promise<RunInfo[]> {
  const entries = await readdir(resultsRoot, { withFileTypes: true }).catch(() => []);
  const runs = await Promise.all(
    entries
      .filter((entry) => entry.isDirectory() && !entry.isSymbolicLink())
      .map(async (entry) => {
        try {
          const directory = runDirectory(entry.name);
          const [manifest, summary, metadata, cases] = await Promise.all([
            readFile(join(directory, "manifest.json"), "utf8").then(JSON.parse),
            readFile(join(directory, "summary.json"), "utf8").then(JSON.parse),
            stat(join(directory, "summary.json")),
            stat(join(directory, "cases.jsonl")),
          ]);
          if (!["diagram-quality-v1", "diagram-quality-v2", "diagram-quality-v3"].includes(summary.quality_version))
            return null;
          return {
            id: entry.name,
            updated: metadata.mtimeMs,
            revision: `${cases.mtimeMs}-${cases.size}`,
            dataset: manifest.config.dataset,
            version: manifest.config.version,
            cases: summary.cases,
            evaluated: summary.success,
            protocol: summary.quality_version,
          } satisfies RunInfo;
        } catch {
          return null;
        }
      }),
  );
  return runs.filter((run): run is RunInfo => run !== null).sort((a, b) => b.updated - a.updated);
}

/** Parse only newline-committed JSONL bytes from a bounded file snapshot. */
export async function* committedLines(path: string, size: number): AsyncGenerator<{ offset: number; bytes: Buffer }> {
  if (size === 0) return;
  let pending = Buffer.alloc(0);
  let offset = 0;
  for await (const chunk of createReadStream(path, { end: size - 1 })) {
    pending = pending.length ? Buffer.concat([pending, chunk]) : chunk;
    let consumed = 0;
    for (let end = pending.indexOf(10, consumed); end !== -1; end = pending.indexOf(10, consumed)) {
      const bytes = pending.subarray(consumed, end);
      if (bytes.length) yield { offset: offset + consumed, bytes };
      consumed = end + 1;
    }
    offset += consumed;
    pending = pending.subarray(consumed);
    if (pending.length > 4 * 1024 * 1024) throw new Error("JSONL record exceeds the 4 MiB safety limit");
  }
  // A killed or still-running writer may leave a partial final record.
}

export async function getCorpus(id: string): Promise<Corpus> {
  const run = (await listRuns()).find((run) => run.id === id);
  if (!run) throw new Error("Run not found or unsupported quality protocol");
  const key = `${id}:${run.revision}`;
  const existing = cache.get(key);
  if (existing) {
    cache.delete(key);
    cache.set(key, existing);
    return existing;
  }
  const loading = indexRun(run).catch((error) => {
    cache.delete(key);
    throw error;
  });
  cache.set(key, loading);
  // Keep only two compact run indexes, not multiple copies of entire JSONL corpora.
  while (cache.size > 2) cache.delete(cache.keys().next().value!);
  return loading;
}

async function indexRun(run: RunInfo): Promise<Corpus> {
  const directory = runDirectory(run.id);
  const path = join(directory, "cases.jsonl");
  const manifest: Manifest = JSON.parse(await readFile(join(directory, "manifest.json"), "utf8"));
  const metadata = await stat(path);
  const offsets = new Map<number, Offset>();
  const data: PlotData = { run, revision: run.revision, points: [], metrics: {}, outcomes: {}, completed: 0, clear: 0 };
  const turns: TurnPoint[] = [];
  for await (const { offset, bytes } of committedLines(path, metadata.size)) {
    const record: CaseRecord = JSON.parse(bytes.toString("utf8"));
    if (!Number.isSafeInteger(record.case.row) || offsets.has(record.case.row))
      throw new Error("Invalid or duplicate source row");
    offsets.set(record.case.row, { offset, length: bytes.length });
    const outcome = `${record.status}:${record.stage}`;
    data.outcomes[outcome] = (data.outcomes[outcome] ?? 0) + 1;
    if (record.status !== "success" || !record.metrics || !record.features) continue;
    if (record.metrics.version !== run.protocol) throw new Error("Mixed metric protocols in one run");
    data.completed++;
    data.clear += Number(record.metrics.readability_clear === true);
    const observations = record.metrics.measurements;
    const point = compactPoint(record);
    if (point) data.points.push(point);
    turns.push(...compactTurns(record));
    for (const [key, observation] of Object.entries(observations)) {
      const metric = (data.metrics[key] ??= {
        measured_cases: 0,
        affected_cases: null,
        not_applicable_cases: 0,
        not_implemented_cases: 0,
      });
      if (observation.state === "not_applicable") metric.not_applicable_cases++;
      else if (observation.state === "not_implemented") metric.not_implemented_cases++;
      else {
        metric.measured_cases++;
        if (observation.violations !== null)
          metric.affected_cases = (metric.affected_cases ?? 0) + Number(observation.violations > 0);
      }
    }
  }
  data.run = { ...run, cases: offsets.size, evaluated: data.completed };
  return { data, offsets, manifest, directory, turns };
}

/** Retain exact per-record sample indices so picking resolves the historical path. */
export function compactTurns(record: CaseRecord): TurnPoint[] {
  if (record.status !== "success" || record.metrics?.profile !== "planar") return [];
  return (record.metrics.macrocycle_geometry?.samples ?? []).flatMap((point, sample) =>
    point.turns.every((value) => Number.isFinite(value) && Math.abs(value) <= 180)
      ? [
          {
            row: record.case.row,
            sample,
            turns: point.turns,
            classes: point.classes,
            cycle_size: point.cycle_size,
            rank: point.block_rank,
          },
        ]
      : [],
  );
}

/** Historical records keep absent new measurements null, not fabricated zeroes. */
export function compactPoint(record: CaseRecord): PlotPoint | null {
  if (record.status !== "success" || !record.metrics || !record.features) return null;
  const observations = record.metrics.measurements;
  const value = (key: string) => (observations[key]?.state === "measured" ? observations[key].value : null);
  const contacts = observations.nonbonded_atom_proximity;
  const count = (key: string) => (observations[key]?.state === "measured" ? observations[key].violations : null);
  const cycles = record.metrics.macrocycle_geometry?.samples ?? [];
  return {
    row: record.case.row,
    id: record.case.id,
    atoms: record.features.atoms,
    rank: record.features.maximum_block_rank,
    contacts: contacts?.state === "measured" ? contacts.violations : null,
    length: value("relative_bond_length_rmse"),
    angle: value("acyclic_carbon_angle_rmse_deg"),
    profile: record.metrics.profile,
    cavity: value("maximum_ring_cavity_radius"),
    roundness: value("ring_isoperimetric_ratio"),
    footprint: value("normalized_footprint"),
    atomEdgeContacts: count("atom_edge_proximity"),
    edgeContacts: count("unannotated_edge_contact"),
    labelBondContacts: count("label_bond_overlap"),
    labelContacts: count("label_label_overlap"),
    macrocycleAngle: value("macrocycle_angle_deviation_rmse_deg"),
    cycleSize: cycles.length ? Math.max(...cycles.map((sample) => sample.cycle_size)) : null,
  };
}

/** Exact byte reads avoid retaining every molecule's full annotation tree in RAM. */
export async function readRecord(corpus: Corpus, row: number): Promise<CaseRecord> {
  const location = corpus.offsets.get(row);
  if (!location) throw new Error("Source row does not exist in this run");
  const handle = await open(join(corpus.directory, "cases.jsonl"), "r");
  try {
    const buffer = Buffer.alloc(location.length);
    const { bytesRead } = await handle.read(buffer, 0, buffer.length, location.offset);
    if (bytesRead !== buffer.length) throw new Error("Run changed while reading; refresh the selection");
    const record = JSON.parse(buffer.toString("utf8"));
    if (record.case.row !== row) throw new Error("Run index is stale; refresh the selection");
    return record;
  } finally {
    await handle.close();
  }
}
