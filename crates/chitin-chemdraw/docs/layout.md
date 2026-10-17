# Layout stages and constraints

The implementation is native Rust and does not invoke an external layout engine.
Topology analysis, polygon assembly, branch placement, and constrained refinement
have distinct ownership. Development references are listed in the crate's
[README](../README.md#inspired-by), with required source attribution and licenses
in the [attribution README](../third_party/README.md).

## Ownership

`chitin-chem::perception::analyze_rings` returns a detached low-link decomposition:
edge-disjoint vertex-biconnected blocks, bridges, and articulation atoms. Isolated
atoms have no edge block; a bridge has a one-edge block. Spiro rings can share an
atom while belonging to different blocks; fused/bridged cycles stay in one block.
`E - V + 1` is cycle-space rank, **not** a catalogue of chemical rings. No graph
mutation, aromaticity perception, or cached derived facts are installed.

`layout::rings` turns supported blocks into rigid polygon constraints:

- A rank-one block is ordered by connectivity and embedded as a regular polygon.
- A rank-two block with exactly two degree-three junctions directly joined by an
  edge is represented by the two other paths closed through that fusion edge.
- Independent rings connected by bonds/chains and simple spiro junctions are
  assembled as part of the same component, not rejected by component cycle rank.
- Higher-rank fused blocks and bridged blocks without the direct fusion edge
  remain `LayoutError::RingSystem`. No ring-closure edge is dropped or stretched.

Large polygons seed first. Shared-edge placement tries both sides; shared-vertex
or linked-ring placement tries six rigid orientations. Existing anchor points
are immutable. Branches expand from their incident ring's exterior directions.

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
SVG raster inspection covers linked rings, naphthalene, a spiro pair, and a
branched multi-ring molecule; geometry assertions alone cannot establish visual
quality.

Next stages: higher-rank fused-ring assembly, bridged-system constrained
embedding/templates, exact label-aware refinement, and richer quality reporting.
Aromaticity/stereochemistry require corresponding chemical representation and
parser support first; lowercase SMILES is not silently converted to uppercase.
