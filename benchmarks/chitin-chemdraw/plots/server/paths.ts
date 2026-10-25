/** Resolve from package identity, never the shell's cwd or a build-relative guess. */
import { readFileSync } from "node:fs";
import { dirname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

function findParent(start: string, matches: (directory: string) => boolean): string {
  for (let directory = start; ; directory = dirname(directory)) {
    if (matches(directory)) return directory;
    if (dirname(directory) === directory) throw new Error(`Cannot resolve project root from ${start}`);
  }
}
export const plotsRoot = findParent(dirname(fileURLToPath(import.meta.url)), (directory) => {
  try {
    return JSON.parse(readFileSync(join(directory, "package.json"), "utf8")).name === "@chitin/chemdraw-plots";
  } catch {
    return false;
  }
});
export const workspaceRoot = findParent(plotsRoot, (directory) => {
  try {
    return readFileSync(join(directory, "Cargo.toml"), "utf8").includes("[workspace]");
  } catch {
    return false;
  }
});
// Data is a sibling of the benchmark package, independent of its ancestor's name.
export const benchmarkRoot = resolve(plotsRoot, "../..");
export const resultsRoot = join(benchmarkRoot, "results");
export const cacheRoot = join(plotsRoot, ".cache");

/** IDs and resolved paths are checked separately; encoded traversal never becomes a path. */
export function runDirectory(id: string): string {
  if (!/^[a-zA-Z0-9][a-zA-Z0-9._-]{0,180}$/.test(id) || id.includes("..")) throw new Error("Invalid run ID");
  const directory = resolve(resultsRoot, id);
  if (!directory.startsWith(resultsRoot + sep)) throw new Error("Run escaped results directory");
  return directory;
}

export function rowNumber(value: string): number {
  if (!/^[1-9][0-9]*$/.test(value)) throw new Error("Invalid row number");
  const row = Number(value);
  if (!Number.isSafeInteger(row)) throw new Error("Row number is out of range");
  return row;
}
