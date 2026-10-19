#import "/book.typ": book-page
#show: book-page

#let source-root = "https://github.com/chitin-dev/chitin/blob/main/crates/chitin-chemdraw/src"

= 1 2D chemical drawing pipeline
<chemdraw-overview>

`chitin-chemdraw` converts a borrowed `chitin-chem::MolecularGraph` into a
validated, frontend-independent scene. It owns geometric layout and depiction,
not windows, selection, editing history, or rendering-device state.

== 1.1 Pipeline stages
<chemdraw-pipeline>

```text
MolecularGraph
      -> layout::generate
      -> Layout2d
      -> depict::depict
      -> Scene
      -> svg::export or another renderer
```

The #link(source-root + "/layout/mod.rs")[layout module] produces normalized
coordinates with nominal unit bonds. The #link(source-root + "/depict.rs")[depict
module] converts chemical facts into line, wedge, label, and color primitives.
The #link(source-root + "/scene.rs")[scene module] validates bounds, colors,
source associations, and geometry before export.

== 1.2 Geometry versus chemistry
<geometry-chemistry-boundary>

The graph remains the source of truth. Layout may choose orientations, reflect
movable subtrees, or refine projected coordinates, but it does not add atoms,
remove bonds, change bond orders, infer missing stereo, or mutate graph facts.
Every emitted primitive retains the atom or bond identity that produced it.

Nominal bond length is one drawing unit. A geometric layout is accepted only
when its required closure, crossing, stereo, and spacing constraints pass. When
an optional search cannot finish, the last complete validated candidate is
retained and a diagnostic may be reported; a partial candidate is never
published.

== 1.3 Depiction responsibilities
<depiction-responsibilities>

Depiction clips bond lines around atom labels, emits ordinary and multiple-bond
strokes, handles aromatic circles or integer-bond forms, and constructs solid
and hashed stereo wedges from final coordinates. Label collision repair may
translate printed text, but its padded envelope must still contain the original
atom anchor. A label is never disconnected from its chemical endpoint merely to
remove visual overlap.

Perspective metadata is explicit. Authored cage crossings can expose front and
rear source bond IDs, while arbitrary intersections do not receive an automatic
over/under interpretation. SVG export serializes the validated scene without
reinterpreting chemistry.

== 1.4 Supported boundary
<chemdraw-boundary>

The current layout includes regular small rings, compatible fused systems,
selected macrocycles, a macrocycle fused to one small face, bicyclic bridge-path
models, selected saturated cage templates, long-backbone routing, and bounded
projected refinement. These are explicit candidate families, not a universal
embedding solver or a physical conformer generator.

Dense substitution, unsupported cage topology, incomplete cyclic stereo, and
unresolved crossings remain diagnostics or typed errors. The result is a
readable 2D drawing with traceable limitations, not a claim of global
collision-free optimality.

== 1.5 Public example
<chemdraw-example>

```rust
use chitin_chemdraw::{depict, layout, svg};

let graph = chitin_chem::io::smiles::read_smiles("c1ccccc1")?.into_graph();
let coordinates = layout::generate(&graph, &Default::default())?;
let scene = depict::depict(&graph, &coordinates, &Default::default())?;
let output = svg::export(&scene, &Default::default())?;
# let _ = output;
# Ok::<(), Box<dyn std::error::Error>>(())
```
