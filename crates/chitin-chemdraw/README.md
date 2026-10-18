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
six-membered systems. Nonmanifold face incidence and incompatible regular-polygon
closures still return `LayoutError::RingSystem`.

Simple macrocycles of at least 12 atoms use closed nonregular outlines rather
than nearly straight regular-polygon corners. Even rings retain their lattice
candidates first. Five-/seven-sided end caps and hexagonal extensions provide
odd sizes; six-/eight-sided caps diversify even turn patterns. The cap faces
are temporary construction geometry, not additional chemical rings or bonds.
All edges stay unit length, with exact closure and visible bends. Cyclic offsets
and reversed traversals are checked against all annotated ring double bonds
together. Unsatisfied configurations are rejected, never traded for a lower
crowding score. External attachments use the free sector opposite the two ring
rays, including at concave corners.

A separate block model supports one macrocycle edge-fused to one 3–8-membered
regular face. It checks actual local interior sides, polygon overlap, skeleton
crossings, and fully known source stereo references before committing the block.
One entry anchor in either face is retained exactly. Multiple fused caps,
macrocycle-to-macrocycle fusion, general cage embedding, and all possible cyclic
stereo configurations are not covered by this candidate family. Construction,
validation and scoring share the ring-search budget; exhaustion never publishes
a partial ring. Small rings below 12 retain their regular-polygon model.

Bicyclic bridged blocks use a separate path model: two degree-three bridgeheads
joined by three internally disjoint paths. A uniquely shortest path lies inside
two longer outer arcs; every edge retains unit length and both endpoints close
exactly. Multiple orientations and bridgehead separations are scored against the
component. Equal shortest paths and higher-rank cages remain unsupported by this
model; it does not invent 3D geometry or stereo from ring topology. Attachments
on an interior bridge can still produce crossings reported by quality diagnostics.

Substituents at fused junctions use the exterior gap between ring neighbors.
Only a separately connected, already placed parent occupies that gap; having
three ring neighbors does not shift a methyl back onto a ring bond. Regression
tests cover both steroid junction methyls and entered cyclic blocks with an
external parent, before and after refinement.

Discrete hydroxyl clearance rotates only terminal neutral O on saturated carbon,
within ±30° of its initial direction and with no new proper bond crossings.
Ring geometry, C–O length and source chemical facts remain fixed during this stage. It
has reserved work inside the existing refinement budget, so a large skeleton's
branch search cannot starve it. Carbonyl/aromatic hosts and ring oxygens are not
subject to this angle adjustment. Actual OH-label separation is regression-tested
for the large polycycle; general label-aware optimization is not yet implemented.

`LayoutOptions::ring_search_budget` bounds discovery and embedding work across
all components (2,000,000 units by default). Exhaustion returns
`RingSearchBudgetExceeded`, not an unsupported-topology error or partial result.
This is independent of `refinement_budget`, whose exhaustion retains a complete
best-so-far layout with a diagnostic. See [layout constraints](docs/layout.md).

The algorithm is not a physical force-field simulation, canonicalization algorithm,
template engine, or global collision-free optimizer. It returns diagnostics for
remaining crowding or crossings. Caller-supplied `Layout2d` coordinates can
bypass automatic layout restrictions.

### Projected coordinate refinement

After the discrete stages, crowded components with a cycle longer than eight
edges can use bounded projected descent. Ring closure and unit source bonds are
hard constraints (`optimization::BOND_TOLERANCE = 1e-9`); configured source
double-bond relations and explicit pins cannot be inverted or moved. Existing
small-cycle templates (3–8 edges) remain fixed. Multiple-bond centers and supplied
tetrahedral centers retain their local angle geometry. Other angles may flex by
at most 30 degrees from the stage's initial snapshot, with projected lower/upper
bounds preventing further needle corners or flattening. These are drawing
constraints, not equilibrium molecular bond angles.

The shared geometry score measures atom crowding, atom/edge clearance and proper
crossings. Soft angle and crossing-depth terms guide search; only a completely
validated improvement with no new crossing pair and a nonworse shared geometry
score can replace the input. There is no force-field energy, chemistry mutation,
new ring construction, or exact text-envelope optimization in this stage.

`LayoutOptions::optimization` controls its separate iteration, projection, and
work limits. Zero `iterations` or `work_budget` disables projected search;
`refinement_passes = 0` disables all optional automatic refinement as before.
Exhaustion adds `OptimizationBudgetExhausted` while retaining the best complete
result. The default is 64 iterations, 256 projection sweeps per trial, and
1,000,000 shared search work units.

For existing coordinates, call `layout::optimization::optimize` directly:

```rust
use std::collections::BTreeSet;
use chitin_chemdraw::layout::optimization::{optimize, OptimizationOptions};
# fn refine(graph: &chitin_chem::MolecularGraph, initial: &chitin_chemdraw::layout::Layout2d)
#   -> Result<(), Box<dyn std::error::Error>> {
let anchors = BTreeSet::new(); // Add graph-local AtomIds to fix exact positions.
let result = optimize(graph, initial, &anchors, &OptimizationOptions::default())?;
let coordinates = result.layout;
let report = result.report;
# let _ = (coordinates, report);
# Ok(()) }
```

The independent API requires exact live-ID coverage and unit initial bonds. It
does not orient or repack components and reports geometry costs, iterations, work
usage and budget exhaustion. Invalid initial double configurations are rejected,
not silently repaired. Its `max_atoms` bounds independent input validation;
automatic generation still uses `LayoutOptions::max_atoms`.

Inspect authored before/after examples with:

```sh
cargo run -p chitin-chemdraw --release --example optimize_coordinates -- \
  .temp/chemdraw-preview/projected-refinement
```

## Depiction

The default depiction supports:

- single, double, and triple bonds;
- tetrahedral solid/hashed wedges with bond provenance and label clearance;
- supported aromatic SMILES through shared, detached C/N/O/S Kekulé assignment;
- skeletal-carbon suppression;
- element, isotope, charge, and hydrogen labels;
- explicit hydrogen vertices;
- label-aware bond clipping;
- default element-colored labels and split-color bonds, with RGBA overrides;
- bundled Noto Sans font metrics and vector glyph outlines.

Unknown or unsupported chemical facts are rejected instead of silently
converted. General aromaticity perception, further aromatic elements/charges,
extended stereochemistry, radicals, complex text shaping, and full sanitization require
future domain support in `chitin-chem`. Read/write coverage is broader than
drawing coverage; see the [compatibility matrix](../chitin-chem/docs/smiles-compatibility.md).

### Element colors and split bonds

`DepictOptions::color_scheme` defaults to `ColorScheme::Element`, and
`bond_color_style` defaults to `BondColorStyle::Split`. Labels and each half of
a bond use their endpoint's element color. All strokes of double/triple bonds
share the original atom-axis midpoint, regardless of label clearance or stroke
shortening. Solid and hashed stereo marks follow the same policy without
changing their shape or stereochemical meaning. Equal colors need no split.
Scene lines carry independent caps: exterior ends remain round, while internal
color seams are flat and do not overlap, including translucent overrides.

Set `BondColorStyle::Uniform` for colored labels with entirely foreground bonds.
Set `ColorScheme::Monochrome` for the previous all-foreground appearance; its
palette overrides are ignored even when the split policy is selected.
The builtin white-page palette uses
O red (`#e53935`), N blue (`#3058d8`), S dark yellow (`#b59b00`), P orange
(`#d97706`), F/Cl/Br/I green (`#218c3c`), B pink (`#b45d8a`), and Si brown
(`#9c6b46`). Carbon, hydrogen and other unlisted elements retain `foreground`.
These opaque defaults are display choices, not physical element properties or
an automatically contrast-adjusted palette for every background.

Attached H, H counts, isotopes and charges follow the host label's color.
Explicit H graph vertices resolve as H, not as their neighbor's element.
Generated chiral H labels also resolve as H, and their virtual C–H wedges use
the carbon/H endpoint colors. Existing carbon suppression and chemical facts do not
change. In element mode, `element_colors: BTreeMap<Element, Color>` overrides
individual entries, including carbon/H, and supports alpha. In monochrome mode
the override map is ignored. `label_color(element)` exposes the resolved color.

```rust
use chitin_chem::Element;
use chitin_chemdraw::{Color, depict::{BondColorStyle, ColorScheme, DepictOptions}};

let mut options = DepictOptions {
  color_scheme: ColorScheme::Element,
  bond_color_style: BondColorStyle::Split,
  ..Default::default()
};
options.element_colors.insert(Element::Oxygen, Color {
  red: 220, green: 40, blue: 40, alpha: 255,
});
```

The single-molecule example accepts `--color-scheme element` and
`--bond-color-style split|uniform`, combinable with
`--explicit-chiral-hydrogens` before the SMILES argument:

```sh
cargo run -p chitin-chemdraw --example smiles_to_svg -- \
  --color-scheme element --explicit-chiral-hydrogens \
  'N[C@@H](C)C(=O)O' .temp/alanine-colored.svg
```

The batch tool exposes the same options in YAML:

```yaml
output_dir: generated
depict:
  color_scheme: element  # default; monochrome restores all-foreground output
  bond_color_style: split  # default; uniform keeps bonds in foreground
  element_colors:
    O: '#dc2828'        # exact element symbols; quote #RRGGBB / #RRGGBBAA
    N: '#3058d880'
smiles:
  - 'NCCO'
```

Run `cargo run -p chitin-chemdraw-batch -- path/to/config.yaml`. Relative output
paths resolve from the YAML's directory; add `--overwrite` to replace existing
numbered SVGs. The tool's own `generated/` directory is Git-ignored. Invalid
scheme names, element symbols or hex values fail configuration parsing before
an output directory is created. Only this transport layer parses YAML; neither
the chemical model nor SVG export needs to know how the palette was configured.

### Custom fonts and host-measured labels

The default is still bundled Noto Sans. `DepictOptions::font` may instead hold
immutable caller-supplied TrueType/OpenType bytes and a collection face index:

```rust
use chitin_chemdraw::{depict::DepictOptions, font::FontFace};

let font = FontFace::from_bytes(std::fs::read("labels.ttf")?, 0)?;
let options = DepictOptions { font, ..Default::default() };
# Ok::<(), Box<dyn std::error::Error>>(())
```

File I/O and family/style resolution belong to the application. The core never
searches installed fonts or silently falls back. Font bytes are shared, not
copied per glyph, and retained by output scenes. Measurement, centering, clipping,
text bounds and SVG outlines all use the same selected face. Element and separate
H anchors use actual measured envelopes rather than a default-font baseline
offset. SVG embeds glyph paths and therefore needs no matching installed font.
Missing glyphs, invalid faces and visible glyphs without supported outlines
produce explicit errors. The caller must have the rights needed to use its font.

Native byte-backed measurement currently maps literal glyphs without kerning or
complex-script shaping; variable fonts use their default instance. Select a
static style face/file or use a shaping host for other requirements. Font family
names alone cannot guarantee reproducible metrics. `FontFace` is now `Clone`,
not `Copy`; `data()` returns `Option<&[u8]>` because host fonts need not have bytes.

The single-molecule example accepts `--font-file labels.ttf` and optionally
`--font-index 0` before the SMILES. The batch tool accepts:

```yaml
depict:
  font:
    path: fonts/labels.ttf  # relative to the YAML file, not process CWD
    face_index: 0          # optional collection face index
  font_size: 17.6
smiles: ['[13NH2+]CO']
```

For a future typesetting/frontend host, use the two-stage API instead of
substituting its font after the drawing is already clipped:

1. Call `depict::measurement_requests(graph, options)` to obtain unique exact
   `TextRequest` values. Requests cover visible element, H, isotope, charge,
   subscript and separately expanded chiral-H runs; no placeholder font is needed.
2. Measure each literal run at its requested size with the resolved host font.
   Return `TextMetrics { advance, min, max }` in the same drawing units, with
   bounds relative to the left baseline and y increasing downwards. A raw
   width/height without baseline placement is not a sufficient measurement.
3. Build `FontFace::from_measurements(identity, entries)`, put it into the same
   options, then call `depict`. The graph, size and H policy must remain unchanged.
4. Render `Drawing` primitives in the host, using each text run's baseline,
   size, width and `font.host_identity()`. Resolve identity to exactly the
   font/style/features/fallback settings used during measurement. Host glyphs
   must be transformed into the supplied target width if it differs from advance.

Both paths use the same chemical label recipes and geometric code. Tables are
immutable and validate finite ordered bounds; missing or duplicate requests are
errors, not interpolation or fallback. Changed font settings require a new
identity/table; changed sizes or chemistry require collecting requests again.
Host-managed fallback/shaping is possible only when its measurements and final
drawing agree. The core does not invoke a host callback or retain mutable session
state, so the contract can be transported through a pure WASM byte interface.

Host-only fonts do not include outlines. SVG export of text backed by them
returns `OutlinesUnavailable`; it never emits a guessed `font-family`. Use the
host renderer or supply actual font bytes for standalone SVG. A Typst wrapper,
WASM transport, OS font discovery and general label-aware skeleton optimization
are not implemented by this font API.

### Aromatic inner circles

`DepictOptions::aromatic_style` defaults to `AromaticStyle::Kekule`, preserving
the existing alternating single/double depiction. `AromaticStyle::Circle`
requests a single-stroke perimeter and inner circle for independent simple
aromatic cycles, including supported heterocycles. Example/YAML options:

```yaml
depict:
  aromatic_style: circle  # or kekule, the default
  color_scheme: element  # palette for labels and split perimeter bonds
```

```sh
cargo run -p chitin-chemdraw --example smiles_to_svg -- \
  --aromatic-style circle --color-scheme element 'c1ccncc1' .temp/pyridine.svg
```

The original input must mark **every ring atom and ring edge as aromatic**.
Uppercase alternating bonds are not assigned aromaticity. Lowercase input with
explicit single/double ring edges also conservatively retains its Kekulé strokes.
Fused/shared-vertex rings fall back as whole blocks; rings connected only by
external bonds may each receive a circle. Chemical validation and label H counts
always use the existing detached integer model, even in circle mode. Invalid
chemistry is not made acceptable by a different display style.

The circle is fitted to a near-regular polygon in the supplied coordinates,
with radius 70% of the smallest center-to-edge distance. Distorted, concave or
star-like manual geometry, thick strokes, inward labels, unrelated bonds or
generated chiral-H marks can trigger a complete per-ring fallback. Geometry is
never moved to force a circle to fit; no per-circle radius setting is exposed.
The circle uses the bond foreground/width, independently of label colors. Inner
double strokes are suppressed only for a fully accepted circle, leaving external
carbonyls and other rings unchanged.

Generic scenes now provide `Circle` / `Primitive::Circle`, with stroke-inclusive
bounds, validation and SVG `<circle fill="none">` export. A chemical inner circle
uses `Source::Atom` with the ring's smallest atom handle as a source locator, not
as a newly invented chemical ring identifier.

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

let graph = read_smiles("CC(=O)Oc1ccccc1C(=O)O")?.into_graph();
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
`chitin-chem`; unsupported SMILES features remain explicitly rejected.
Paired slash/backslash markers constrain relative double-bond reference sides.
Automatic layout reflects complete acyclic sides without stretching bonds;
optional overlap refinement cannot invert these constraints. Cyclic geometry
is accepted only if the ring seed satisfies the supplied configuration;
supported macrocycles search nonregular candidates for cis/trans relations.
Other cyclic mismatches still return an error. `LayoutOptions::stereo_budget` bounds
required stereo work independently of optional quality refinement. Depiction
rejects caller coordinates that flatten or invert an annotated double bond.
Partial/unpaired directions and cumulated stereo remain unsupported; no CIP
E/Z labels are assigned. Supplied aromatic facts are retained in the source graph; layout and
depiction share a detached integer-bond adapter, not separate display guesses.
Future work includes broader aromatic chemistry, symmetric/cage bridge layouts, exact label-aware refinement, further stereo
depiction, complex text shaping, and raster export.

Tetrahedral `@`/`@@` and `@TH1`/`@TH2` preserve relative configuration through
SMILES writing and reflected/rotated coordinates. Wedge direction is computed
from final geometry, not directly from token spelling. Ordinary chiral carbon
and its declared H remain skeletal by default; isotope/charge labels remain visible.
`DepictOptions::wedge_width` controls the broad end (default 6.72 drawing units),
independently of `stroke_width` (default 1.6). The default H model is now
`SmilesOrganicV1`, including normal valences for nonaromatic sulfur/phosphorus;
the older restricted V1 remains selectable. Degenerate stereo or centers without
an independent nonshared wedge return typed errors; CIP labels, cage-stereo
rendering, E/Z and extended classes are not claimed as supported.

### Displaying chiral carbon hydrogens

For fused rings or other structures where a visible H makes the configuration
clearer, opt into depiction-only expansion:

```rust
use chitin_chemdraw::depict::{ChiralHydrogenMode, DepictOptions};

let options = DepictOptions {
  chiral_hydrogens: ChiralHydrogenMode::Explicit,
  ..Default::default()
};
```

Each annotated `[C@H]`/`[C@@H]` receives one H label and a C–H solid/hashed wedge,
instead of another wedge on an existing bond. Four-explicit-ligand centers,
unannotated CH groups, other elements, and actual `[H]`/`[2H]` vertices are not
expanded. Host isotope/charge labels stay visible without repeating the H.
Glyphs and marks use `Source::AttachedHydrogen(center)` rather than fictitious
atom/bond IDs; the graph, serialized facts, and layout remain unchanged.

Candidate directions derive from local bond vectors, with bounded label/bond
clearance scoring and H-bond lengths of 0.55–0.85 times the normal bond length.
Final coordinates determine wedge polarity; placement never changes molecular
configuration. This local display policy does not repair crowded skeletons or
guarantee a globally collision-free drawing. Degenerate coordinates or completely
obscured C–H marks fail explicitly.

The example exposes the same policy as an optional leading flag; existing
two-positional-argument usage retains its original default:

```sh
cargo run -p chitin-chemdraw --example smiles_to_svg -- \
  --explicit-chiral-hydrogens 'N[C@H](C)C(=O)O' alanine.svg
```

## Validation

```sh
cargo test -p chitin-chem -p chitin-chemdraw --locked --offline
cargo clippy -p chitin-chemdraw --all-targets --locked --offline -- -D warnings
cargo fmt --all --check
```

The fixed corpus in `tests/fixtures/layout.tsv` covers linear/angular fused rings,
mixed five-/six-membered rings, acid, ester, amide, nitrile, and substituted fused
systems, as well as odd/even macrocycles, combined double-bond constraints and
single-cap fused macrocycles. It is shared by geometry tests and the visual export example:

```sh
cargo run -p chitin-chemdraw --example layout_regressions -- \
  .temp/chemdraw-preview/fused-regressions
```

Inspect the generated SVGs as well as the tests. Other regressions cover multi-
anchor closure, incompatible closures, spiro/linked attachments, sparse handles,
bond insertion-order changes, shared budgets, and an iterative 128-ring strip.

## Inspired by

The macrocycle candidate strategy is informed by
[typed-smiles](https://github.com/GeronimoCastano/typed-smiles): separate closed
outline construction, cyclic assignment, and stereochemical validation. Chitin's
implementation uses its own immutable graph models, exact unit-edge contract,
explicit work budgets, and typed failures; no external engine is invoked.
