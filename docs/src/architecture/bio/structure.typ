#import "/book.typ": book-page
#show: book-page

#let source-root = "https://github.com/chitin-dev/chitin/blob/main/crates/chitin-bio/src"

= 1 Structure model contract
<structure-model-contract>

This page defines the scientific contract shared by parsers, analysis, and
renderers. It describes what the model means, not how a particular crate stores
or uploads it.

== 1.1 Topology and coordinate states
<topology-and-coordinate-states>

The logical hierarchy is expressed by ownership: a structure contains models,
models reference coordinate sets and chains, chains contain residues, and
residues contain atoms. Bonds connect atoms and are independent of the visual
representation selected by a consumer.

Topology contains atom identity, membership, and bonds. A coordinate state
contains positions and other values that may vary between models. Changing the
active coordinate state must not change atom identifiers or silently rewrite
source topology.

The corresponding implementation types are defined in
#link(source-root + "/structure/model.rs")[`structure/model.rs`]. The builder
that aligns model coordinates with stable atom identities is in
#link(source-root + "/structure/builder.rs")[`structure/builder.rs`].

== 1.2 Units and coordinate systems
<units-and-coordinate-systems>

Cartesian coordinates use ångströms and a right-handed frame. Fractional
coordinates use the unit-cell basis:

$ bold(x) = A bold(f) $

Here $bold(x)$ is a Cartesian position, $bold(f)$ is a fractional position, and
$A$ is the basis matrix derived from the unit-cell lengths and angles.

For edge lengths $a$, $b$, and $c$, and angles $alpha$, $beta$, and $gamma$, a
conventional triclinic basis can be written as:

$ A = mat(delim: "[", a, b cos(gamma), c cos(beta); 0, b sin(gamma), c v; 0, 0, c w) $

where:

$ v = (cos(alpha) - cos(beta) cos(gamma)) / sin(gamma) $

$ w = sqrt(1 - cos(beta)^2 - v^2) $

Valid cells have positive edge lengths, angles strictly between $0 degree$ and
$180 degree$, and a non-singular basis. The cell volume is:

$ V = abs(det(A)) > 0 $

These equations define coordinate conversion only. Reading unit-cell metadata
does not automatically expand symmetry mates or duplicate atoms.

== 1.3 Identifiers and provenance
<identifiers-and-provenance>

Author and label identifiers are separate namespaces. Alternate locations,
model numbers, source serials, and optional experimental values are preserved
when present.

Every bond records whether it came from source connectivity or geometric
inference. Derived geometry must remain traceable to the atom identities that
produced it. This allows selection and diagnostics to move between a rendered
object and the source structure without relying on a floating-point position.

== 1.4 Failure boundary
<failure-boundary>

The parser rejects malformed values and invalid references with a source field,
record, or row location. A renderer may reject unsupported visual data, but it
must not be responsible for discovering broken scientific invariants.

The public parse result combines the validated structure with recoverable
diagnostics. The error and diagnostic types are defined in
#link(source-root + "/structure/error.rs")[`structure/error.rs`].

This boundary keeps error handling local: format errors belong to the reader,
relationship errors belong to structure construction, and GPU or presentation
errors belong to the renderer.
