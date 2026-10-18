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

## Macrocycle candidates

An independent unicyclic block of at least 12 atoms uses a separate
outline model. It is not installed as a nonregular face inside the rigid fused
polygon solver. Small rings retain their existing regular polygon model;
linked or spiro-connected blocks retain separate ownership.

Even-size candidate boundaries start with compact hexagonal-cell growth and a guaranteed
strip construction, capped by a three-cell triangle for sizes divisible by
four. Shared integer edges cancel before tracing one simple closed perimeter.
Every retained edge is unit length: there is no fitted closing edge, omitted
vertex, odd-corner shortcut, or spring-length tolerance. Compact growth may stop
short of its requested perimeter; the strip construction still supplies a
complete exact-size candidate. Holes, pinches and extra loops are rejected.

Nonlattice candidates use a regular five- or seven-sided end cap for odd sizes,
and a six- or eight-sided cap to diversify even rings. Attaching a hexagon along
one boundary edge replaces that edge with five unit edges, adding four vertices.
The old endpoints are retained exactly; no graph edge or atom is invented.
Straight and alternating extension sequences supply different turn patterns.
Every complete walk is checked for finite unit edges (including closure), visible
nonacute bends, crossings, collinear overlaps, tangencies and a 0.25-unit
nonincident-edge clearance. The end-cap families have occupied bond angles
between 105 and 135 degrees. They do not use a near-linear odd-corner shortcut.

Each boundary is tested in both traversal directions at every cyclic offset.
Annotated ring double bonds are hard filters, with exocyclic references converted
to the equivalent adjacent-ring-neighbor relation. If no enumerated candidate
satisfies all relations, layout returns `DoubleBondStereo`, not a lower-quality
incorrect configuration. This is a limited candidate family, not a complete
solver for all physically possible cyclic configurations.

Scoring considers component-local center clearance and penalizes external
attachments assigned to concave corners. Attachment headings use the free
sector opposite the sum of the two ring rays, not a convex-only radial approximation.
It is exactly the free 120-degree sector at ordinary lattice corners; nonregular
cap corners retain their actual ring angles and bisect the remaining sector.
At a concave center this slot can point into the global ring interior; using
the topological exterior there would make 60-degree substituent angles and
can invert an exocyclic stereo reference. Remaining crowding is still diagnosed.

A single existing articulation point is retained bit-for-bit and six rigid
rotations are compared when entering an anchored block. Construction, cyclic
assignment, constraint checks, snapshots and scoring consume the existing shared
ring-search allowance. Growth and candidate evaluation can be quadratic (with
ordered-map overhead); exhaustion returns an error without partial geometry.

## A macrocycle with one fused small face

Exactly two selected faces, one of size at least 12 and one of size 3–8,
dispatch to a separate atomic block model. The large face streams full candidate
poses; the small regular polygon is assembled on the common unit edge. The
local interior side follows polygon winding and directed boundary order, not
the centroid of a possibly concave macrocycle. Interior containment and all
nonincident block edge pairs reject overlapping faces, crossings and tangencies.

Source stereo referencing a vertex of the attached face is deferred until both
faces exist. Such a reference is not a freely positioned substituent, so simply
inverting a ring-neighbor side can be wrong at a concave junction. Constraints
whose four reference/axis vertices are in the block are checked together before
scoring. External references still pass through the required final stereo stage.
If no candidate meets a known configuration, an explicit `DoubleBondStereo`
failure remains; this does not classify the input as chemically invalid.

Whole-block rigid placements preserve one fixed entry vertex in either face,
including a shared junction, bit-for-bit. Two fixed vertices are unsupported by
this model. Multiple small faces, two fused macrocycles, shared paths and cages
still require more general embedding; they are not advertised as solved by this
two-face construction. Cross-face validation can be quadratic per candidate;
all scans and trials share the ring work allowance. There is no continuous
relaxation or physical conformation search in this stage.

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
Required acyclic double-bond relations are enforced before optional refinement;
refinement candidates cannot invert them. Ring double-bond relations must be
satisfied by the selected seed, including the macrocycle candidates above.
Other extended stereochemistry remains unsupported.

## Projected coordinate search

`layout::optimization` owns a separate complete-coordinate solver. After discrete
refinement, automatic placement invokes it for crowded components containing a
nonbridge edge whose shortest alternate path has more than seven edges. A
bounded BFS per cyclic edge detects short-cycle witnesses; their vertices stay
fixed as existing small-ring templates. The standalone `optimize` entry point
can also refine acyclic complete coordinates with explicit graph-local anchors.
Neither path constructs missing coordinates or broadens ring seed support.

Source bonds are unit-distance equations. Multiple-bond and supplied tetrahedral
centers additionally constrain neighbor chord lengths and preserve nondegenerate
ray orientation. At other centers, the original angle supplies a reference and
a ±30-degree motion envelope. The lower angle bound is the smaller of the
original angle and 60 degrees; the upper bound is the larger of the original
angle and 150 degrees, intersected with that envelope. Existing acute/straight
input is not made worse merely to escape a collision. Bounds are implemented
as neighbor-chord intervals and projected with unit equations; rejecting active
inequalities only after a gradient step would spuriously stall feasible descent.

Each trial applies a bounded objective-gradient step and deterministic sequential
distance projection, then scans all residuals. Unit-edge error at the acceptance
boundary is at most `1e-9`; projection targets `1e-10`. Explicit anchors and
short-ring template pins retain exact original points. Failed or nonconverged
projection is a discarded scratch trial, never an approximately closed output.
Angle magnitudes, protected orientations, coordinate range and every supplied
double relation are checked again before scoring. No graph facts are changed.

The old and new stages share `layout::quality` for geometry score and diagnostic
contacts. Analytical gradients include atom repulsion and atom/edge clearance;
bounded soft-angle terms prefer the initial projection. Proper crossings also
have a depth term, because their discrete count alone has no descent direction.
Backtracking requires a strictly lower combined objective and no crossing pair
outside the initial set. Private intermediate states may temporarily have a
worse geometry-only score; the separately retained publishable best must satisfy
all constraints and a shared geometry score no greater than at entry. Exhaustion
or stagnation returns that best, including the unchanged original if appropriate.
This is not a guarantee of finding a feasible uncrossing path or a global optimum.

The default allowance is 64 iterations per component, up to 256 projection
sweeps per backtracking trial, and 1,000,000 shared work units. Short-cycle search,
angle/chord preparation, proposal generation, projection scans, validation,
gradients, and candidate scoring consume this allowance. Mandatory topology,
input validation, and initial/final diagnostic inspection are outside search
work and bounded by the input atom limit. Initial integer/aromatic representation
validation uses the existing domain contract. The initial gradient implementation
scans atom pairs and atom/edge pairs quadratically; it is budgeted, not described
as globally linear. Metadata allocation and ordered-map operations add overhead.

Automatic `optimization.iterations = 0` or `optimization.work_budget = 0` disables
this stage; `refinement_passes = 0` still disables all optional stages. Its work
allowance is independent of the discrete budget and cannot consume the reserved
hydroxyl allowance. `OptimizationBudgetExhausted` differs from discrete
`RefinementBudgetExhausted`; neither implies a truncated result. Reaching the
iteration limit or a stationary projected step is not certified convergence.

The independent API additionally checks exact live-ID coverage, unknown anchors,
finite guarded coordinate ranges, unit source edges, and initial double-bond
configuration. Disconnected components are scored independently without pose
reorientation or repacking; the caller retains responsibility for interfragment
spacing. Explicit rejection of an invalid initial state is distinct from a
valid but unhelpful or budget-limited optimization result.

## Quality and resource contract

Cost combines close centers, atom-to-nonincident-edge clearance, and proper
nonincident bond crossings. Unit-cell indexing prunes ordinary sparse drawings;
dense overlaps can still have quadratic work. It is not an exact glyph-envelope
collision model: labels are constructed later with `DepictOptions` and real font
metrics. Passing layout quality checks is not a promise of collision-free labels.

`LayoutOptions::refinement_passes` bounds passes per search stage (default 4;
0 disables whole-branch, terminal and projected search).
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
