import { createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import { GROUP_COLORS, rankGroup, type PlotPoint } from "../../shared/types";
import { SpatialGrid, symlog, inverseSymlog, type View } from "@/lib/geometry";
import { Button } from "./ui/button";

interface Prepared {
  index: number;
  x: number;
  y: number;
  group: number;
}
export function Scatter(props: {
  points: PlotPoint[];
  groups: boolean[];
  selected: number | null;
  onPick: (indices: number[]) => void;
}) {
  let host!: HTMLDivElement, canvas!: HTMLCanvasElement, overlay!: HTMLCanvasElement;
  let width = 600,
    height = 410,
    frame = 0,
    hoverFrame = 0;
  let view: View = { x0: 0, x1: 3, y0: -0.1, y1: 4 };
  let original = { ...view };
  let prepared: Prepared[] = [];
  let byRow = new Map<number, Prepared>();
  let byIndex = new Map<number, Prepared>();
  let byGroup: Prepared[][] = [[], [], [], []];
  let grid = new SpatialGrid();
  let observer: ResizeObserver | undefined;
  let drag: { x: number; y: number; view: View; moved: boolean } | undefined;
  let hovered: number | undefined;
  const [tooltip, setTooltip] = createSignal<{ x: number; y: number; text: string }>();
  const [visible, setVisible] = createSignal(0);
  const margin = { left: 65, right: 18, top: 22, bottom: 52 };
  const plotWidth = () => Math.max(1, width - margin.left - margin.right);
  const plotHeight = () => Math.max(1, height - margin.top - margin.bottom);
  const screen = (point: Prepared) => ({
    x: margin.left + ((point.x - view.x0) / (view.x1 - view.x0)) * plotWidth(),
    y: height - margin.bottom - ((point.y - view.y0) / (view.y1 - view.y0)) * plotHeight(),
  });
  function schedule() {
    if (!frame) frame = requestAnimationFrame(paint);
  }
  function paintOverlay() {
    const context = overlay?.getContext("2d");
    if (!context) return;
    const ratio = Math.min(devicePixelRatio || 1, 2);
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, width, height);
    const highlights = new Set([byRow.get(props.selected ?? -1), byIndex.get(hovered ?? -1)]);
    for (const point of highlights) {
      if (!point || !props.groups[point.group]) continue;
      const { x, y } = screen(point);
      if (x < margin.left || x > width - margin.right || y < margin.top || y > height - margin.bottom) continue;
      context.beginPath();
      context.arc(x, y, 6, 0, Math.PI * 2);
      context.strokeStyle = "#111827";
      context.lineWidth = 1.8;
      context.stroke();
      context.beginPath();
      context.arc(x, y, 8, 0, Math.PI * 2);
      context.strokeStyle = "white";
      context.lineWidth = 2;
      context.stroke();
    }
  }
  /** Four batched fills, no per-point DOM nodes or reactive event subscriptions. */
  function paint() {
    frame = 0;
    const context = canvas?.getContext("2d");
    if (!context) return;
    const ratio = Math.min(devicePixelRatio || 1, 2);
    for (const layer of [canvas, overlay]) {
      if (layer.width !== Math.round(width * ratio) || layer.height !== Math.round(height * ratio)) {
        layer.width = Math.round(width * ratio);
        layer.height = Math.round(height * ratio);
      }
    }
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.clearRect(0, 0, width, height);
    context.font = "11px system-ui";
    context.lineWidth = 1;
    context.textAlign = "center";
    const xValues = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000];
    for (const value of xValues) {
      const x = screen({ x: Math.log10(value), y: 0, index: 0, group: 0 }).x;
      if (x < margin.left || x > width - margin.right) continue;
      context.beginPath();
      context.moveTo(x, margin.top);
      context.lineTo(x, height - margin.bottom);
      context.strokeStyle = "#e8ecf0";
      context.stroke();
      context.fillStyle = "#64748b";
      context.fillText(String(value), x, height - margin.bottom + 19);
    }
    context.textAlign = "right";
    for (const value of [0, 1, 2, 5, 10, 20, 50, 100, 200, 500, 1000]) {
      const y = screen({ x: 0, y: symlog(value), index: 0, group: 0 }).y;
      if (y < margin.top || y > height - margin.bottom) continue;
      context.beginPath();
      context.moveTo(margin.left, y);
      context.lineTo(width - margin.right, y);
      context.strokeStyle = "#e8ecf0";
      context.stroke();
      context.fillStyle = "#64748b";
      context.fillText(String(value), margin.left - 10, y + 4);
    }
    context.beginPath();
    context.moveTo(margin.left, margin.top);
    context.lineTo(margin.left, height - margin.bottom);
    context.lineTo(width - margin.right, height - margin.bottom);
    context.strokeStyle = "#a4adb8";
    context.stroke();
    context.textAlign = "center";
    context.fillStyle = "#475569";
    context.fillText("Source atom count · log scale", margin.left + plotWidth() / 2, height - 10);
    context.save();
    context.translate(14, margin.top + plotHeight() / 2);
    context.rotate(-Math.PI / 2);
    context.fillText("Close nonbonded atom pairs · symlog", 0, 0);
    context.restore();
    context.save();
    context.beginPath();
    context.rect(margin.left, margin.top, plotWidth(), plotHeight());
    context.clip();
    grid = new SpatialGrid();
    let count = 0;
    for (let group = 0; group < 4; group++) {
      if (!props.groups[group]) continue;
      context.beginPath();
      for (const point of byGroup[group]) {
        if (point.x < view.x0 || point.x > view.x1 || point.y < view.y0 || point.y > view.y1) continue;
        const { x, y } = screen(point);
        context.moveTo(x + 2.2, y);
        context.arc(x, y, 2.2, 0, Math.PI * 2);
        grid.add({ index: point.index, x, y });
        count++;
      }
      context.globalAlpha = 0.55;
      context.fillStyle = GROUP_COLORS[group];
      context.fill();
    }
    context.restore();
    context.globalAlpha = 1;
    setVisible(count);
    paintOverlay();
  }
  function reset() {
    view = { ...original };
    setTooltip();
    schedule();
  }
  function zoom(factor: number, x = margin.left + plotWidth() / 2, y = margin.top + plotHeight() / 2) {
    const px = (x - margin.left) / plotWidth(),
      py = 1 - (y - margin.top) / plotHeight();
    const sx = view.x1 - view.x0,
      sy = view.y1 - view.y0;
    const nx = Math.max(0.0001, Math.min((original.x1 - original.x0) * 20, sx * factor));
    const ny = Math.max(0.0001, Math.min((original.y1 - original.y0) * 20, sy * factor));
    const cx = view.x0 + sx * px,
      cy = view.y0 + sy * py;
    view = { x0: cx - nx * px, x1: cx + nx * (1 - px), y0: cy - ny * py, y1: cy + ny * (1 - py) };
    setTooltip();
    schedule();
  }
  function coordinates(event: PointerEvent | WheelEvent) {
    const bounds = canvas.getBoundingClientRect();
    return { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
  }
  createEffect(() => {
    const points = props.points;
    prepared = [];
    byRow = new Map();
    byIndex = new Map();
    byGroup = [[], [], [], []];
    let xmin = Infinity,
      xmax = -Infinity,
      ymax = 1;
    for (let index = 0; index < points.length; index++) {
      const record = points[index];
      if (record.contacts === null || record.atoms <= 0) continue;
      const point = { index, x: Math.log10(record.atoms), y: symlog(record.contacts), group: rankGroup(record.rank) };
      prepared.push(point);
      byRow.set(record.row, point);
      byIndex.set(point.index, point);
      byGroup[point.group].push(point);
      xmin = Math.min(xmin, point.x);
      xmax = Math.max(xmax, point.x);
      ymax = Math.max(ymax, point.y);
    }
    const gap = Math.max(0.08, (xmax - xmin) * 0.05);
    original = {
      x0: Number.isFinite(xmin) ? xmin - gap : 0,
      x1: Number.isFinite(xmax) ? xmax + gap : 3,
      y0: -ymax * 0.03,
      y1: ymax * 1.08,
    };
    view = { ...original };
    schedule();
  });
  createEffect(() => {
    props.groups;
    schedule();
  });
  createEffect(() => {
    props.selected;
    requestAnimationFrame(paintOverlay);
  });
  onMount(() => {
    observer = new ResizeObserver(([entry]) => {
      width = entry.contentRect.width;
      height = entry.contentRect.height;
      schedule();
    });
    observer.observe(host);
    schedule();
    const wheel = (event: WheelEvent) => {
      if (!event.ctrlKey && !event.metaKey) return;
      event.preventDefault();
      const point = coordinates(event);
      zoom(Math.exp(Math.max(-1, Math.min(1, event.deltaY * 0.005))), point.x, point.y);
    };
    canvas.addEventListener("wheel", wheel, { passive: false });
    onCleanup(() => canvas.removeEventListener("wheel", wheel));
  });
  onCleanup(() => {
    observer?.disconnect();
    cancelAnimationFrame(frame);
    cancelAnimationFrame(hoverFrame);
  });
  return (
    <div>
      <div class="chart-controls">
        <span class="tabular-nums">{visible().toLocaleString()} visible · no subsampling</span>
        <div class="flex gap-1">
          <Button variant="ghost" size="icon" aria-label="Zoom in" onClick={() => zoom(0.7)}>
            ＋
          </Button>
          <Button variant="ghost" size="icon" aria-label="Zoom out" onClick={() => zoom(1.4)}>
            −
          </Button>
          <Button variant="outline" size="sm" onClick={reset}>
            Reset view
          </Button>
        </div>
      </div>
      <div class="scatter-host" ref={host}>
        <canvas
          ref={canvas}
          class="scatter-layer"
          tabIndex={0}
          role="img"
          aria-label="B. Interactive molecular-size scatter plot. Drag to pan, Control-wheel to zoom. Search below to access any molecule."
          onDblClick={reset}
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            const { x, y } = coordinates(event);
            drag = { x, y, view: { ...view }, moved: false };
            canvas.setPointerCapture(event.pointerId);
          }}
          onPointerMove={(event) => {
            const { x, y } = coordinates(event);
            if (drag) {
              const dx = x - drag.x,
                dy = y - drag.y;
              if (Math.abs(dx) + Math.abs(dy) > 4) drag.moved = true;
              if (drag.moved) {
                const sx = (drag.view.x1 - drag.view.x0) / plotWidth(),
                  sy = (drag.view.y1 - drag.view.y0) / plotHeight();
                view = {
                  x0: drag.view.x0 - dx * sx,
                  x1: drag.view.x1 - dx * sx,
                  y0: drag.view.y0 + dy * sy,
                  y1: drag.view.y1 + dy * sy,
                };
                setTooltip();
                schedule();
              }
              return;
            }
            cancelAnimationFrame(hoverFrame);
            hoverFrame = requestAnimationFrame(() => {
              const hit = grid.inspect(x, y);
              hovered = hit.index;
              const point = props.points[hovered ?? -1];
              setTooltip(
                point
                  ? {
                      x: Math.min(x + 12, width - 235),
                      y: Math.max(0, y - 58),
                      text: `${point.id} · ${point.atoms} atoms · ${point.contacts} pairs${hit.count > 1 ? ` · ${hit.count} nearby: click to inspect all` : ""}`,
                    }
                  : undefined,
              );
              paintOverlay();
            });
          }}
          onPointerUp={(event) => {
            if (!drag) return;
            const clicked = !drag.moved;
            drag = undefined;
            canvas.releasePointerCapture(event.pointerId);
            if (clicked) {
              const { x, y } = coordinates(event);
              const candidates = grid.nearest(x, y);
              if (candidates.length) props.onPick(candidates);
            }
          }}
          onPointerCancel={() => {
            drag = undefined;
          }}
          onPointerLeave={() => {
            if (!drag) {
              hovered = undefined;
              setTooltip();
              paintOverlay();
            }
          }}
          onKeyDown={(event) => {
            if (event.key === "0" || event.key === "Home") {
              reset();
              return;
            }
            if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
            event.preventDefault();
            const list = prepared.filter((point) => props.groups[point.group]);
            const previous = list.findIndex((point) => props.points[point.index].row === props.selected);
            const next = list[(previous + (event.key === "ArrowRight" ? 1 : -1) + list.length) % list.length];
            if (next) props.onPick([next.index]);
          }}
        />
        <canvas ref={overlay} class="scatter-layer pointer-events-none" aria-hidden="true" />
        <Show when={tooltip()}>
          {(tip) => (
            <div class="plot-tooltip" style={{ left: `${tip().x}px`, top: `${tip().y}px` }}>
              {tip().text}
            </div>
          )}
        </Show>
      </div>
      <p class="chart-note">
        Drag to pan · Ctrl/⌘ + wheel to zoom · double-click to reset · ←/→ to select · overlapping points remain
        individually selectable
      </p>
    </div>
  );
}
