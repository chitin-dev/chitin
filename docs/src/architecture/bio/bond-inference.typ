#import "/book.typ": book-page
#show: book-page

#let source-root = "https://github.com/chitin-dev/chitin/blob/main/crates/chitin-bio/src"

= 1 Geometric bond inference
<geometric-bond-inference>

Structure files often omit ordinary covalent bonds. Chitin therefore combines
explicit connectivity with a conservative geometric inference step when a
consumer needs complete visual connectivity.

The implementation is in
#link(source-root + "/chemistry/bond_inference.rs")[`chemistry/bond_inference.rs`].
It receives atoms from one selected coordinate state and returns inferred bonds
with explicit provenance. It never overwrites source topology.

== 1.1 Distance criterion
<distance-criterion>

For atoms $i$ and $j$, with positions $bold(x)_i$ and $bold(x)_j$, their
separation is:

$ d_(i j) = |bold(x)_i - bold(x)_j| $

A candidate is accepted only when:

$ d_min < d_(i j) <= t(e_i, e_j) $

Here $e_i$ and $e_j$ are normalized elements, $d_min$ excludes coincident
coordinates, and $t$ is the element-pair distance threshold. The threshold uses
known element-pair values when available and otherwise falls back to covalent
radii:

$ t(e_i, e_j) = (r_i + r_j) / 1.95 $

Distance alone cannot reliably determine whether a connection is single,
double, triple, or aromatic. Inferred bond order therefore remains unknown
unless stronger source evidence is available.

== 1.2 Compatibility rules
<compatibility-rules>

Two atoms in different explicit alternate conformations must not be connected.
Hydrogen–hydrogen pairs are excluded. A pair already present in source
connectivity is not inferred again. These rules keep the result useful for
visualization without pretending to solve complete chemical perception.

The inference configuration and result types are
#link(source-root + "/chemistry/bond_inference.rs")[`BondInferenceConfig`] and
#link(source-root + "/chemistry/bond_inference.rs")[`InferredBond`]

== 1.3 Spatial locality
<locality>

Testing every atom pair requires:

$ N (N - 1) / 2 in O(N^2) $

The implementation instead partitions space into cells whose width is the
maximum search distance. An atom visits its own cell and the immediately
adjacent cells on each axis. This limits candidate generation to local pairs
while preserving every pair that could satisfy the distance criterion.

The spatial-grid proof obligation is also documented in the implementation
tests. The grid is an acceleration structure only; it must not change the
acceptance result compared with an exhaustive search.

== 1.4 Provenance and scope
<scope>

Inference is a fallback for missing topology. It does not replace component
dictionaries, valence models, aromaticity perception, metal-coordination rules,
or periodic crystal-image bonds. Those sources take precedence whenever they
are available.

An inferred bond is derived data. Consumers may use it for stick or cartoon
rendering, but analysis tools must be able to distinguish it from an explicit
source bond. This distinction is represented by the bond-source field in the
structure model.
