# chitin-chemdraw

Frontend-independent chemical layout and depiction. This crate produces drawing
structure data and exports SVG; it is not a GPUI widget, a sketcher document,
or a terminal/command-bus feature. PNG is a planned scene consumer, not an
implemented export backend in this first version.

## Pipeline and ownership

```text
chitin-chem MolecularGraph
    -> layout::generate -> Layout2d
    -> depict::depict   -> Drawing
    -> svg::export     -> SVG string
```

- `chitin-chem` owns elements, topology, H/charge/radical facts, SMILES parsing,
  and named chemical perception models. It does not depend on this crate.
- `layout` owns only coordinates and placement-quality diagnostics. Automatic
  layout can be bypassed with `Layout2d::from_positions`.
- `depict` chooses chemical labels, skeletal-carbon suppression, isotope/charge
  annotation, parallel bond strokes, and clearance around labels. The H model
  is selected explicitly in `DepictOptions`; its default is the restricted
  `MainGroupClosedShellV1` model, not general chemical sanitization.
- `scene` owns immutable, validated generic line/text primitives, colors,
  stroke/text envelopes, and optional `Source::Atom`/`Source::Bond` associations.
  Chemical interpretation is absent from the scene constructor and exporters.
  Text carries a bundled font identity so consumers can share exact metrics.
- `svg` consumes `Drawing` without consulting a molecular graph. A future PNG
  backend or frontend adapter must consume that same scene rather than duplicate
  chemical depiction rules.

Selection, windows, event handling, tool state, edit commands, and undo belong
to consumers. No stage mutates molecular input, installs property caches,
invokes RDKit/Indigo/OpenBabel, or reads files below `.temp/projects` at runtime.

## Coordinate and source identity contract

`Layout2d` uses x-right/y-up normalized coordinates; the automatic model gives
each bond length 1. `DepictOptions::bond_length` converts one such unit into
drawing units (48 by default), while depiction reverses y exactly once.
`Drawing` and SVG are x-right/y-down. There is no hidden renderer/viewport scale.

Bounds on `Layout2d` cover atom centers only. Bounds on `Drawing` include stroke
widths and text envelopes; SVG adds only document padding/background. Empty
coordinates/scenes retain `None` bounds; an empty SVG uses a 1x1 content frame.

Atom/bond IDs remain graph-local handles, not dense offsets or persistent global
identities. Coordinate maps and source associations must use the same graph
namespace. Depiction requires exact live-atom coverage, rejecting missing or
extra positions. It deliberately does not forbid reuse of existing coordinates
after topology edits; the caller owns that decision. A separately created graph
with numerically identical handles cannot be distinguished by those IDs alone.

## Initial automatic layout profile

Supported input: trees, simple single-cycle components with arbitrary tree
attachments, isolated atoms, and multiple disconnected components. Each connected
component must have cycle rank `E - V + 1 <= 1`. This is stricter than merely
rejecting fused rings: connected biphenyl/two separate ring systems are not yet
automatically supported. Supplying coordinates bypasses this automatic-model
restriction, not the depiction representation checks.

The independent algorithm:

1. Build a dense temporary adjacency view without changing source handles.
2. Peel leaves to isolate cycle cores; reject unsupported multi-cycle components.
3. Embed one cycle as a regular polygon, or seed a tree at a stable endpoint.
4. Expand branches iteratively with zigzag chain directions, linear degree-two
   triple/cumulated-double centers, and outward cycle attachments.
5. Choose among at most six sibling-group rotations using a unit-cell spatial
   index and local center-overlap penalties.
6. Translate component center bounds into a horizontal strip, preserving internal
   geometry and the configured component gap.

This is bounded greedy placement, not a global energy minimum, template engine,
or crossing-free/label-collision-free guarantee. Unresolved nearby atom centers
produce `LayoutDiagnostic::CrowdedAtoms`; clients must inspect diagnostics.
Inter-fragment packing currently uses center bounds, not final text envelopes.
Large labels may require a larger configured component gap or caller coordinates.
The search does not enforce tetrahedral/alkene stereo constraints or ring strain.

Grid queries keep typical low-degree placement local, and traversal is iterative.
Ordered-map lookups/sorting add logarithmic overhead; sibling scoring can still
be quadratic in a high-degree vertex's number of children. The default 4096-atom
budget is explicit and configurable. Exceeding a budget returns an error rather
than returning a truncated molecule or pretending a partial layout is complete.

## Depiction and export profile

- Single/double/triple bonds; unknown/aromatic labels are rejected. Explicit
  Kekule single/double rings work without claiming aromaticity perception.
- Neutral, unlabelled, inferred-H skeletal carbons are suppressed unless isolated.
  Bracket-state, charged, isotope-labelled, or declared-H carbons remain visible.
- Element symbols, declared/fixed H, inferred H when the selected model resolves
  it, isotope superscripts, H-count subscripts, and charge superscripts.
- Explicit and isotope-labelled H vertices remain vertices. Neighbor H are not
  added again to attached-H labels. Unknown charge/declared H is not defaulted
  to zero. Known nonzero radical counts are rejected pending radical depiction;
  an unknown radical state remains unspecified, never invented as a dot or zero.
- Lines are clipped against their own endpoint-label envelopes, including
  clearance and stroke radius. Multiple strokes share one axial label-clearance
  interval, preventing staggered ends at diagonal labels. Fully occluded bonds are errors,
  not silently missing/inverted strokes.
- Monochrome color/alpha, transparent or colored SVG backgrounds, XML escaping,
  and finite-geometry checks. Literal text cannot inject SVG markup.

## SmilesDrawer-style depiction

The local SmilesDrawer `SvgDrawer.drawEdge` and `DrawerBase.defaultOptions` are
the visual reference, not an executable dependency or a pixel-identical oracle.
At the default 48-unit bond length, the reference proportions are:

- Stroke width `bond_length / 30`, label size `bond_length * 11 / 30`.
- Double-bond spacing `bond_length * 0.17`.
- An offset auxiliary double stroke retains 0.8 of the axis length, unless
  neighbor intersection or label clearance requires more shortening.
- Triple side strokes are offset by `bond_spacing / 1.5`.

Ring doubles keep a complete skeleton edge and a short inward auxiliary stroke.
Acyclic doubles use centered pairs for isolated/branched-terminal and cumulated
contexts, otherwise an axis plus a short side stroke. Neighbor-ray miters join
centered carbonyl legs without drawing into the wrong ray extensions. The
geometry lives in `depict::bonds`, not in SVG serialization.

Bridge identification is iterative and shared across bonds; cyclic doubles use
a shortest alternate path to obtain a local cycle. This is not the reference's
full largest/aromatic-common-ring catalogue. Manual multi-cycle/concave layouts
therefore do not claim complete SmilesDrawer ring-choice parity. Cyclic search
cost is O(Dc * (V + E)) after bridge identification, where Dc is cyclic double
count. Acyclic side-count tie breaks may also scan the supplied graph. This is
not a canonical depiction algorithm or an automatic layout expansion.

Text uses bundled **Noto Sans Regular**, an unmodified SIL-OFL font similar in
role to the reference's Arial/Helvetica sans-serif stack, not the exact same
typeface. `font` obtains real advances and glyph bounds using the general
`ttf-parser` library. Layout envelopes and `Text` primitives retain this face
identity; SVG exports its vector outlines with accessible literal labels.
No installed fonts, `textLength` support, browser DOM measurement, or platform
font fallback are required. Horizontal attached-H direction follows neighboring
geometry; vertical text stacking and full complex-script shaping are not yet
implemented. Missing glyphs are errors rather than replacement boxes. Negative
charge uses the supported ASCII minus glyph, not a hidden U+2212 font fallback.

The default remains monochrome; no gradients or per-element coloring were added.
Font provenance, checksum, and the retained license are in [assets](assets/README.md).

## Example

```rust
use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{depict, layout, svg};

let graph = read_smiles("CC(=O)OC1=CC=CC=C1C(=O)O")?.into_graph();
let coordinates = layout::generate(&graph, &layout::LayoutOptions::default())?;
// Inspect coordinates.diagnostics() before presenting a layout as final.
let drawing = depict::depict(&graph, &coordinates, &depict::DepictOptions::default())?;
let document = svg::export(&drawing, &svg::SvgOptions::default())?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Run the file-writing example from the workspace root:

```sh
cargo run -p chitin-chemdraw --example smiles_to_svg -- \
  'CC(=O)OC1=CC=CC=C1C(=O)O' aspirin.svg
```

An aromatic lowercase input such as `c1ccccc1` still fails in the current
`chitin-chem` reader. Do not silently dearomatize it or ignore stereo tokens to
make depiction appear to succeed.

## References and next stages

Local reference study considered SmilesDrawer's staged positioning/overlap
handling, COSMolKit's detached coordinate/depicter boundaries, and ChemCanvas's
incremental coordinate extension. This implementation is independently written;
no algorithm source, templates, or test corpora were copied from those projects.
The separately licensed font asset is documented above and retains its license.
ChemCanvas's GPL source is not incorporated into this Apache-2.0 crate.

Next: stronger collision diagnostics and complex text shaping; ring-system analysis
and fused/spiro embedding; bridged-ring constraints/templates; shared raster
export; and aromatic/stereo depiction after the chemical model retains those
semantics. Paths and richer scene primitives should be added when an actual
depicter/export feature needs them, rather than by creating a speculative UI API.

## Validation

```sh
cargo test -p chitin-chem -p chitin-chemdraw --locked --offline
cargo clippy -p chitin-chemdraw --all-targets --locked --offline -- -D warnings
cargo fmt --all --check
RUSTDOCFLAGS='-D warnings' cargo doc -p chitin-chemdraw \
  --no-deps --document-private-items --locked --offline
```

Pipeline tests check source immutability, unit-bond geometry, cycle connectivity,
linear centers, sparse handle provenance, fragment separation, clipping/stroke
counts, H/isotope/charge labels, malformed options/geometry, XML escaping, empty
input, unsupported topology, and iterative large-chain placement. Exported SVG
examples should also be parsed as XML and rasterized for visual inspection;
string checks alone do not establish depiction quality.
