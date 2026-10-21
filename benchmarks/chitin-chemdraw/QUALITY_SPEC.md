# Diagram quality benchmark v1

Protocol: `diagram-quality-v1`; disk schema: `2`. This protocol evaluates
single-molecule diagrams, including disconnected components, not reaction
schemes. It produces a quality vector, not a weighted aesthetic score or
chemical certificate.

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
