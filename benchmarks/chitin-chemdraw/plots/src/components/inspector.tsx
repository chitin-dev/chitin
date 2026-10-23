import { createEffect, createResource, createSignal, For, onCleanup, Show } from "solid-js";
import { Dialog } from "@kobalte/core/dialog";
import type { CaseRecord, RenderResult } from "../../shared/types";
import { Button } from "./ui/button";
import { SelectField } from "./ui/select";
import { MoleculePreview } from "./molecule-preview";

/** Selection, metric provenance and image loading have independent lifetimes. */
export function Inspector(props: {
  external?: boolean;
  run: string;
  row: number;
  sourceVersion: string;
  onClose: () => void;
  turnSample?: number;
}) {
  const [mode, setMode] = createSignal<"snapshot" | "current">("snapshot");
  createEffect(() => {
    if (props.external) setMode("snapshot");
  });
  const [zoom, setZoom] = createSignal(1);
  const [visibleFaces, setVisibleFaces] = createSignal(6);
  let recordRequest: AbortController | undefined, renderRequest: AbortController | undefined;
  const [recordState, records] = createResource(
    () => ({ run: props.run, row: props.row }),
    async (key): Promise<CaseRecord> => {
      recordRequest?.abort();
      recordRequest = new AbortController();
      const response = await fetch(`/api/runs/${encodeURIComponent(key.run)}/records/${key.row}`, {
        signal: recordRequest.signal,
      });
      const result = await response.json();
      if (!response.ok) throw new Error(result.error);
      return result;
    },
  );
  const [imageState, images] = createResource(
    () => ({
      run: props.run,
      row: props.row,
      mode: mode(),
      source: mode() === "current" ? props.sourceVersion : "historical",
    }),
    async (key): Promise<RenderResult> => {
      renderRequest?.abort();
      renderRequest = new AbortController();
      setZoom(1);
      setVisibleFaces(6);
      const response = await fetch(`/api/runs/${encodeURIComponent(key.run)}/render/${key.row}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "X-Chitin-Client": "plots" },
        body: JSON.stringify({ mode: key.mode }),
        signal: renderRequest.signal,
      });
      const result = await response.json();
      if (!response.ok) throw new Error(result.error);
      return result;
    },
  );
  // Resource errors belong to the local retry UI, not the root Solid error boundary.
  const record = () => (recordState.error ? undefined : recordState());
  const image = () => (imageState.error ? undefined : imageState());
  const selectedPath = () =>
    !recordState.loading && record()?.case.row === props.row && props.turnSample !== undefined
      ? record()?.metrics?.macrocycle_geometry?.samples[props.turnSample]
      : undefined;
  const historicalFrame = () => (mode() === "snapshot" ? record()?.metrics?.macrocycle_geometry?.scene : undefined);
  onCleanup(() => {
    recordRequest?.abort();
    renderRequest?.abort();
  });
  const drawing = () => (
    <Show when={!imageState.loading && !imageState.error && image()}>
      {(result) => (
        <div class="molecule-scroll">
          <MoleculePreview
            url={result().url}
            alt={`Chemical drawing of source row ${props.row}`}
            zoom={zoom()}
            frame={historicalFrame()}
            atoms={selectedPath()?.atoms}
          />
        </div>
      )}
    </Show>
  );
  return (
    <aside class="inspector" aria-label="Molecule inspector">
      <div class="inspector-heading">
        <div>
          <p class="eyebrow">SOURCE ROW {props.row}</p>
          <h2>{record()?.case.id ?? "Molecule"}</h2>
        </div>
        <Button variant="ghost" size="icon" aria-label="Close inspector" onClick={props.onClose}>
          ×
        </Button>
      </div>
      <label class="field-label">Renderer</label>
      <SelectField
        label="Renderer provenance"
        options={props.external ? ["snapshot"] : (["snapshot", "current"] as ("snapshot" | "current")[])}
        value={mode()}
        onChange={setMode}
        text={(value) =>
          value === "snapshot"
            ? props.external
              ? "CoordGen layout · Chitin drawing"
              : "Run snapshot"
            : "Current preview"
        }
      />
      <Show when={mode() === "current"}>
        <p class="notice">Preview only; run metrics unchanged.</p>
      </Show>
      <Show when={imageState.loading}>
        <div class="image-loading" role="status">
          <span class="spinner" />
          Loading SVG…
        </div>
      </Show>
      <Show when={imageState.error}>
        <div class="error-box" role="alert">
          {String(imageState.error?.message ?? imageState.error)}
          <div class="flex flex-wrap gap-2 mt-3">
            <Button variant="outline" size="sm" onClick={() => images.refetch()}>
              Retry
            </Button>
            <Show when={mode() === "snapshot"}>
              <Button size="sm" onClick={() => setMode("current")}>
                Current preview
              </Button>
            </Show>
          </div>
        </div>
      </Show>
      {drawing()}
      <Show when={image() && !imageState.loading && !imageState.error}>
        <>
          <div class="flex items-center justify-between gap-2 my-2">
            <span class="badge">{image()?.provenance}</span>
            <div class="flex gap-1">
              <Button
                variant="ghost"
                size="icon"
                aria-label="Zoom molecule out"
                onClick={() => setZoom((value) => Math.max(0.5, value / 1.3))}
              >
                −
              </Button>
              <Button
                variant="ghost"
                size="icon"
                aria-label="Zoom molecule in"
                onClick={() => setZoom((value) => Math.min(8, value * 1.3))}
              >
                ＋
              </Button>
              <a class="text-link text-xs" href={image()?.url} download={`${record()?.case.id ?? props.row}.svg`}>
                Download SVG
              </a>
            </div>
          </div>
          <Dialog>
            <Dialog.Trigger class="text-link text-xs">Expand structure</Dialog.Trigger>
            <Dialog.Portal>
              <Dialog.Overlay class="dialog-overlay" />
              <Dialog.Content class="dialog-content">
                <div class="flex items-center justify-between">
                  <Dialog.Title>{record()?.case.id ?? "Structure"}</Dialog.Title>
                  <Dialog.CloseButton class="dialog-close" aria-label="Close expanded structure">
                    ×
                  </Dialog.CloseButton>
                </div>
                <Dialog.Description class="chart-note">
                  {mode() === "snapshot" ? "Run snapshot" : "Current preview · run metrics unchanged"}
                </Dialog.Description>
                <MoleculePreview
                  url={image()!.url}
                  alt="Expanded chemical structure"
                  class="expanded-molecule"
                  frame={historicalFrame()}
                  atoms={selectedPath()?.atoms}
                />
              </Dialog.Content>
            </Dialog.Portal>
          </Dialog>
          <p class="fingerprint">renderer {image()?.fingerprint.slice(0, 16)}</p>
        </>
      </Show>
      <Show when={recordState.error}>
        <p class="error-box">{String(recordState.error?.message ?? recordState.error)}</p>
        <Button onClick={() => records.refetch()}>Retry record</Button>
      </Show>
      <Show when={!recordState.loading && record()}>
        {(entry) => (
          <>
            <Show when={selectedPath()}>
              {(path) => (
                <section class="ring-face" aria-label="Selected large-cycle path">
                  <strong>Selected path · atoms {path().atoms.join(" → ")}</strong>
                  <span>
                    Signed turns{" "}
                    {path()
                      .turns.map((t) => t.toFixed(2) + "°")
                      .join(" / ")}
                  </span>
                  <span>
                    {path().classes.join(" / ")} · selected cycle {path().cycle_size} atoms · block rank{" "}
                    {path().block_rank}
                  </span>
                  <small>
                    {mode() === "snapshot"
                      ? "Orange: source atoms · zero-based indices"
                      : "Run statistics · no preview overlay"}
                  </small>
                </section>
              )}
            </Show>
            <Show when={entry().metrics?.macrocycle_geometry}>
              {(geometry) => (
                <p class="chart-note">
                  Angles {geometry().valid_angles}/{geometry().unique_angles} · cycles{" "}
                  {geometry().selected_large_cycles} · excluded blocks {geometry().excluded_blocks}
                </p>
              )}
            </Show>
            <div class="molecule-facts">
              <span>{entry().features?.atoms ?? "—"} atoms</span>
              <span>{entry().features?.bonds ?? "—"} bonds</span>
              <span>rank {entry().features?.maximum_block_rank ?? "—"}</span>
            </div>
            <label class="field-label" for="selected-smiles">
              Source SMILES
            </label>
            <textarea id="selected-smiles" class="input smiles" value={entry().case.smiles} readOnly rows="3" />
            <Show when={entry().metrics?.ring_geometry}>
              {(geometry) => (
                <details class="ring-details">
                  <summary>Ring geometry · {geometry().faces.length} bounded faces</summary>
                  <p class="chart-note">
                    Area: L² · length: L · atoms: zero-based · excluded blocks: {geometry().excluded_blocks}
                  </p>
                  <For each={geometry().faces.slice(0, visibleFaces())}>
                    {(face, index) => (
                      <div class="ring-face">
                        <strong>
                          Face {index() + 1} · {face.atoms.length} atoms
                        </strong>
                        <span>
                          Q {face.isoperimetric_ratio.toPrecision(4)} · area {face.area.toPrecision(4)} · perimeter{" "}
                          {face.perimeter.toPrecision(4)}
                        </span>
                        <span>
                          Cavity [{face.cavity_radius.toPrecision(4)}, {face.cavity_upper_bound.toPrecision(4)}] L ·{" "}
                          {face.cavity_converged ? "converged" : "budget exhausted"}
                        </span>
                        <small>Atoms: {face.atoms.join(", ")}</small>
                      </div>
                    )}
                  </For>
                  <Show when={visibleFaces() < geometry().faces.length}>
                    <Button variant="ghost" size="sm" onClick={() => setVisibleFaces((count) => count + 6)}>
                      Show more faces
                    </Button>
                  </Show>
                </details>
              )}
            </Show>
            <h3 class="section-title">Historical measurements</h3>
            <div class="metric-table">
              <For each={Object.entries(entry().metrics?.measurements ?? {})}>
                {([name, metric]) => (
                  <div class="metric-row">
                    <div title={metric.reason ?? name}>
                      {name.replaceAll("_", " ")}
                      <small>
                        {metric.state === "measured"
                          ? metric.violations === null
                            ? `${metric.eligible} eligible · descriptive`
                            : `${metric.violations} flags / ${metric.eligible} eligible`
                          : metric.state.replaceAll("_", " ")}
                      </small>
                    </div>
                    <span class="tabular-nums">{metric.value === null ? "—" : metric.value.toPrecision(4)}</span>
                  </div>
                )}
              </For>
            </div>
          </>
        )}
      </Show>
    </aside>
  );
}
