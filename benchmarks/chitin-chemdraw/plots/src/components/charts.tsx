import { createMemo, createSignal, For, Show } from "solid-js";
import { GROUP_COLORS, GROUP_LABELS, type PlotData } from "../../shared/types";
import type { Derived } from "@/lib/derive";
import { symlog } from "@/lib/geometry";

const NAMES: Record<string, string> = {
  nonbonded_atom_proximity: "Nonbonded atom proximity",
  atom_edge_proximity: "Atom–bond proximity",
  unannotated_edge_contact: "Unannotated bond contact",
  acyclic_carbon_angle_rmse_deg: "Chain-angle deviation",
  label_bond_overlap: "Label–bond contact",
  label_label_overlap: "Label–label overlap",
  relative_bond_length_rmse: "Bond-length deviation",
  macrocycle_angle_deviation_rmse_deg: "Large-cycle angle deviation",
  macrocycle_near_linear_fraction: "Large-cycle near-straight paths",
};
export function Prevalence(props: { data: PlotData }) {
  const rows = createMemo(() =>
    Object.entries(NAMES).flatMap(([key, name]) => {
      const metric = props.data.metrics[key];
      return metric?.measured_cases && metric.affected_cases !== null
        ? [{ key, name, ...metric, rate: (metric.affected_cases / metric.measured_cases) * 100 }]
        : [];
    }),
  );
  const maximum = createMemo(() => Math.max(15, ...rows().map((row) => row.rate * 1.15)));
  const bottom = () => Math.max(310, 25 + (rows().length - 1) * 42 + 22);
  const [hover, setHover] = createSignal<string>();
  return (
    <>
      <svg
        viewBox={`0 0 660 ${bottom() + 55}`}
        class="data-svg"
        role="img"
        aria-label="A. Quality-flag prevalence with applicable denominators"
      >
        <For each={[0, 0.25, 0.5, 0.75, 1]}>
          {(fraction) => (
            <g>
              <line x1={260 + fraction * 320} x2={260 + fraction * 320} y1="20" y2={bottom()} class="grid-line" />
              <text x={260 + fraction * 320} y={bottom() + 23} text-anchor="middle" class="axis-text">
                {(maximum() * fraction).toFixed(1)}%
              </text>
            </g>
          )}
        </For>
        <For each={rows()}>
          {(row, i) => (
            <g
              onPointerEnter={() =>
                setHover(
                  `${row.name}: ${row.affected_cases?.toLocaleString()} / ${row.measured_cases.toLocaleString()} applicable diagrams`,
                )
              }
              onPointerLeave={() => setHover()}
            >
              <rect x="0" y={25 + i() * 42 - 17} width="650" height="40" fill="transparent" />
              <text x="246" y={25 + i() * 42 + 4} text-anchor="end" class="plot-label">
                {row.name}
              </text>
              <line
                x1="260"
                x2={260 + (row.rate / maximum()) * 320}
                y1={25 + i() * 42}
                y2={25 + i() * 42}
                stroke={row.key.includes("rmse") ? "#8a79ac" : "#6798c9"}
                stroke-width="2"
              />
              <circle
                cx={260 + (row.rate / maximum()) * 320}
                cy={25 + i() * 42}
                r="4"
                fill={row.key.includes("rmse") ? "#7566a4" : "#2864bc"}
              />
              <text x={270 + (row.rate / maximum()) * 320} y={29 + i() * 42} class="plot-label">
                {row.rate.toFixed(2)}%
              </text>
            </g>
          )}
        </For>
        <text x="420" y={bottom() + 49} text-anchor="middle" class="axis-title">
          Applicable diagrams with ≥1 flagged object (%)
        </text>
      </svg>
      <p class="chart-note h-9">{hover() ?? "Hover for counts · purple: style proxies"}</p>
    </>
  );
}

export function LengthChart(props: { derived: Derived; groups: boolean[] }) {
  const [hover, setHover] = createSignal<string>();
  const curves = () => props.derived.curves.filter((curve) => props.groups[curve.group]);
  const xmax = createMemo(() => Math.max(0.001, ...curves().map((curve) => curve.coordinates.at(-1)?.[0] ?? 0)));
  const ymin = createMemo(() => -Math.max(1, Math.ceil(Math.log10(Math.max(1, ...curves().map((curve) => curve.n))))));
  const x = (value: number) => 74 + (symlog(value, 0.001) / symlog(xmax(), 0.001)) * 550;
  const ticks = createMemo(() => {
    const values: number[] = [];
    for (const value of [0, 0.0001, 0.001, 0.01, 0.1, 1]) {
      if (value <= xmax() && (!values.length || x(value) - x(values.at(-1)!) >= 44)) values.push(value);
    }
    return values;
  });
  const y = (value: number) => 20 + (Math.log10(Math.max(value, 10 ** ymin())) / ymin()) * 280;
  const paths = createMemo(() =>
    curves().map((curve) => ({
      ...curve,
      path: curve.coordinates.map(([a, b], i) => (i === 0 ? `M${x(a)},${y(b)}` : `V${y(b)}H${x(a)}`)).join(""),
    })),
  );
  return (
    <>
      <svg
        viewBox="0 0 660 365"
        class="data-svg"
        role="img"
        aria-label="C. Empirical bond-length error survival distributions"
        onPointerMove={(event) => {
          const bounds = event.currentTarget.getBoundingClientRect();
          const pointerX = ((event.clientX - bounds.left) / bounds.width) * 660;
          const values = curves().flatMap((curve) => {
            let low = 0,
              high = curve.coordinates.length;
            while (low < high) {
              const middle = (low + high) >>> 1;
              if (x(curve.coordinates[middle][0]) < pointerX) low = middle + 1;
              else high = middle;
            }
            const point = curve.coordinates[Math.min(low, curve.coordinates.length - 1)];
            return point
              ? [
                  `r=${GROUP_LABELS[curve.group]}: RMSE ${point[0].toPrecision(3)}, ${(point[1] * 100).toPrecision(3)}% ≥ x (n=${curve.n})`,
                ]
              : [];
          });
          setHover(values.join(" · "));
        }}
        onPointerLeave={() => setHover()}
      >
        <For each={Array.from({ length: 1 - ymin() }, (_, i) => -i)}>
          {(power) => (
            <g>
              <line x1="74" x2="624" y1={y(10 ** power)} y2={y(10 ** power)} class="grid-line" />
              <text x="63" y={y(10 ** power) + 4} text-anchor="end" class="axis-text">
                {power === 0 ? "1" : `10${superscript(power)}`}
              </text>
            </g>
          )}
        </For>
        <For each={ticks()}>
          {(value) => (
            <g>
              <line x1={x(value)} x2={x(value)} y1="20" y2="300" class="grid-line" />
              <text x={x(value)} y="323" text-anchor="middle" class="axis-text">
                {value === 0 ? "0" : value.toExponential(0)}
              </text>
            </g>
          )}
        </For>
        <For each={paths()}>
          {(curve) => <path d={curve.path} fill="none" stroke={GROUP_COLORS[curve.group]} stroke-width="1.8" />}
        </For>
        <path d="M74,20V300H624" class="axis-line" />
        <text x="350" y="359" text-anchor="middle" class="axis-title">
          Relative bond-length RMSE · symlog, linear below 0.001
        </text>
        <text transform="translate(16,160) rotate(-90)" text-anchor="middle" class="axis-title">
          Empirical fraction with RMSE ≥ x
        </text>
      </svg>
      <p class="chart-note h-9">{hover() ?? "L = median bond length"}</p>
    </>
  );
}
function superscript(value: number) {
  return String(value)
    .split("")
    .map(
      (char) =>
        ({
          "-": "⁻",
          "0": "⁰",
          "1": "¹",
          "2": "²",
          "3": "³",
          "4": "⁴",
          "5": "⁵",
          "6": "⁶",
          "7": "⁷",
          "8": "⁸",
          "9": "⁹",
        })[char] ?? char,
    )
    .join("");
}

export function AngleChart(props: { derived: Derived; groups: boolean[] }) {
  const [hover, setHover] = createSignal<string>();
  const boxes = () => props.derived.boxes.filter((box) => props.groups[box.group]);
  const max = createMemo(() =>
    boxes().reduce(
      (maximum, box) => box.outliers.reduce((value, outlier) => Math.max(value, outlier), Math.max(maximum, box.max)),
      1,
    ),
  );
  const x = (group: number) => 125 + group * 145;
  const y = (value: number) => 300 - (symlog(value, 1) / symlog(max() * 1.08, 1)) * 280;
  const paths = createMemo(() =>
    boxes().map((box) => ({
      ...box,
      path: box.outliers
        .map((value) => `M${x(box.group) - 1.5},${y(value)}a1.5,1.5 0 1,0 3,0a1.5,1.5 0 1,0 -3,0`)
        .join(""),
    })),
  );
  return (
    <>
      <svg viewBox="0 0 660 365" class="data-svg" role="img" aria-label="D. Chain-angle error box plots by block rank">
        <For each={[0, 1, 5, 10, 30, 60, 90].filter((value) => value <= max() * 1.08)}>
          {(value) => (
            <g>
              <line x1="74" x2="624" y1={y(value)} y2={y(value)} class="grid-line" />
              <text x="63" y={y(value) + 4} text-anchor="end" class="axis-text">
                {value}
              </text>
            </g>
          )}
        </For>
        <For each={paths()}>
          {(box) => (
            <g
              onPointerEnter={() =>
                setHover(
                  `r=${GROUP_LABELS[box.group]} · n=${box.n.toLocaleString()} · median ${box.median.toPrecision(3)}° · P95 ${box.p95.toPrecision(3)}°`,
                )
              }
              onPointerLeave={() => setHover()}
            >
              <rect x={x(box.group) - 65} y="0" width="130" height="325" fill="transparent" />
              <path d={box.path} fill={GROUP_COLORS[box.group]} opacity="0.4" pointer-events="none" />
              <line x1={x(box.group)} x2={x(box.group)} y1={y(box.min)} y2={y(box.max)} stroke="#111827" />
              <rect
                x={x(box.group) - 10}
                y={y(box.q3)}
                width="20"
                height={Math.max(1, y(box.q1) - y(box.q3))}
                fill="white"
                stroke="#111827"
              />
              <line
                x1={x(box.group) - 12}
                x2={x(box.group) + 12}
                y1={y(box.median)}
                y2={y(box.median)}
                stroke="#111827"
                stroke-width="2"
              />
              <path d={`M${x(box.group)},${y(box.p95) - 5}l5,5l-5,5l-5,-5Z`} fill={GROUP_COLORS[box.group]} />
              <text x={x(box.group)} y="14" text-anchor="middle" class="axis-text">
                n={box.n.toLocaleString()}
              </text>
              <text x={x(box.group)} y="323" text-anchor="middle" class="axis-text">
                {GROUP_LABELS[box.group]}
              </text>
            </g>
          )}
        </For>
        <path d="M74,20V300H624" class="axis-line" />
        <text x="350" y="359" text-anchor="middle" class="axis-title">
          Maximum cyclic-block rank
        </text>
        <text transform="translate(18,160) rotate(-90)" text-anchor="middle" class="axis-title">
          Per-molecule chain-angle RMSE (°) · symlog
        </text>
      </svg>
      <p class="chart-note h-9">{hover() ?? "Box: IQR · whiskers: 1.5 IQR · ◇: P95"}</p>
    </>
  );
}
