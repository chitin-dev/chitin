/** Lazy SVG rendering with immutable binary identity, bounded concurrency and timeouts. */
import { createHash } from "node:crypto";
import { createReadStream, constants, statSync, watch, type FSWatcher } from "node:fs";
import { chmod, copyFile, mkdir, rename, stat, unlink } from "node:fs/promises";
import { join } from "node:path";
import { execFile, spawn } from "node:child_process";
import { promisify } from "node:util";
import { cacheRoot, plotsRoot, workspaceRoot } from "./paths";
import { readRecord, type Corpus } from "./corpus";
import type { RenderResult } from "../shared/types";

const exec = promisify(execFile);
const sourceRoots = ["crates/chitin-chem/src", "crates/chitin-chemdraw/src", "crates/chitin-chemdraw/assets"]
  .map((path) => join(workspaceRoot, path))
  .concat(join(plotsRoot, "../src"));
let sourceEpoch = Date.now();
let build: Promise<{ path: string; fingerprint: string; epoch: number }> | undefined;
let cachedBuild: Awaited<NonNullable<typeof build>> | undefined;
export const sourceRevision = () => String(sourceEpoch);

export async function fileHash(path: string): Promise<string> {
  const hash = createHash("sha256");
  for await (const bytes of createReadStream(path)) hash.update(bytes);
  return hash.digest("hex");
}

/** Source edits invalidate only current previews; measured historical results remain immutable. */
export function watchSources(
  onChange: () => void,
  roots: readonly string[] = [
    ...sourceRoots,
    join(workspaceRoot, "Cargo.toml"),
    join(workspaceRoot, "Cargo.lock"),
    join(workspaceRoot, "crates/chitin-chemdraw/Cargo.toml"),
    join(workspaceRoot, "crates/chitin-chem/Cargo.toml"),
    join(plotsRoot, "../Cargo.toml"),
  ],
): () => void {
  const watchers: FSWatcher[] = [];
  let timer: ReturnType<typeof setTimeout> | undefined;
  const changed = () => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      sourceEpoch++;
      cachedBuild = undefined;
      onChange();
    }, 250);
  };
  for (const root of roots) {
    try {
      watchers.push(watch(root, { recursive: statSync(root).isDirectory() }, changed));
    } catch {
      /* Optional asset directories may not exist. */
    }
  }
  return () => {
    clearTimeout(timer);
    watchers.forEach((watcher) => watcher.close());
  };
}

async function binaryPath(): Promise<string> {
  const metadata = await exec("cargo", ["metadata", "--no-deps", "--offline", "--locked", "--format-version=1"], {
    cwd: workspaceRoot,
    maxBuffer: 8 * 1024 * 1024,
  });
  return join(JSON.parse(metadata.stdout).target_directory, "release", "chitin-chemdraw-benchmark");
}

/** Copy before execution: cargo can replace its target while a preview is running. */
async function freezeBinary(path: string, expected?: string): Promise<{ path: string; fingerprint: string }> {
  const fingerprint = await fileHash(path);
  if (expected && expected !== fingerprint)
    throw new Error("Renderer fingerprint does not match the benchmark manifest");
  const directory = join(cacheRoot, "renderers", fingerprint);
  await mkdir(directory, { recursive: true });
  const frozen = join(directory, "renderer");
  await copyFile(path, frozen, constants.COPYFILE_EXCL).catch((error) => {
    if (error.code !== "EEXIST") throw error;
  });
  if ((await fileHash(frozen)) !== fingerprint) throw new Error("Cached renderer integrity check failed");
  await chmod(frozen, 0o700);
  return { path: frozen, fingerprint };
}

/** A pre-snapshot run is reproducible only if its exact binary is still available. */
export async function historicalBinary(corpus: Corpus): Promise<{ path: string; fingerprint: string } | null> {
  const expected = corpus.manifest.executable_sha256;
  if (!expected || !/^[a-f0-9]{64}$/.test(expected)) return null;
  for (const candidate of [
    join(corpus.directory, "renderer"),
    join(cacheRoot, "renderers", expected, "renderer"),
    await binaryPath(),
  ]) {
    try {
      if ((await fileHash(candidate)) === expected) return await freezeBinary(candidate, expected);
    } catch {
      /* Try the next verified source. */
    }
  }
  return null;
}

export async function getCurrentBinary(): Promise<{ path: string; fingerprint: string }> {
  if (cachedBuild?.epoch === sourceEpoch) return cachedBuild;
  if (build) {
    const previous = await build;
    if (previous.epoch === sourceEpoch) return previous;
  }
  const epoch = sourceEpoch;
  build = (async () => {
    await exec("cargo", ["build", "--release", "--locked", "-p", "chitin-chemdraw-benchmark"], {
      cwd: workspaceRoot,
      timeout: 180_000,
      maxBuffer: 4 * 1024 * 1024,
    });
    if (epoch !== sourceEpoch) throw new Error("Source changed during build; retry the preview");
    cachedBuild = { ...(await freezeBinary(await binaryPath())), epoch };
    return cachedBuild;
  })();
  try {
    return await build;
  } finally {
    build = undefined;
  }
}

/** Shared jobs deduplicate repeated clicks; at most two workers and eight waiting jobs. */
export class RenderQueue {
  private active = 0;
  private waiting: (() => void)[] = [];
  private jobs = new Map<string, Promise<void>>();
  async run(key: string, action: () => Promise<void>): Promise<void> {
    const existing = this.jobs.get(key);
    if (existing) return existing;
    if (this.waiting.length >= 8) throw new Error("Preview queue is full; retry shortly");
    const task = (async () => {
      if (this.active >= 2) await new Promise<void>((resolve) => this.waiting.push(resolve));
      else this.active++;
      try {
        await action();
      } finally {
        // Transfer the reserved slot directly: newcomers must not overtake waiters.
        const next = this.waiting.shift();
        if (next) next();
        else this.active--;
      }
    })();
    this.jobs.set(key, task);
    try {
      await task;
    } finally {
      this.jobs.delete(key);
    }
  }
}
const queue = new RenderQueue();

/** Rendering occurs inside the original bounded worker, with no shell interpolation. */
async function renderWorker(binary: string, corpus: Corpus, row: number, destination: string): Promise<void> {
  const record = await readRecord(corpus, row);
  const temporary = destination + `.${process.pid}.${Date.now()}.tmp`;
  const child = spawn(binary, ["--worker"], { cwd: workspaceRoot, stdio: ["pipe", "pipe", "pipe"] });
  let output = "";
  let errors = "";
  child.stdout.on("data", (bytes) => {
    if (output.length < 2_000_000) output += bytes;
  });
  child.stderr.on("data", (bytes) => {
    if (errors.length < 16_384) errors += bytes;
  });
  const timer = setTimeout(
    () => child.kill("SIGKILL"),
    Math.min(30, Math.max(5, corpus.manifest.config.timeout_seconds)) * 1000,
  );
  child.stdin.on("error", () => {});
  child.stdin.end(
    JSON.stringify({ case: record.case, max_atoms: corpus.manifest.config.max_atoms, svg_path: temporary }),
  );
  try {
    await new Promise<void>((resolve, reject) => {
      child.on("error", reject);
      child.on("close", (code, signal) =>
        code === 0
          ? resolve()
          : reject(new Error(`Renderer ${signal ?? code}: ${errors || "hard timeout or process failure"}`)),
      );
    });
    const finished = output
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line))
      .findLast((event) => event.event === "finished");
    if (finished?.record?.status !== "success") throw new Error(finished?.record?.error ?? "Renderer did not complete");
    await rename(temporary, destination);
  } finally {
    clearTimeout(timer);
    await unlink(temporary).catch(() => {});
  }
}

export async function renderSvg(corpus: Corpus, row: number, mode: "snapshot" | "current"): Promise<RenderResult> {
  if (!corpus.offsets.has(row)) throw new Error("Source row not found");
  if (mode === "snapshot") {
    const saved = join(corpus.directory, "svg", `${String(row).padStart(8, "0")}.svg`);
    if (
      await stat(saved)
        .then((value) => value.isFile())
        .catch(() => false)
    )
      return {
        url: `/api/runs/${encodeURIComponent(corpus.data.run.id)}/saved/${row}`,
        provenance: "saved",
        fingerprint: corpus.manifest.executable_sha256 ?? "unrecorded",
        warning: null,
      };
  }
  const binary = mode === "snapshot" ? await historicalBinary(corpus) : await getCurrentBinary();
  if (!binary)
    throw new Error(
      "Historical renderer unavailable. Select Current algorithm to preview explicitly; historical scores will not change.",
    );
  const directory = join(cacheRoot, "svg", binary.fingerprint, corpus.data.run.id);
  await mkdir(directory, { recursive: true });
  // A run can be replaced on disk; row identity alone is not a cache identity.
  const record = await readRecord(corpus, row);
  const inputHash = createHash("sha256")
    .update(JSON.stringify({ case: record.case, max_atoms: corpus.manifest.config.max_atoms }))
    .digest("hex");
  const artifact = `${row}-${inputHash}`;
  const destination = join(directory, `${artifact}.svg`);
  if (
    !(await stat(destination)
      .then((value) => value.isFile())
      .catch(() => false))
  ) {
    await queue.run(destination, () => renderWorker(binary.path, corpus, row, destination));
  }
  return {
    url: `/api/cache/${binary.fingerprint}/${encodeURIComponent(corpus.data.run.id)}/${artifact}`,
    provenance: mode,
    fingerprint: binary.fingerprint,
    warning:
      mode === "current"
        ? "Preview uses the current algorithm; plotted scores remain the selected historical benchmark."
        : null,
  };
}
