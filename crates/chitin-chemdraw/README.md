# chitin-chemdraw

Frontend-independent 2D molecular layout, depiction, and SVG export. This crate
produces drawing data; it is not a GPUI widget or an editor.

## Pipeline

```text
chitin-chem::MolecularGraph
        -> layout::generate
        -> depict::depict
        -> svg::export
```

- `layout` creates normalized x-right/y-up coordinates.
- `depict` converts chemical facts and coordinates into generic line and text
  primitives.
- `scene` validates geometry, text, colors, bounds, and source associations.
- `svg` exports the validated scene without reinterpreting chemistry.

The source graph is never mutated. Atom and bond IDs remain graph-local handles.
Selection, editing, undo, windows, and UI state belong to consuming
applications.

## Layout

The automatic layout is a deterministic, rule-based graph embedding algorithm.
It uses biconnected ring blocks, regular polygon seeds, iterative branch
expansion, and bounded local refinement:

- single bonds use unit lengths and constrained angle slots;
- triple and cumulated-double centers remain linear;
- branches are placed with local overlap and crossing penalties;
- ring systems are embedded through shared vertices and edges;
- disconnected components are oriented and packed horizontally.

Fused systems are assembled as complete blocks, rather than being limited to
two rings. Short independent cycles define polygon constraints; faces sharing
an edge must occupy opposite sides, and every already known anchor must agree.
This supports linear/angular six-membered systems and compatible mixed five-/
six-membered systems. Bridged/shared-path blocks, nonmanifold face incidence,
or incompatible regular-polygon closures still return `LayoutError::RingSystem`.

`LayoutOptions::ring_search_budget` bounds discovery and embedding work across
all components (2,000,000 units by default). Exhaustion returns
`RingSearchBudgetExceeded`, not an unsupported-topology error or partial result.
This is independent of `refinement_budget`, whose exhaustion retains a complete
best-so-far layout with a diagnostic. See [layout constraints](docs/layout.md).

The algorithm is not a force-directed simulation, canonicalization algorithm,
template engine, or global collision-free optimizer. It returns diagnostics for
remaining crowding or crossings. Caller-supplied `Layout2d` coordinates can
bypass automatic layout restrictions.

## Depiction

The default depiction supports:

- single, double, and triple bonds;
- skeletal-carbon suppression;
- element, isotope, charge, and hydrogen labels;
- explicit hydrogen vertices;
- label-aware bond clipping;
- monochrome colors with configurable alpha;
- bundled Noto Sans font metrics and vector glyph outlines.

Unknown or unsupported chemical facts are rejected instead of silently
converted. Aromaticity, stereochemistry, radicals, complex text shaping, and
full sanitization require future domain support in `chitin-chem`.

## SVG export

SVG export consumes only the validated `Drawing` scene. It provides:

- transparent or colored backgrounds;
- configurable document padding;
- XML escaping;
- accessible literal text labels;
- deterministic outlined glyphs without system-font dependencies.

## Example

```rust
use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{depict, layout, svg};

let graph = read_smiles("CC(=O)OC1=CC=CC=C1C(=O)O")?.into_graph();
let coordinates = layout::generate(&graph, &layout::LayoutOptions::default())?;
let drawing = depict::depict(&graph, &coordinates, &depict::DepictOptions::default())?;
let document = svg::export(&drawing, &svg::SvgOptions::default())?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Run the example from the workspace root:

```sh
cargo run -p chitin-chemdraw --example smiles_to_svg -- \
  'CC(=O)OC1=CC=CC=C1C(=O)O' aspirin.svg
```

## Scope

This is an initial depiction backend, not a complete cheminformatics toolkit.
Automatic layout and depiction currently depend on the capabilities of
`chitin-chem`; unsupported SMILES features such as aromatic and stereo syntax
remain rejected. Future work includes bridged-ring layout, exact label-aware refinement, stereo-aware
depiction, complex text shaping, and raster export.

## Validation

```sh
cargo test -p chitin-chem -p chitin-chemdraw --locked --offline
cargo clippy -p chitin-chemdraw --all-targets --locked --offline -- -D warnings
cargo fmt --all --check
```

The fixed corpus in `tests/fixtures/layout.tsv` covers linear/angular fused rings,
mixed five-/six-membered rings, acid, ester, amide, nitrile, and substituted fused
systems. It is shared by geometry tests and the visual export example:

```sh
cargo run -p chitin-chemdraw --example layout_regressions -- \
  .temp/chemdraw-preview/fused-regressions
```

Inspect the generated SVGs as well as the tests. Other regressions cover multi-
anchor closure, incompatible closures, spiro/linked attachments, sparse handles,
bond insertion-order changes, shared budgets, and an iterative 128-ring strip.
