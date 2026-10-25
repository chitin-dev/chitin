/** Explicit reruns preserve source inputs/seed while selecting the current algorithm. */
import { spawn, type ChildProcess } from "node:child_process";
import { mkdir, stat, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import type { Corpus } from "./corpus";
import { benchmarkRoot, cacheRoot, resultsRoot, workspaceRoot } from "./paths";
import { fileHash, getCurrentBinary } from "./renderer";

export interface BenchmarkJob {
  run: string;
  status: "building" | "running" | "complete" | "error";
  message: string;
}
let job: BenchmarkJob | null = null;
let child: ChildProcess | undefined;
export const benchmarkJob = () => job;

/** One user-requested sweep at a time; a source-save never silently starts 10,000 jobs. */
export function startBenchmark(corpus: Corpus, notify: () => void): BenchmarkJob {
  if (job?.status === "building" || job?.status === "running") throw new Error("A benchmark is already running");
  const id = `rerun-${corpus.data.run.id.slice(0, 90)}-${Date.now()}-${randomUUID().slice(0, 6)}`;
  job = { run: id, status: "building", message: "Building the current benchmark renderer…" };
  void (async () => {
    try {
      let input = corpus.manifest.config.input;
      if (!(await stat(input).catch(() => null))) {
        // A benchmark directory may move with its local data; never edit old manifests.
        const suffix = input.split("/data/").at(-1);
        if (suffix && suffix !== input) input = join(benchmarkRoot, "data", suffix);
      }
      if ((await fileHash(input)) !== corpus.manifest.input_sha256)
        throw new Error("Dataset checksum changed; a paired rerun requires the original input bytes");
      const renderer = await getCurrentBinary();
      const directory = join(cacheRoot, "reruns");
      await mkdir(directory, { recursive: true });
      const config = join(directory, `${id}.yaml`);
      // The manifest contains the resolved original config; JSON is valid YAML.
      await writeFile(config, JSON.stringify({ ...corpus.manifest.config, input, output: join(resultsRoot, id) }));
      job = { run: id, status: "running", message: "Evaluating source records; plots update as checkpoints arrive." };
      notify();
      child = spawn(renderer.path, [config], { cwd: workspaceRoot, stdio: ["ignore", "ignore", "pipe"] });
      let tail = "";
      child.stderr?.on("data", (bytes) => {
        tail = (tail + bytes).slice(-4096);
      });
      await new Promise<void>((resolve, reject) => {
        child!.on("error", reject);
        child!.on("close", (code) => (code === 0 ? resolve() : reject(new Error(tail || `Benchmark exited ${code}`))));
      });
      job = { run: id, status: "complete", message: "Benchmark finished. All committed records are available." };
    } catch (error) {
      job = { run: id, status: "error", message: error instanceof Error ? error.message : String(error) };
    } finally {
      child = undefined;
      notify();
    }
  })();
  return { ...job };
}
