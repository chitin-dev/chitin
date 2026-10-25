import { createMemo } from "solid-js";
import { palette, type ColorScale } from "../../shared/color-mapping";

/** Numeric labels remain in original units even when the color mapping is logarithmic. */
export function ColorLegend(props: { label: string; scale: ColorScale; note?: string }) {
  const gradient = Array.from({ length: 17 }, (_, i) => `${palette(i / 16)} ${(i / 16) * 100}%`).join(",");
  const ticks = createMemo(() =>
    [0, 0.5, 1].map((t) => {
      const { min, max, log } = props.scale;
      const value = log ? Math.expm1(Math.log1p(min) + t * (Math.log1p(max) - Math.log1p(min))) : min + t * (max - min);
      return Number(value.toPrecision(3)).toLocaleString();
    }),
  );
  return (
    <div class="color-legend" role="group" aria-label={`${props.label} color scale`}>
      <div class="color-legend-title">
        <span>{props.label}</span>
        <span>
          {props.note}
          {props.scale.log ? " · log1p" : ""}
        </span>
      </div>
      <div class="color-legend-bar" style={{ background: `linear-gradient(to right, ${gradient})` }} />
      <div class="color-legend-ticks">
        <span>{ticks()[0]}</span>
        <span>{props.scale.min === props.scale.max ? "constant" : ticks()[1]}</span>
        <span>{ticks()[2]}</span>
      </div>
      <span class="chart-note">
        {props.scale.measured ? "" : "No measured values · "}Gray: not measured
        {props.scale.missing ? ` (${props.scale.missing.toLocaleString()})` : ""}
      </span>
    </div>
  );
}
