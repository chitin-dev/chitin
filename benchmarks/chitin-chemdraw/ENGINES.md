# Coordinate-engine comparisons

The optional `rdkit_coordgen` provider generates coordinates only. Both engines
use Chitin's source graph, depiction options, font, SVG export and unmodified
`diagram-quality-v3` evaluator. This is **not** an evaluation of RDKit's drawing
engine. No SVG parsing or screenshot-based scoring is involved.

## Reproduce

From the repository root:

```sh
uv sync --locked --project benchmarks/chitin-chemdraw/engines/coordgen
cargo run --release --locked -p chitin-chemdraw-benchmark -- benchmarks/chitin-chemdraw/coordgen-10k.yaml
```

`coordgen-10k.yaml` and `coconut-10k.yaml` select the same COCONUT source rows
with the same input, sample size and seed. Use fresh output paths for repeats.
For a layout-only comparison, keep `label_feedback: false` in **both** runs. An
automatic Chitin pipeline with feedback can be an additional, separately
identified comparison, but is not the identical fixed-coordinate pipeline.

Configuration additions:

```yaml
layout_engine: rdkit_coordgen
external_python: engines/coordgen/.venv/bin/python
coordgen_precision: standard
label_feedback: false
```

Paths are relative to the YAML file. Existing configs default to `chitin` and do
not require Python. Precision accepts `coarse`, `quick`, `standard` or `best`;
`standard` is explicit rather than relying on the wrapper's coarse default.

## Evaluation boundary

### Default native fragment engine

`layout_engine: chitin` now selects the sole native Rust generator. The old
backend was removed. `chitin_fragmented` is accepted as a configuration alias,
not a separate engine. See `native-default-10k.yaml` for the fixed COCONUT sample;
keep `label_feedback: false` when comparing coordinate engines alone.

Polygons, templates, constrained even/odd macrocycles, detached ring opening,
fragment search and stereo validation never invoke an older generator or closure
fallback. Unsupported geometry and work exhaustion fail explicitly. The default
entry reproduces all 10,000 statuses, errors and quality measurements from
`fragment-port-10k-009`: 9,854 generated and 9,215 readability-clear. This is
backend replacement, not completed reference parity; known failures remain.

Use a fresh output directory after algorithm changes. Resume and lazy web previews
retain the frozen executable and historical request flag, including migration-era
flags needed by older executables. Historical runs are not rewritten to use the
new default. Their frozen executables may still contain retired implementations.

### External coordinate provider

- Direct `rdCoordGen.AddCoords` invocation; no silent default-layout fallback.
- Original source-order atoms, explicit H vertices, isotope/charge/H
  declarations and original bonds are validated against Chitin's parsed graph.
- Sanitization that changes atomic facts or nonaromatic bond orders is rejected.
  Aromaticity perception of an explicitly Kekulized ring is permitted.
- One positive global scaling sets the median bond length to one. No packing,
  angle repair, branch movement, label feedback or internal layout fallback
  runs.
- The exact same skeleton, label and stroke metrics are evaluated, including
  large-cycle turns, cavities and footprint. Invalid imported geometry and final
  depiction failures remain failures in the full sampled denominator.
- The shared depiction engine is a controlled factor, not a neutral renderer:
  final-scene results measure **external coordinates plus Chitin drawing**.
- Independent tetrahedral depiction decoding remains unavailable for both.
- External coordinates are planar and contain no authored depth/crossing-order
  metadata. No intentional crossing is inferred merely to lower a score. For
  cages and bridges, compare planar-only strata separately from Chitin's
  explicitly annotated perspective layouts; aggregate crossing counts are not
  profile-neutral.

The manifest records interpreter and provider versions/build, module and bundled
coordinate-library hashes, precision, renderer identity and the metric-spec
hash. The underlying library's standalone version is `null` when not exposed;
the wrapper version must not be presented as that library's version. The YAML
seed selects source rows only; it is not an external coordinate seed. The
coordinate API exposes no seed here. Exact historical replay uses the saved
coordinates, not an assumption that another provider invocation is identical.

## Resume an interrupted run

```sh
cargo run --release --locked -p chitin-chemdraw-benchmark -- benchmarks/chitin-chemdraw/coordgen-10k.yaml --resume
```

Add `--check` to validate without changing any checkpoint files. `--output DIR`
can select a relocated run; paths remain relative to the YAML file.

Resume validates configuration, input, metric specification, frozen renderer and
external provider identity. Workers use the run's saved `renderer`, not the
newly compiled algorithm. A changed provider or adapter is rejected; restore the
original environment or start a new output directory.

Every complete newline-terminated `cases.jsonl` record is finished, including
errors and timeouts. Statistics are rebuilt from those records; only the pending
case and following samples execute. Completed observations and SVGs are
retained. Uncommitted journal tails and pending SVGs are backed up under
`recovery/` before reconciliation. Corrupt complete records are rejected, not
silently discarded. An already completed run only rebuilds statistics.

An advisory lock prevents simultaneous new controllers. Stop older controllers
that predate locking before resuming their output directory. Resume is for
process interruption, not a guarantee of recovery from storage failure.

Provider import, parsing and coordinate generation have separate timings.
Per-case wall time includes both cold processes, exchange and QA; compare the
coordinate stage separately from Python import overhead. One deadline covers
provider execution and the Rust evaluator. Each child is terminated and reaped
directly; no nested chemistry subprocess is left behind on timeout.

Validated source-order coordinates are stored in every imported case record.
Plots can replay them with the frozen Rust renderer without an external runtime,
including rows outside the bounded SVG artifact sample. Provider runs are marked
in the picker and inspector; current-internal-layout substitution is prohibited.

Sources:
[coordinate API](https://www.rdkit.org/docs/source/rdkit.Chem.rdCoordGen.html),
[integration examples](https://www.rdkit.org/docs/Cookbook.html#using-coordgen-library).
