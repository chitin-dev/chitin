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

`layout::rings` turns supported blocks into rigid polygon constraints:

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

These are rigid regular-polygon constraints. Some topologically fused graphs
need nonregular geometry, alternative face decompositions, or more general
constrained embedding. Such inputs can still be unsupported; no geometric
success is claimed solely because a cycle basis was found.

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

The current input model has no stereo constraints. When those become supported,
reflection/rotation eligibility must respect them before enabling stereo layout.

## Quality and resource contract

Cost combines close centers, atom-to-nonincident-edge clearance, and proper
nonincident bond crossings. Unit-cell indexing prunes ordinary sparse drawings;
dense overlaps can still have quadratic work. It is not an exact glyph-envelope
collision model: labels are constructed later with `DepictOptions` and real font
metrics. Passing layout quality checks is not a promise of collision-free labels.

`LayoutOptions::refinement_passes` bounds search passes (default 4; 0 disables).
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

Next stages: bridged-system constrained embedding/templates, nonregular fused
embedding, exact label-aware refinement, and richer quality reporting.
Aromaticity/stereochemistry require corresponding chemical representation and
parser support first; lowercase SMILES is not silently converted to uppercase.
