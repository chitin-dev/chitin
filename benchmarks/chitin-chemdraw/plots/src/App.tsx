import { batch, createEffect, createMemo, createSignal, For, onCleanup, onMount, Show, type JSX } from "solid-js";
import { GROUP_COLORS, GROUP_LABELS, rankGroup, type PlotData, type RunInfo } from "../shared/types";
import type { Derived } from "./lib/derive";
import { Button } from "./components/ui/button";
import { SelectField } from "./components/ui/select";
import { Prevalence, LengthChart, AngleChart } from "./components/charts";
import { InteractiveScatter } from "./components/interactive-scatter";
import { Inspector } from "./components/inspector";
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
            {connected() ? "Watching local runs" : "Reconnecting…"}
          </span>
          <a class="text-link" href="/api/spec" target="_blank" rel="noreferrer">
            Metric protocol ↗
          </a>
        </div>
      </header>
      <div classList={{ workspace: true, "with-inspector": selected() !== null }}>
        <main class="plots-main">
          <div class="page-heading">
            <div>
              <p class="eyebrow">SCIENTIFIC BENCHMARK · LOCAL WORKSPACE</p>
              <h1>Diagram quality</h1>
              <p class="description">
                Measured geometry, not a visual quality score. Inspect every molecule behind the data.
              </p>
            </div>
            <div class="run-picker">
              <label class="field-label">Benchmark run</label>
              <SelectField
                label="Benchmark run"
                options={runs()}
                value={selectedRun()}
                onChange={selectRun}
                text={(run) => `${run.id} · ${run.cases.toLocaleString()}`}
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
                Follow latest run
              </label>
            </div>
          </div>
          <Show when={selectedRun()}>
            <div class="run-context">
              <span>
                Results are measured snapshots. New runs refresh automatically; source edits refresh current-algorithm
                previews.
              </span>
              <Button
                variant="outline"
                disabled={starting() || job()?.status === "building" || job()?.status === "running"}
                onClick={rerun}
              >
                Rerun with current algorithm
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
              <h2>No versioned benchmark runs yet</h2>
              <p>Run the benchmark below. New results appear automatically; no frontend rebuild is needed.</p>
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
              {data() ? "Refreshing committed records…" : "Loading measurements in background…"}
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
                <span>{data()!.clear.toLocaleString()} basic screens clear</span>
                <span class="badge">{selectedRun()?.protocol}</span>
              </div>
              <div class="legend">
                <span>Maximum block rank · B–G</span>
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
                <Panel
                  letter="A"
                  title="Individual quality flags"
                  subtitle="Applicable-case prevalence · categories overlap"
                >
                  <Prevalence data={data()!} />
                </Panel>
                <Panel
                  letter="B"
                  title="Atom proximity versus molecular size"
                  subtitle="All measured points · click to inspect the source structure"
                  class="scatter-panel"
                >
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
                    onSelect={setSelected}
                  />
                </Panel>
                <Panel
                  letter="C"
                  title="Bond-length error distribution"
                  subtitle="Empirical survival probability · planar layouts only"
                >
                  <LengthChart derived={derived()!} groups={groups()} />
                </Panel>
                <Panel
                  letter="D"
                  title="Chain-angle error by topology"
                  subtitle="Applicable acyclic carbon centers · full measured distributions"
                >
                  <AngleChart derived={derived()!} groups={groups()} />
                </Panel>
                <Panel
                  letter="E"
                  title="Maximum ring cavity radius"
                  subtitle="Largest empty skeleton cavity / L · simple bounded planar faces"
                >
                  <InteractiveScatter
                    metric="cavity"
                    points={points()}
                    groups={groups()}
                    selected={selected()}
                    onSelect={setSelected}
                    historical={selectedRun()?.protocol === "diagram-quality-v1"}
                  />
                  <p class="chart-note">
                    Search tolerance ≤ 0.02 L; incomplete searches and crossing blocks are excluded. Inspect the
                    contributing ring size before comparing molecules.
                  </p>
                </Panel>
                <Panel
                  letter="F"
                  title="Ring roundness"
                  subtitle="Maximum Q = 4πA / P² · descriptive, not a pass/fail score"
                >
                  <InteractiveScatter
                    metric="roundness"
                    points={points()}
                    groups={groups()}
                    selected={selected()}
                    onSelect={setSelected}
                    historical={selectedRun()?.protocol === "diagram-quality-v1"}
                  />
                  <p class="chart-note">
                    Q near one means round, not necessarily wrong. Ring atom ordinals, area and perimeter are available
                    in the inspector.
                  </p>
                </Panel>
                <Panel
                  letter="G"
                  title="Normalized drawing footprint"
                  subtitle="Sum of component hull areas / (source atoms × L²) · planar only"
                >
                  <InteractiveScatter
                    metric="footprint"
                    points={points()}
                    groups={groups()}
                    selected={selected()}
                    onSelect={setSelected}
                    historical={selectedRun()?.protocol === "diagram-quality-v1"}
                  />
                  <p class="chart-note">
                    Salt packing does not inflate component areas. Smaller is not always better: compactness must not
                    introduce new collisions.
                  </p>
                </Panel>
              </div>
              <footer class="page-footer">
                Basic readability is not chemical correctness or IUPAC certification. Tetrahedral decoding and chemical
                label contents remain unverified.
                <span>Data: committed JSONL records · automatic discovery every 2 s · SVG: on demand only</span>
              </footer>
            </>
          </Show>
        </main>
        <Show when={selected() !== null && runId()}>
          <Inspector run={runId()} row={selected()!} sourceVersion={source()} onClose={() => setSelected(null)} />
        </Show>
      </div>
    </>
  );
}
