import { batch, createEffect, createMemo, createSignal, For, Show } from "solid-js";
import { GROUP_COLORS, rankGroup, type PlotPoint } from "../../shared/types";
import { matchesPointQuery, scatterLabel, scatterValue, type ScatterMetric } from "../../shared/plot-metrics";
import { Scatter } from "./scatter";
import { Button } from "./ui/button";

/** Every scatter owns its query/candidate page, while SVG inspection is shared by source row. */
export function InteractiveScatter(props: {
  points: PlotPoint[];
  groups: boolean[];
  metric: ScatterMetric;
  selected: number | null;
  onSelect: (row: number) => void;
  historical?: boolean;
}) {
  const [candidates, setCandidates] = createSignal<number[]>([]);
  const [query, setQuery] = createSignal("");
  const [browse, setBrowse] = createSignal(false);
  const [page, setPage] = createSignal(0);
  const rows = createMemo(() => new Map(props.points.map((point) => [point.row, point])));
  const measured = createMemo(() =>
    props.points.filter((point) => scatterValue(point, props.metric) !== null && point.atoms > 0),
  );
  const choices = createMemo(() => {
    const q = query().trim().toLowerCase();
    const source = q || browse() ? measured() : candidates().flatMap((row) => rows().get(row) ?? []);
    return source.filter(
      (point) =>
        scatterValue(point, props.metric) !== null &&
        props.groups[rankGroup(point.rank)] &&
        matchesPointQuery(point, q),
    );
  });
  createEffect(() => {
    query();
    browse();
    candidates();
    props.groups;
    setPage(0);
  });
  createEffect(() => {
    props.points;
    batch(() => {
      setCandidates([]);
      setQuery("");
      setBrowse(false);
      setPage(0);
    });
  });
  function pick(indices: number[]) {
    const selected = indices.map((index) => props.points[index].row);
    batch(() => {
      setCandidates(selected);
      setQuery("");
      setBrowse(false);
      setPage(0);
    });
    if (selected[0] !== undefined) props.onSelect(selected[0]);
  }
  const pageCount = () => Math.ceil(choices().length / 12);
  return (
    <div class="interactive-scatter" data-metric={props.metric}>
      <Show
        when={measured().length > 0}
        fallback={
          <p class="metric-empty" role="status">
            {props.historical ? "Not recorded. Rerun benchmark." : "No applicable measurements."}
          </p>
        }
      >
        <Scatter
          points={props.points}
          groups={props.groups}
          metric={props.metric}
          selected={props.selected}
          onPick={pick}
        />
        <div class="point-browser">
          <div class="point-search">
            <input
              class="input"
              aria-label="Find a molecule by ID or source row"
              placeholder="Find CNP ID or source row…"
              value={query()}
              onInput={(event) => setQuery(event.currentTarget.value)}
            />
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                setBrowse((value) => !value);
                setCandidates([]);
              }}
            >
              {browse() ? "Hide list" : "Browse all points"}
            </Button>
          </div>
          <Show when={query() || browse() || candidates().length}>
            <div class="point-list-heading">
              <span>
                {choices().length.toLocaleString()}{" "}
                {query() ? "matching points" : browse() ? "points" : "nearby candidates"}
              </span>
            </div>
            <div class="point-list">
              <For each={choices().slice(page() * 12, page() * 12 + 12)}>
                {(point) => (
                  <button
                    class="point-row"
                    classList={{ selected: props.selected === point.row }}
                    onClick={() => props.onSelect(point.row)}
                  >
                    <i style={{ background: GROUP_COLORS[rankGroup(point.rank)] }} />
                    <strong>{point.id}</strong>
                    <span>row {point.row}</span>
                    <span>
                      {point.atoms} atoms · {scatterLabel(point, props.metric)}
                    </span>
                  </button>
                )}
              </For>
            </div>
            <div class="point-pagination">
              <Button variant="ghost" size="sm" disabled={page() <= 0} onClick={() => setPage((value) => value - 1)}>
                Previous
              </Button>
              <span>
                {pageCount() ? page() + 1 : 0} / {pageCount()}
              </span>
              <Button
                variant="ghost"
                size="sm"
                disabled={page() + 1 >= pageCount()}
                onClick={() => setPage((value) => value + 1)}
              >
                Next
              </Button>
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
}
