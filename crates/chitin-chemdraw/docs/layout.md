# Layout stages and constraints

The implementation is native Rust and does not invoke an external layout engine.
Topology analysis, polygon assembly, branch placement, and constrained refinement
have distinct ownership. The crate's [README](../README.md) summarizes the public
pipeline and supported scope; this document details constraints and work limits.

## Ownership

`chitin-chem::perception::analyze_rings` returns a detached low-link decomposition:
edge-disjoint vertex-biconnected blocks, bridges, and articulation atoms. Isolated
atoms have no edge block; a bridge has a one-edge block. Spiro rings can share an
atom while belonging to different blocks; fused/bridged cycles stay in one block.
`E - V + 1` is cycle-space rank, **not** a catalogue of chemical rings. No graph
mutation, aromaticity perception, or cached derived facts are installed.

`layout::rings` selects geometry per block. Edge-fused blocks use rigid polygon
constraints:

- Each edge supplies all tied shortest alternate paths as small-cycle candidates.
  A spanning tree adds a complete fundamental fallback cycle set. A unicyclic
  block requires only one alternate-path search.
- Candidates are ordered by length and stable vertex order; binary edge-vector
  elimination selects E - V + 1 independent cycles. This selected polygon basis
  is not an all-rings catalogue, aromaticity assignment, or a general minimum-
  cycle-basis guarantee.
- Each selected edge has one or two incident polygons. Two shared vertices must
  form one shared edge; shared paths and nonmanifold incidence are rejected. The
  shared-edge face graph must cover the entire cyclic block.
- Independent rings connected by bonds/chains and simple spiro junctions are
  assembled as part of the same component, not rejected by component cycle rank.
- Compatible higher-rank fused systems are supported without a fixed ring count.
  Incompatible incidence or regular-polygon geometry returns
  `LayoutError::RingSystem`. No closing edge is dropped or stretched.

Large polygons seed first. A cyclic block is assembled completely in private
scratch coordinates before external branches expand. The next face is selected
by known shared edges/vertices, not whichever atom happens to be visited first.
Every fused neighbor must lie on the opposite side of its shared edge. Extra
known vertices are checked as closure constraints, including three-face junctions.

Shared-vertex or linked-system entry tries six rigid orientations and retains
the least crowded complete candidate. Existing anchors remain unchanged. Branch
headings use exterior angular gaps outside all incident faces, not the radius
of whichever face was placed last. Existing entry directions are refreshed too;
an occupied exterior parent slot is not reused for a remaining substituent.
The parent must actually lie outside the atom's cyclic block. A fused junction's
three already embedded ring neighbors do not occupy that exterior slot: its
remaining methyl uses the exterior bisector unchanged. Offsetting such a methyl
by 60 degrees would fold it back onto a ring edge. An entered block with an
actual external bridge parent retains the adjacent-slot allocation instead.

These are rigid regular-polygon constraints. Some topologically fused graphs
need nonregular geometry, alternative face decompositions, or more general
constrained embedding. Such inputs can still be unsupported; no geometric
success is claimed solely because a cycle basis was found.

## Bicyclic bridge-path model

A rank-two block with two degree-three atoms and degree-two internal atoms has
three internally disjoint bridgehead paths. Classification uses only block edges,
so an external substituent does not change an atom's ring-system degree. A direct
bridgehead edge keeps the existing edge-fused polygon model.

For three paths with a uniquely shortest path, the two longer paths form outer
arcs and the shorter path forms an interior bridge. Each arc consists of unit
chords. Bisection solves its turning angle for a common bridgehead separation;
all paths install the same exact endpoints and validate every edge before any
coordinates are published. This is not a relaxed regular-polygon closure and
does not drop bonds or shorten them to hide an overlap.

Three separations, two interior sides, and six rigid orientations are scored
deterministically against the current component. A single known entry anchor is
retained exactly. External attachment headings use actual bond rays rather than
regular-face interior angles. Path detection, root solving, temporary snapshots,
scoring, and heading scans consume the shared ring-search budget; the current
heading scans are quadratic in block size, but budgeted.

Equal shortest paths would occupy identical arcs in this model and are explicitly
unsupported, as are higher-rank cages and multi-anchor entry. They need a flexible
chain/constrained embedding model. No physical conformation, over/under crossing,
or tetrahedral configuration is inferred. Source stereochemistry is left to the
existing depiction stage. Interior-bridge substituents can still cause crossings;
remaining crowding/crossings are reported normally, not hidden by this solver.

`layout::refinement` then searches whole-branch transformations across **single
bridges only**. Cyclic edges and multiple bonds are never cut for refinement.
The smaller branch is reflected across the attachment axis or rotated ±120
degrees about its own attachment atom. Reflections preserve angle magnitudes;
rotations preserve internal branch/ring geometry and all bond lengths. Every
candidate must additionally preserve the angles between the incoming cut bond
and each outgoing neighbor. Improving collision cost cannot override this
constraint. Rotations at linear triple/cumulated-double centers are excluded.
Trial coordinates always
start from the original snapshot and must strictly improve component cost.
This is a depiction heuristic, not a physical energy minimization.

Terminal neutral hydroxyls on saturated neutral carbon use a separate final
clearance stage. Rotating a one-atom branch about its own oxygen cannot move
that oxygen; these no-op branch trials are skipped. The local stage instead
rotates O about its fixed carbon by ±15°/±30° from the post-branch snapshot.
The entire ring skeleton stays fixed. Eligible carbon hosts have only single
bonds; carbonyl/unsaturated/aromatic hosts, ring oxygens, ethers and unknown
charge/radical states are excluded. This does not change any H count or source
tetrahedral descriptor.

Each final angle to another carbon ligand stays within 30° of the local stage's
starting value, with at least 60° separation; all trials refer to that same
snapshot, so repeated passes cannot accumulate angular drift. This bounded
saturated projection exception does not relax the rigid-branch trigonal guard.
Only a completely measured, strictly lower geometry cost with no new proper
crossings can be committed. The geometry score does not measure text envelopes;
the large-polycycle regression additionally verifies the actual union of O/H
glyph bounds, rather than asserting that separated centers always imply legible
labels. General font-aware layout remains future work.

When eligible hydroxyls exist, one quarter of the remaining refinement budget
is reserved for this final stage. A large whole-branch search cannot consume
that reserve. Unspent work is returned, total allowance is unchanged, and either
stage's exhaustion retains a complete scored result with the existing diagnostic.

These are planar drawing transforms, not changes to supplied tetrahedral facts
or 3D stereoisomer generation. Depiction computes wedge polarity from the final
coordinates and unchanged ordered ligand descriptors, including reflections.
Directional double-bond and other extended stereo remain unsupported.

## Quality and resource contract

Cost combines close centers, atom-to-nonincident-edge clearance, and proper
nonincident bond crossings. Unit-cell indexing prunes ordinary sparse drawings;
dense overlaps can still have quadratic work. It is not an exact glyph-envelope
collision model: labels are constructed later with `DepictOptions` and real font
metrics. Passing layout quality checks is not a promise of collision-free labels.

`LayoutOptions::refinement_passes` bounds passes per search stage (default 4;
0 disables both whole-branch and terminal search).
`refinement_budget` bounds candidate traversal/indexing/geometric work across all
fragments (default 1,000,000). Initial/final quality inspection and seed generation
are outside that search budget; `max_atoms` bounds the input independently.
Exhaustion retains complete, best-so-far scored coordinates and adds
`RefinementBudgetExhausted`; it never returns a truncated molecule. Reaching the
pass limit does not imply a global optimum either.

`ring_search_budget` separately bounds discovery, path enumeration, cycle-vector
elimination, incidence checks, scratch assembly, and initial scoring (default
2,000,000 work units). It is shared across blocks and fragments. Unlike limited
refinement, exhausted ring construction has no complete result to return and
fails with `RingSearchBudgetExceeded`. Zero is invalid. Work units account for
algorithmic visits and stored path/vector lengths, not elapsed CPU time; ordered
map and sorting operations add logarithmic overhead.

Enumerating tied shortest paths can be exponential on unusual graphs. The budget
bounds that enumeration rather than silently discarding alternatives. Selecting
the next face currently scans the remaining faces, so assembly can be quadratic
in face count; it is iterative, budgeted, and is not described as constant-time
or globally linear. Ordinary short-ring systems benefit from shallow, local BFS
distance maps instead of repeatedly clearing block-sized arrays.

Final crowd and crossing diagnostics are recomputed **after** refinement, so
resolved placement-time contacts are not reported as if still present. Fragments
are scored separately, oriented horizontally in 30-degree increments, then
repacked with the requested center-bound gap. This
still does not account for wide labels between fragments.

## Validation and remaining work

Tests cover block partitioning, root articulation cases, sparse IDs, unknown bond
orders in topology analysis, iterative long chains, linked/spiro/fused unit bonds,
determinism, graph immutability, disabled/budget-limited refinement, proper
crossing detection, and a folded-chain fixture whose score actually decreases.
The fixed visual corpus covers linear/angular fused systems, mixed five-/six-
membered systems, acid/ester/amide/nitrile geometry, and substituted fused systems.
Tests also cover multi-anchor junctions, incompatible closures, reversed edge
insertion, sparse handles, shared search budgets, and a 128-ring strip. The
`layout_regressions` example exports the same inputs for SVG raster inspection;
geometry assertions alone cannot establish visual quality.

Bridge regressions cover asymmetric bicyclic paths, heteroatoms, explicit chiral
H, linked/spiro entry, exact unit closures, sparse handles, reordered edges, shared
work budgets, and the supplied large polycycle. Its complete export is tested;
that does not imply a collision-free global drawing.

Next stages: symmetric/cage constrained embedding/templates, nonregular fused
embedding, exact label-aware refinement, and richer quality reporting.
Aromaticity/stereochemistry require corresponding chemical representation and
parser support first; lowercase SMILES is not silently converted to uppercase.
