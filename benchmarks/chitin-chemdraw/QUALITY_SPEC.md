# Diagram quality benchmark v3

Protocol: `diagram-quality-v3`; disk schema: `2`. This protocol evaluates
single-molecule diagrams, including disconnected components, not reaction
schemes. It produces a quality vector, not a weighted aesthetic score or
chemical certificate.

Version 3 adds topology-defined large-cycle angle/turn measurements and
`macrocycle_geometry` provenance. Version 2 added planar whitespace measurements
and `ring_geometry`. Existing measurement formulas and basic
readability criteria are unchanged. The web viewer accepts v1/v2/v3 separately;
missing historical measurements are unavailable, never inferred or zero-filled.

## Reproducibility

Sample original records before chemistry parsing. Preserve duplicates, failures,
timeouts and oversize records. Pin source SHA-256, dataset release, selected
rows, seed, renderer parameters, actual font and executable fingerprint in each
run. The `renderer` executable snapshot reproduces deferred drawings after the
source algorithm changes. Without a matching renderer or a saved SVG, a
historical drawing is unavailable; current-algorithm previews must be explicitly
labeled.

The pipeline is parse, characterize, layout, depict, SVG export, independent QA.
QA never changes the coordinates, and its time is excluded from generation
stages. Single-case cold-process timings are descriptive, not controlled
performance CIs.

New case records also retain `constraint_closure`: whether coupled distance/alkene
closure replaced an unsupported preferred embedding or repaired its cyclic
configuration. Cohort reports count these separately. This provenance is not a
quality exemption: all resulting coordinates, labels, contacts and bond sides
undergo the same independent checks. A generated SVG is not a readability pass.
Historical records lacking this field do not establish absence of fallback.

## Applicability and numerical policy

Every observation records `state`, `value`, `eligible`, `violations`, and
`reason`. States are `measured`, `not_applicable`, and `not_implemented`. An
unavailable value is null, never a passing zero. A rate is violations /
eligible; an empty domain has no rate. A descriptive count can have one scene as
its eligible object.

L is the median strictly positive source-bond length, averaging the middle pair
for an even count. Geometric distances are divided by L; crossing tolerance is
1e-9 L. Zero-length bonds are measured separately. Distances without a usable L
are unavailable. Finite-coordinate and complete graph-handle coverage are
required. The checks are scale/translation/rotation invariant within normal
floating-point range. Relative length error cannot detect uniform enlargement of
every bond.

All numeric thresholds below are local, versioned screening policy, not
numerical limits prescribed by IUPAC. They require calibration against a
human-rated set. Relevant graphical principles are described by the
[IUPAC 2008 recommendations](https://iupac.qmul.ac.uk/drawing/drawing.html) and
[IUPAC 2006 stereochemical recommendations](https://iupac.qmul.ac.uk/drawing/stereo.html).
Neither zero warnings nor a successful exporter establishes full compliance.

## Measurements

### Large-cycle local geometry (v3)

These measurements use source topology, not the validity of the drawn planar
faces. They remain available for crossing layouts, without interpreting local
turns as polygon interior angles. Explicit perspective layouts are inapplicable.

Within each cyclic block with at least nine vertices, collect one deterministic
shortest alternate path per edge and a spanning-tree fundamental cycle set.
Sort cycles by length and source-ordinal traversal, then select an independent
basis over GF(2). This is a short-first basis, not an exhaustive minimum basis,
SSSR, a unique chemical ring set, or a claim that block size is ring size.
Only selected cycles with at least nine vertices supply observations. Discovery
has a shared 1,000,000-operation allowance and stored pairs are bounded at 8192
per molecule. An excluded block makes the aggregate channels unavailable;
partial local records retain their explicit exclusion count.

Deduplicate each angle by center and unordered ligand pair. Deduplicate each
adjacent-angle pair by its four source ordinals modulo path reversal; preserve
the smallest selected large cycle that contains it. Source-order canonical
traversal defines signs. Reversing a path maps (x,y) to (-y,-x); reflecting the
drawing maps it to (-x,-y). Neither transformation implies stereo inversion.

For A-B-C-D, turns are atan2(cross(AB,BC),dot(AB,BC)) and
atan2(cross(BC,CD),dot(BC,CD)), using normalized rays, in [-180,180] degrees.
Collapsed bonds produce missing turns, not numerical zeroes. The unsigned
local bond angle is 180-abs(turn), even when the complete walk crosses itself.

Centers are stratified as linear (incident triple or degree-two cumulene),
junction (more than two block neighbors or more than four total neighbors),
small_ring (the exact local ligand pair occurs in a selected cycle of size <=8),
or ordinary path. Only ordinary paths receive the following drawing-angle and
near-straight flags. This avoids penalizing required linearity and forcing a
universal 120-degree rule onto bridgeheads. The ordinary drawing reference set
is {108,120,900/7,135} degrees; it is not a prediction of physical hybridization.

| Key | Definition |
| --- | --- |
| macrocycle_angle_coverage | Valid unique local angles / all unique local angles on selected large cycles; descriptive, not an error rate |
| macrocycle_angle_deviation_rmse_deg | RMSE of distance to the nearest ordinary drawing reference angle; violations exceed 15 degrees |
| macrocycle_angle_deviation_p95_deg | Nearest-rank 95th percentile of those absolute deviations |
| macrocycle_near_linear_fraction | Ordinary valid local angles >=165 degrees / ordinary valid local angles |
| macrocycle_longest_near_linear_run | Maximum cyclic contiguous count of ordinary near-straight centers over selected large cycles |
| macrocycle_longest_same_turn_run | Maximum cyclic contiguous count of ordinary nonzero turns with the same sign; descriptive only |

An empty domain is unavailable, not zero. Collapsed bonds reduce coverage and
make aggregate distortion/run measurements unavailable. Junctions, small-ring
centers, invalid angles and required linear centers break ordinary runs. Runs
wrap across cycle ends but cannot exceed the cycle length. No fixed favored
regions are asserted for the adjacent-turn density map: cycle size, closure,
substitution, fusion and stereo constraints matter. Long same-sign runs are a
diagnostic, not a universal violation. These channels do not change the existing
basic readability screen or certify acceptable macrocycle layout.

`macrocycle_geometry` stores exact source-ordinal paths, local classes, signed
turns, selected cycle sizes/ranks, coverage and exclusion counts. The benchmark
worker attaches source centers in scene units and the exact SVG viewBox so the
viewer can overlay the historical path. Overlays are disabled on current
algorithm previews; historical coordinates cannot annotate a different layout.

### Existing scalar measurements

| Key                           | Eligible objects and value                                                                                                                                                                        |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| nonbonded_atom_proximity      | Unordered atom pairs without a direct bond, including different components; fraction with distance/L < 0.55                                                                                       |
| atom_edge_proximity           | Nonincident atom-bond pairs; fraction with closed-segment distance/L < 0.15                                                                                                                       |
| unannotated_edge_contact      | Unordered bonds with no common endpoint and no explicit perspective crossing annotation; intersecting, collinear overlapping or tolerance-touching fraction                                       |
| zero_length_bonds             | All source bonds; fraction with exactly zero center distance                                                                                                                                      |
| relative_bond_length_rmse     | Planar bonds; sqrt(mean((length/L - 1)^2)); violations have absolute relative deviation > 0.20                                                                                                    |
| acyclic_carbon_angle_rmse_deg | Planar degree-two carbon centers with both incident bonds outside cycles; target 120 degrees for two singles, 180 for a triple or two doubles; other cases excluded; violations exceed 15 degrees |
| simple_ring_angle_rmse_deg    | Planar atom occurrences in rank-one blocks with 3-8 vertices; target (n-2)180/n degrees; violations exceed 15 degrees; a regularity proxy, not a compulsory ring style                            |
| double_bond_relation          | Supplied explicit relative-side descriptors; fraction incorrect or degenerate; normalized cross-product magnitude must exceed 1e-8; not CIP assignment                                            |
| source_bond_coverage          | All source bonds; fraction without a rendered line/polygon carrying that source; no independent bond-order decoding is claimed                                                                    |
| required_atom_label_coverage  | Non-C/non-H atoms, charged/isotope/radical atoms and isolated atoms; fraction lacking ordinary atom-associated text; presence is not content correctness                                          |
| label_label_overlap           | Distinct logical label pairs; positive-area intersection of actual-font run envelopes, deduplicated by source; expanded H is separate                                                             |
| label_bond_overlap            | Nonincident logical label/source-stroke pairs; envelope contact with a stroked line or filled polygon, deduplicated across color halves and hash strokes                                          |
| component_radius_gyration     | Sum of squared distances to each component's centroid / (atom_count L^2); descriptive only, compared on identical inputs, not globally minimized                                                  |
| measured_labels               | One evaluated scene; count of logical labels with measured text                                                                                                                                   |

Line/label intersection conservatively expands the label box by half the stroke
width. Polygon intersections include filled interiors. These are envelope tests,
not pixel masks. Ordinary incident labels are excluded; auxiliary H retains its
own identity. Arbitrary salt packing does not change component centroid spread.

Perspective is indicated by layout depths or authored crossings. It excludes
planar length/angle penalties, not all contact checks. Only explicitly annotated
bond pairs are removed from crossing eligibility. Macrocycles and multicyclic
blocks are not forced into regular polygons. Block rank E-V+1 is not chemical
ring count, and cyclic-block vertex count is not chemical ring size. This runner
uses the ordinary auto-layout, not automatically inferred metal-coordination
annotations.

## Planar whitespace and footprint

These measurements have no violation threshold and do not contribute to
`readability_clear`. Larger cavities, rounder contours or larger hulls are not
automatically incorrect. Compare identical inputs and inspect ring size and
contact screens before assessing alternative layouts.

Ring contours are the simple bounded faces of each vertex-biconnected cyclic
block in its actual angular half-edge embedding. They are not a minimum cycle
basis, all simple cycles, or a block's outer perimeter. For example, two fused
rings yield two bounded faces, not an extra composite outer ring. The positive
orientation is retained; its reverse exterior walk is discarded. Nonincident
contacts at tolerance 1e-9 L, coincident adjacent rays, zero-length edges, or a
face count different from the block rank make that block unassessed. If any
cyclic block is unassessed, global ring maxima are `not_implemented`, not maxima
over a silently reduced domain. Valid per-face details remain available.

- `maximum_ring_cavity_radius`: maximum over bounded faces of the largest
  in-face disk avoiding every source skeleton segment in that face's connected
  component, divided by L. It measures skeleton whitespace, not ink clearance:
  label outlines and stroke width are not obstacles. Other disconnected
  components do not obstruct the face. Deterministic best-first square
  subdivision uses the signed-clearance 1-Lipschitz upper bound; each face
  receives at most 8192 cell evaluations and absolute error tolerance 0.02 L.
  `value` is the maximum lower bound only when every face converged. Budget
  exhaustion makes the scalar `not_implemented`; each face retains its lower
  bound, upper bound and convergence flag. With no bounded faces, it is
  `not_applicable`. `eligible` counts valid bounded faces.
- `ring_isoperimetric_ratio`: maximum `4πA/P²` over bounded faces, with exact
  polygon shoelace area A and source-segment perimeter P. Range [0,1] up to
  floating-point clipping. `eligible` counts bounded faces. This can be
  dominated by a small regular ring; it is not a macrocycle-specific quality
  score.
- `normalized_footprint`: sum of each connected component's source-atom convex
  hull area divided by `N L²`, where N includes all source atoms. This refines a
  single global hull to remove arbitrary salt-packing distance. Isolated atoms
  and collinear components contribute zero area. `eligible` is N. Labels, stroke
  widths and SVG padding are not included.

All three exclude authored perspective layouts and layouts without a positive
bond scale. `ring_geometry.faces` records the ordered zero-based source atom
ordinals for each bounded face, A/L², P/L, Q, cavity bounds and convergence.
These ordinals refer to source graph atom iteration order, not opaque handles.
Statistics are molecule-weighted scalar distributions; individual face values
are audit metadata, not extra molecules in the scatter plot.

## Missing evaluators and restricted readability

Tetrahedral final wedge/hash parity is unimplemented when descriptors exist and
inapplicable otherwise. Chemical label contents (element, isotope, charge, H,
radical) are not independently decoded. Curved ink contacts are unimplemented
when circle primitives exist. Bond-order decoding and scaffold-specific
graphical orientation are not certified by this protocol.

`readability_clear` combines the five proximity/contact checks with required
label coverage, source-bond coverage and nonzero-bond checks. It is false if any
has a measured violation, true only when every check has zero violations or a
genuinely empty domain, and null otherwise. It excludes style and full chemical
meaning; therefore it must not be called overall diagram correctness.

## Aggregation and validation

Report per-metric measured/inapplicable/unimplemented counts, eligible objects,
unavailable objects, affected cases, total violations, molecule-weighted mean
and nearest-rank P50/P95/P99/max. Micro rates pool measured eligible objects. No
empty denominator is replaced by 0% or 100%. Largest-value source references
identify review candidates; descriptive maxima are not automatically worst
drawings.

Coverage = evaluated / selected. End-to-end readability yield = clear /
selected. Conditional readability yield = clear / evaluated; unknown screens
remain in the latter denominator and are reported separately. Failures stay in
the input denominator, not in measured scalar distributions. Contact categories
overlap.

Cohorts use source atom count 0-24/25-49/50-99/100+, maximum block rank
0/1/2-3/4+, presence of tetrahedral/double-bond descriptors, and final
planar/perspective profile. Uncharacterized inputs and unavailable final
profiles have separate cohorts. Do not sum overlapping cohorts or mistake
final-profile conditioning for full-input coverage. Compare only matching
protocol, selected records, parameters and dataset checksum, accounting for
paired generation failures first.

Evaluator tests use known authored collisions, double-bond reversals, missing
source strokes, filled wedges, geometric transforms, component translations,
small rings, macrocycle exclusion, perspective and missing-value boundaries.
Saved SVGs are visual audit samples, not human-rated ground truth. A separately
annotated calibration set remains necessary before claims about human
preference.
