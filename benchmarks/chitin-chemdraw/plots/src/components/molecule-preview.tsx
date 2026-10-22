import { For, Show } from "solid-js";
import type { SceneFrame } from "../../shared/types";

/** Overlay historical source centers without rewriting or trusting SVG markup. */
export function MoleculePreview(props: {
  url: string;
  alt: string;
  frame?: SceneFrame | null;
  atoms?: readonly number[];
  zoom?: number;
  class?: string;
}) {
  const positions = () => props.frame && props.atoms?.map((id) => props.frame!.positions[id]);
  return (
    <Show
      when={props.frame && props.atoms?.length && positions()?.every(Boolean)}
      fallback={
        <img
          src={props.url}
          alt={props.alt}
          class={props.class}
          loading="lazy"
          decoding="async"
          style={{ width: `${(props.zoom ?? 1) * 100}%`, "max-width": "none" }}
        />
      }
    >
      <svg
        viewBox={props.frame!.view_box.join(" ")}
        role="img"
        aria-label={`${props.alt}; selected four-atom path highlighted`}
        class={props.class}
        style={{ width: `${(props.zoom ?? 1) * 100}%`, height: "auto", "max-width": "none" }}
      >
        <image
          href={props.url}
          x={props.frame!.view_box[0]}
          y={props.frame!.view_box[1]}
          width={props.frame!.view_box[2]}
          height={props.frame!.view_box[3]}
        />
        <polyline
          points={positions()!
            .map((p) => p.join(","))
            .join(" ")}
          fill="none"
          stroke="#f59e0b"
          stroke-width="10"
          stroke-opacity="0.45"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
        <For each={positions()}>
          {(p, i) => (
            <g>
              <circle cx={p[0]} cy={p[1]} r="7" fill="#f59e0b" fill-opacity="0.5" />
              <text x={p[0] + 8} y={p[1] - 8} font-size="12" fill="#92400e">
                {props.atoms![i()]}
              </text>
            </g>
          )}
        </For>
      </svg>
    </Show>
  );
}
