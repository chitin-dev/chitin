import { batch, createEffect, createMemo, createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import {
  GROUP_COLORS,
  GROUP_LABELS,
  rankGroup,
  type PlotData,
  type RunInfo,
  type TurnSelection,
} from "../shared/types";
import type { Derived } from "./lib/derive";
import { Button } from "./components/ui/button";
import { SelectField } from "./components/ui/select";
import { Prevalence, LengthChart, AngleChart } from "./components/charts";
import { InteractiveScatter } from "./components/interactive-scatter";
import { Inspector } from "./components/inspector";
import { TurnMap } from "./components/turn-map";
import type { BenchmarkJob } from "../server/benchmark";

function Panel(props: { letter: string; title: string; subtitle: string; children: JSX.Element; class?: string }) {
  return (
    <section class={`plot-panel ${props.class ?? ""}`} aria-label={`${props.letter}. ${props.title}`}>
      <header class="panel-heading">
        <span class="panel-letter">{props.letter}</span>
        <div>
          <h2>{props.title}</h2>
          <p>{props.subtitle}</p>
        </div>
      </header>
      {props.children}
    </section>
  );
}

export default function App() {
  const [runs, setRuns] = createSignal<RunInfo[]>([]);
  const [runId, setRunId] = createSignal(new URLSearchParams(location.search).get("run") ?? "");
  const [source, setSource] = createSignal("");
  const [followLatest, setFollowLatest] = createSignal(!location.search.includes("run="));
  const [job, setJob] = createSignal<BenchmarkJob | null>(null);
  const [starting, setStarting] = createSignal(false);
  const [connected, setConnected] = createSignal(false);
  const [data, setData] = createSignal<PlotData>();
  const [derived, setDerived] = createSignal<Derived>();
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal("");
  const [groups, setGroups] = createSignal([true, true, true, true]);
  const [selected, setSelected] = createSignal<number | null>(null);
  const [turnSelection, setTurnSelection] = createSignal<TurnSelection | null>(null);
  function selectMolecule(row: number) {
    setTurnSelection(null);
    setSelected(row);
  }
  const [expanded, setExpanded] = createSignal(false);
  let worker: Worker | undefined;
  const [workerReady, setWorkerReady] = createSignal(false);
  let requestId = 0;
  const selectedRun = createMemo(() => runs().find((run) => run.id === runId()));
  const groupCounts = createMemo(() => {
    const counts = [0, 0, 0, 0];
    for (const point of data()?.points ?? []) counts[rankGroup(point.rank)]++;
    return counts;
  });
  const points = createMemo(() => data()?.points ?? []);
  createEffect(() => {
    const run = selectedRun();
    if (!run || !workerReady() || !worker) return;
    setLoading(true);
    setError("");
    worker.postMessage({ id: ++requestId, run: run.id, revision: run.revision });
  });
  function selectRun(run: RunInfo) {
    if (run.id === runId()) return;
    setFollowLatest(false);
    activateRun(run);
  }
  function activateRun(run: RunInfo) {
    // Controlled select reconciliation can re-emit the same value without navigation.
    if (run.id === runId()) return;
    batch(() => {
      setRunId(run.id);
      setSelected(null);
      setTurnSelection(null);
      setData();
      setDerived();
    });
    const url = new URL(location.href);
    url.searchParams.set("run", run.id);
    history.replaceState(null, "", url);
  }
  onMount(() => {
    worker = new Worker(new URL("./plot.worker.ts", import.meta.url), { type: "module" });
    worker.onmessage = (event) => {
      if (event.data.id !== requestId) return;
      if (event.data.error) {
        setError(event.data.error);
        setLoading(false);
        return;
      }
      batch(() => {
        setData(event.data.data);
        setDerived(event.data.derived);
        setLoading(false);
      });
    };
    worker.onerror = () => {
      setError("Plot data worker failed. Refresh to reconnect.");
      setLoading(false);
    };
    setWorkerReady(true);
    const events = new EventSource("/api/events");
    events.onmessage = (event) => {
      const state: { runs: RunInfo[]; sourceRevision: string; job: BenchmarkJob | null } = JSON.parse(event.data);
      batch(() => {
        setConnected(true);
        setSource(state.sourceRevision);
        setJob(state.job);
        // Reconcile identities by revision: source-only changes must not reload chart data.
        setRuns((previous) =>
          state.runs.map(
            (next) => previous.find((run) => run.id === next.id && run.revision === next.revision) ?? next,
          ),
        );
        if ((followLatest() && state.runs[0]?.id !== runId()) || !state.runs.some((run) => run.id === runId())) {
          if (state.runs[0]) activateRun(state.runs[0]);
          else setRunId("");
        }
        if (!state.runs.length) setLoading(false);
      });
    };
    events.onerror = () => setConnected(false);
    onCleanup(() => {
      events.close();
      worker?.terminate();
    });
  });
  async function rerun() {
    setStarting(true);
    try {
      const response = await fetch("/api/benchmark", {
        method: "POST",
        headers: { "Content-Type": "application/json", "X-Chitin-Client": "plots" },
        body: JSON.stringify({ run: runId() }),
      });
      const result = await response.json();
      if (!response.ok) throw new Error(result.error ?? "Could not start benchmark");
      setJob(result);
      setFollowLatest(true);
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setStarting(false);
    }
  }
  return (
    <>
      <header class="topbar">
        <a href="/" class="brand">
          <span class="brand-mark">⌁</span>
          <span>
            chitin<span class="brand-section"> / diagram quality</span>
          </span>
        </a>
        <div class="topbar-actions">
          <span class="connection">
            <i classList={{ live: connected() }} />
            {connected() ? "Live" : "Reconnecting…"}
          </span>
          <a class="text-link" href="/api/spec" target="_blank" rel="noreferrer">
            Metrics ↗
          </a>
        </div>
      </header>
      <div classList={{ workspace: true, "with-inspector": selected() !== null }}>
        <main class="plots-main">
          <div class="page-heading">
            <div>
              <h1>Diagram quality</h1>
            </div>
            <div class="run-picker">
              <label class="field-label">Benchmark run</label>
              <SelectField
                label="Benchmark run"
                options={runs()}
                value={selectedRun()}
                onChange={selectRun}
                text={(run) =>
                  `${run.id} · ${run.engine === "rdkit_coordgen" ? "CoordGen / Chitin drawing" : "Chitin"} · ${run.cases.toLocaleString()}`
                }
                optionKey={(run) => run.id}
              />
              <label class="field-label">
                <input
                  type="checkbox"
                  checked={followLatest()}
                  onChange={(event) => {
                    setFollowLatest(event.currentTarget.checked);
                    if (followLatest() && runs()[0]) activateRun(runs()[0]);
                  }}
                />{" "}
                Follow latest
              </label>
            </div>
          </div>
          <Show when={selectedRun()}>
            <div class="run-context">
              <Button
                variant="outline"
                disabled={starting() || job()?.status === "building" || job()?.status === "running"}
                onClick={rerun}
              >
                Rerun benchmark
              </Button>
            </div>
          </Show>
          <Show when={job()}>
            <p class="description" role="status">
              {job()!.status}: {job()!.message}
            </p>
          </Show>
          <Show when={runs().length === 0 && !loading()}>
            <div class="empty-state">
              <h2>No benchmark runs</h2>
              <code>
                cargo run --release -p chitin-chemdraw-benchmark -- benchmarks/chitin-chemdraw/coconut-10k.yaml --output
                ../results/run-001
              </code>
            </div>
          </Show>
          <Show when={error()}>
            <div class="error-box" role="alert">
              {error()}
            </div>
          </Show>
          <Show when={loading()}>
            <p class="loading-line" role="status">
              <span class="spinner" />
              {data() ? "Refreshing…" : "Loading…"}
            </p>
          </Show>
          <Show when={data() && derived()}>
            <>
              <div class="run-context">
                <span>
                  {selectedRun()?.dataset} {selectedRun()?.version}
                </span>
                <span>{data()!.run.cases.toLocaleString()} inputs</span>
                <span>{data()!.completed.toLocaleString()} evaluated</span>
                <span>{data()!.clear.toLocaleString()} clear</span>
                <span class="badge">{selectedRun()?.protocol}</span>
              </div>
              <div class="legend">
                <span>Max block rank · B–H</span>
                <For each={GROUP_LABELS}>
                  {(label, index) => (
                    <button
                      class="legend-toggle"
                      classList={{ muted: !groups()[index()] }}
                      aria-pressed={groups()[index()]}
                      onClick={() =>
                        setGroups((previous) => previous.map((value, i) => (i === index() ? !value : value)))
                      }
                    >
                      <i style={{ background: GROUP_COLORS[index()] }} />
                      {label}
                      <small>n={groupCounts()[index()].toLocaleString()}</small>
                    </button>
                  )}
                </For>
              </div>
              <div classList={{ "plots-grid": true, "scatter-expanded": expanded() }}>
                <Panel letter="A" title="Quality flags" subtitle="Applicable cases (%)">
                  <Prevalence data={data()!} />
                </Panel>
                <Panel letter="B" title="Atom proximity" subtitle="By atom count" class="scatter-panel">
                  <div class="scatter-expand">
                    <Button variant="ghost" size="sm" onClick={() => setExpanded((value) => !value)}>
                      {expanded() ? "Compact plot" : "Expand plot"}
                    </Button>
                  </div>
                  <InteractiveScatter
                    metric="contacts"
                    points={points()}
                    groups={groups()}
                    selected={selected()}
                    onSelect={selectMolecule}
                  />
                </Panel>
                <Panel letter="C" title="Bond-length error" subtitle="Planar layouts · survival distribution">
                  <LengthChart derived={derived()!} groups={groups()} />
                </Panel>
                <Panel letter="D" title="Chain-angle error" subtitle="Acyclic carbon centers">
                  <AngleChart derived={derived()!} groups={groups()} />
                </Panel>
                <Panel letter="E" title="Ring cavity radius" subtitle="Maximum radius / L · planar faces">
                  <InteractiveScatter
                    metric="cavity"
                    points={points()}
                    groups={groups()}
                    selected={selected()}
                    onSelect={selectMolecule}
                    historical={selectedRun()?.protocol === "diagram-quality-v1"}
                  />
                </Panel>
                <Panel letter="F" title="Ring roundness" subtitle="Maximum Q = 4πA / P² · descriptive">
                  <InteractiveScatter
                    metric="roundness"
                    points={points()}
                    groups={groups()}
                    selected={selected()}
                    onSelect={selectMolecule}
                    historical={selectedRun()?.protocol === "diagram-quality-v1"}
                  />
                </Panel>
                <Panel letter="G" title="Drawing footprint" subtitle="Component hull area / (atoms × L²) · planar">
                  <InteractiveScatter
                    metric="footprint"
                    points={points()}
                    groups={groups()}
                    selected={selected()}
                    onSelect={selectMolecule}
                    historical={selectedRun()?.protocol === "diagram-quality-v1"}
                  />
                </Panel>
                <Panel
                  letter="H"
                  title="Large-cycle turns"
                  subtitle="Adjacent 2D turns · not torsions"
                  class="turn-panel"
                >
                  <TurnMap
                    points={data()!.turns ?? []}
                    molecules={points()}
                    groups={groups()}
                    historical={selectedRun()?.protocol !== "diagram-quality-v3"}
                    selected={turnSelection()}
                    onSelect={(selection) => {
                      setSelected(selection.row);
                      setTurnSelection(selection);
                    }}
                  />
                </Panel>
              </div>
              <footer class="page-footer">Geometry checks only; chemical correctness unverified.</footer>
            </>
          </Show>
        </main>
        <Show when={selected() !== null && runId()}>
          <Inspector
            external={data()?.run.engine === "rdkit_coordgen"}
            run={runId()}
            row={selected()!}
            sourceVersion={source()}
            turnSample={turnSelection()?.row === selected() ? turnSelection()?.sample : undefined}
            onClose={() => {
              setSelected(null);
              setTurnSelection(null);
            }}
          />
        </Show>
      </div>
    </>
  );
}
