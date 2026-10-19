#import "/book.typ": book-page
#import "@preview/lovelace:0.3.1": pseudocode-list
#show: book-page

#let source-root = "https://github.com/chitin-dev/chitin/blob/main/crates/chitin-chemdraw/src/layout"

= 1 Layout algorithms
<chemdraw-layout>

The layout implementation is native Rust and does not call an external layout
engine. Topology analysis, ring construction, branch placement, collision repair,
backbone routing, and projected refinement have separate ownership and budgets.

== 1.1 Topology decomposition
<layout-topology>

`chitin-chem::perception::analyze_rings` supplies bridges, articulation atoms,
and vertex-biconnected edge blocks. A block's cycle-space rank is

$ mu = E - V + 1 $

for a connected block, but this is a topological dimension, not a catalogue of
chemical rings. The layout module selects a polygon basis from shortest alternate
paths and a spanning-tree fallback. Independent cycles are assembled through
shared vertices and edges; nonmanifold incidence, incompatible closure, and
missing constraints are rejected rather than repaired by dropping edges.

== 1.2 Ring construction
<ring-construction>

Small cyclic blocks use regular polygon seeds. Fused faces are placed in private
scratch coordinates before branches are expanded. A shared edge has one
coordinate pair and neighboring faces occupy opposite sides. Existing anchors
are retained when a block is entered from an already placed component, and
several rigid orientations compete deterministically.

Independent macrocycles use exact closed unit-edge candidates. Even sizes start
with compact lattice or strip constructions; odd sizes use bounded end-cap
families. Candidate walks are checked for closure, visible bends, crossings,
clearance, and all annotated cyclic double-bond relations. Construction faces
are temporary geometry, not invented chemical rings.

One macrocycle fused to a small regular face has a separate two-face model.
Interior side is determined by directed boundary winding, which remains valid
for concave macrocycle corners. Multiple fused macrocycles, multi-face cages,
and arbitrary nonregular polycycles are outside this candidate family.

== 1.3 Bridges and cages
<bridges-and-cages>

A bicyclic bridge block with two degree-three bridgeheads is represented by three
internally disjoint paths. When the shortest path is unique, the longer paths
form the outer arcs and close at common bridgeheads. Tied shortest paths use a
bounded drawing projection; the interior path may be shortened within an
explicit range because three distinct two-edge unit paths cannot be embedded
with all unit lengths in a plane. This is a drawing compromise, not a physical
conformation.

Complete adamantane and cubane cores use exact template matching. Perspective
views are enabled only under conservative symmetry and stereo conditions. Authored
front/rear crossings retain source bond IDs and are rendered with explicit rear
gaps; arbitrary intersections never acquire automatic over/under order.

== 1.4 Branches and component-local clearance
<component-clearance>

Branches are assigned to free exterior sectors around already placed ring rays.
During placement, close-pair, near-bond, and terminal-leaf searches inspect only
the current connected component. An unrelated fragment cannot change the
threshold, obstacle set, or wide-search mode of a small component. Components
are packed after local placement.

== 1.5 Long-backbone routing
<backbone-routing>

Large bridge-rich components may enter an optional routing stage after ordinary
collision repair. Two breadth-first sweeps approximate a principal spine. Only
source single-bond bridges split movable downstream subtrees, so reflecting a
subtree preserves its internal bond lengths, ring islands, and local angles.

Finite serpentine row guides compete with the compact pose. A candidate is
accepted only if it improves the shared footprint score without adding skeleton
crossings, worsening crowding, or violating supplied double-bond stereo. Neutral
saturated ring carbons may receive a bounded external-port rotation; aromatic,
carbonyl, and heteroatom hosts remain protected.

`LayoutOptions::backbone_budget` is independent of ordinary refinement and is
shared across components and alternatives. Zero disables routing. An exhausted
or incomplete guide leaves the previously validated layout unchanged.

== 1.6 Collision repair and projected refinement
<refinement>

Discrete refinement rotates eligible terminal hydroxyls and other bounded
attachments within their policy limits. It preserves ring geometry, source bond
lengths, and proper crossing constraints. Label-only repair may shift text but
must keep its anchor inside the padded label envelope.

For crowded long-cycle components, projected coordinate refinement applies
bounded descent to the current coordinates. Bond closure, explicit pins, source
double-bond relations, and small-cycle templates are hard constraints. Soft
terms score atom crowding, atom-edge clearance, angle deviation, and crossing
depth. A candidate replaces the current pose only after complete validation and
without introducing a new crossing pair.

== 1.7 Work budgets and failure behavior
<layout-budgets>

Ring construction, ordinary refinement, projected optimization, stereo checks,
and backbone routing use explicit work units rather than elapsed-time limits.
Optional child searches receive bounded allowances and reserve capacity for
required placement. Work exhaustion discards only the unfinished optional trial.
Required construction reports a typed budget error; complete best-so-far
geometry may be retained for optional refinement exhaustion.

The layout is deterministic for a fixed graph, options, and source ordering.
Determinism does not imply canonical SMILES, a globally optimal drawing, or a
physical equilibrium. Remaining crowding and crossings are surfaced through
diagnostics so callers can choose another style, inspect the geometry, or report
the limitation.

== 1.8 Automatic layout pipeline
<automatic-layout-pipeline>

Automatic layout is a staged constrained embedding problem. The input is a
labelled graph $G = (V, E)$ together with bond orders and optional stereo
relations. The output is a coordinate map $p: V -> bb(R)^2$ and diagnostics.
The graph is never changed. Every stage either publishes a complete candidate
or leaves the previous complete candidate untouched.

For each connected component $C$, placement maintains a partial map
$p_C: V_C -> bb(R)^2$. A source bond has nominal length one:

$ ||p_C(u) - p_C(v)|| = 1 quad ((u, v) in E_C) $

except for the explicitly supported symmetric bridge projection, whose shortened
edges are marked and excluded from the unit-distance optimizer. The component is
placed before the next component is translated into the packing strip.

#pseudocode-list[
  + $G <- sans("normalize")("source")$
  + $G <- sans("validate")(G, "options")$
  + $K <- sans("decompose")(G)$
  + $S <- sans("seed")(K)$
  + *for* $C in K$
    + $p <- sans("embed")(C, S)$
    + $p <- sans("refine")(p)$
    + *if* $B_"backbone" > 0$
      + $p <- sans("route")(p, B_"backbone")$
    + *if* $B_"project" > 0$
      + $p <- sans("project")(p, B_"project")$
    + $p_C <- sans("commit")(p, C)$
  + *return* $(p, D)$
]

The order is important. Ring closure happens before branch expansion, complete
molecule scoring happens before a ring pose is selected, and final diagnostics
happen outside the optional search budgets. This prevents a partially searched
candidate from starving required placement or replacing a valid complete pose.

== 1.9 Ring-block candidate construction
<ring-block-pseudocode>

Ring construction first selects a bounded geometric family; it does not try to
solve arbitrary molecular embedding in one optimizer. For each biconnected block,
the algorithm builds a cycle basis from shortest alternate paths and a spanning
tree fallback. The selected independent cycle count is $mu = E - V + 1$.

For a regular face with $m$ vertices and unit edge length, the seed radius is

$ R_m = 1 / (2 sin(pi / m)) $

and the seed vertices are

$ p_k = c + R_m (cos(theta + 2 pi k / m), sin(theta + 2 pi k / m)) $

for $k in {0, dots, m - 1}$. These coordinates are only a seed: shared-edge
placement, closure, side tests, and source stereo are validated before commit.

#pseudocode-list[
  + $P <- {p = sans("seed")(B, xi) : xi in Xi(B)}$
  + $P <- {p in P : C_B(p) = "true"}$
  + $p^* <- arg min_(p in P) Q_B(p)$
  + *if* $P = "empty"$
    + *return* $bot$
  + *return* $p^*$
]

An edge shared by two faces is not duplicated: both faces reference the same
two coordinates. This is why an incompatible closure is an error rather than a
reason to stretch a final edge. The candidate score is evaluated only after
external branches are expanded, because a ring-only score cannot see a crowded
substituent.

== 1.10 Macrocycle and bridge candidates
<macrocycle-pseudocode>

Independent macrocycles with at least twelve atoms use closed unit-edge walks.
Even sizes receive lattice and strip candidates; odd sizes receive bounded
five- or seven-sided end-cap candidates. A candidate with vertices
$p_0, dots, p_(n-1)$ is accepted only when

$ ||p_(i+1) - p_i|| = 1 quad (0 <= i < n),
quad ||p_0 - p_(n-1)|| = 1 $

and every nonincident edge pair has the required clearance. The construction
faces used to generate the walk are not added to the molecular graph.

#pseudocode-list[
  + $P <- {p = sans("seed")(n, xi) : xi in Xi_"lattice" union Xi_"strip" union Xi_"cap"}$
  + $P <- {p in P : ||p_i - p_j|| = 1 "for" (i, j) in E_B}$
  + $P <- {p in P : C_"macro"(p) = "true"}$
  + $p^* <- arg min_(p in P) Q(p)$
  + *if* $P != "empty"$
    + *return* $p^*$
  + *if* $B_"ring" = 0$
    + *return* $"BudgetExceeded"$
  + *return* $"DoubleBondStereo"$
]

For a bicyclic bridge block, three internally disjoint bridgehead paths are
identified. If the shortest path is unique, the two longer paths form the outer
arcs. If shortest paths tie, a bounded projection may shorten only the interior
bridge; the endpoint positions and source edge identities remain fixed. The
projection is a drawing fallback, not a molecular force-field result.

== 1.11 Geometry quality model
<quality-function>

The broad-phase evaluator bins atoms and segments into unit spatial cells before
testing neighboring cells. It reports center crowding, atom-to-edge clearance,
and proper nonincident crossings. A representative score for a complete
component is

$ Q(p) = lambda_a sum_(i < j) max(0, d_a - ||p_i - p_j||)^2 \
  + lambda_e sum_(i, e) max(0, d_e - delta(p_i, e))^2 \
  + lambda_x |X(p)| + lambda_b A(p) $

where $delta(p_i, e)$ is point-to-segment distance, $X(p)$ is the set of proper
edge crossings, and $A(p)$ is a bounded footprint term. The implementation uses
separate diagnostic thresholds and score weights: finding a pair closer than the
threshold is not the same as assigning it an infinite penalty.

#pseudocode-list[
  + $h_i <- (floor(p_i.x), floor(p_i.y))$
  + $H <- {(i, j) : h_i - h_j in {-1, 0, 1}^2}$
  + $Q_a <- sum_((i, j) in H) max(0, d_a - ||p_i - p_j||)^2$
  + $Q_e <- sum_((i, e) in H) max(0, d_e - delta(p_i, e))^2$
  + $X <- {(e, f) : e ∩ f = "empty" and chi_"cross"(e, f) = 1}$
  + $Q(p) <- lambda_a Q_a + lambda_e Q_e + lambda_x |X| + lambda_b A(p)$
  + *return* $(Q(p), X)$
]

The cell index reduces sparse typical drawings from an all-pairs scan, but the
algorithm retains quadratic worst-case behavior when many atoms occupy the same
cells. Every potentially expensive scan charges the shared work budget before it
executes.

== 1.12 Discrete refinement and long-backbone routing
<discrete-refinement>

Discrete refinement changes whole branches or bounded attachment angles, not
individual ring vertices. For a candidate transform $T$, ring distances and
protected source relations are invariant:

$ ||T(p_u) - T(p_v)|| = ||p_u - p_v|| $

for every pair inside a rigid island. A neutral saturated ring port may rotate
within its thirty-degree policy window; aromatic, carbonyl, and heteroatom hosts
are excluded.

#pseudocode-list[
  + $p_0 <- p; q_0 <- Q(p_0)$
  + *for* $t in {1, dots, "refinement_passes"}$
    + $A_t <- {T(p_(t-1)) : T in "admissible_transforms"}$
    + $A_t <- {p in A_t : C_"protected"(p, p_0) = "true"}$
    + $p_t <- arg min_(p in A_t) Q(p)$
    + *if* $A_t = "empty" or Q(p_t) >= Q(p_(t-1))$
      + *break*
  + $p^* <- p_t$
  + $s <- sans("spine")(G)$
  + $B <- {e in s : e in E_"bridge"}$
  + $R <- {p = T_xi(p^*) : xi in Xi_"row" and T_xi(B) = B}$
  + $R <- {p in R : C_"stereo"(p) = "true" and X(p) ⊆ X(p^*)}$
  + *if* $R != "empty"$
    + $p^* <- arg min_(p in R) Q(p)$
  + *return* $p^*$
]

The backbone stage is optional and independent from ordinary refinement. A
search that stops halfway cannot publish its private coordinates. This makes
`backbone_budget = 0` a meaningful baseline for regression comparisons.

== 1.13 Projected coordinate optimization
<projected-optimization>

Projected optimization is continuous drawing refinement, not molecular-energy
minimization. It starts from a complete unit-bond component and keeps source
anchors, double-bond relations, ring closure, and protected small-cycle geometry
as hard constraints. The objective combines the geometry score with soft angle
and crossing-depth terms:

$ F(p) = Q(p) + lambda_theta sum_i (theta_i - theta_i^0)^2
  + lambda_c C(p) $

where $theta_i^0$ is the stage-entry angle and $C(p)$ penalizes undesirable
crossing depth or near-crossing states. A candidate can be accepted only when
the complete shared geometry score does not worsen and no new crossing pair is
introduced.

#pseudocode-list[
  + $z_0 <- p_0; F_0 <- F(z_0)$
  + *for* $t in {0, dots, I - 1}$
    + $g_t <- nabla F(z_t)$
    + *if* $max_i ||g_(t,i)|| < 10^(-9)$
      + *break*
    + $z_(t+1/2) <- z_t - eta_t g_t$
    + $z_(t+1) <- Pi_C(z_(t+1/2))$
    + *if* $z_(t+1) = bot or max_i ||z_(t+1,i) - z_(t,i)|| > 0.2$
      + $eta_t <- eta_t / 2$
      + *continue*
    + *if* $F(z_(t+1)) >= F(z_t) or X(z_(t+1)) ⊄ X(p_0)$
      + $eta_t <- eta_t / 2$
      + *continue*
    + $p^* <- z_(t+1)$
  + *return* $arg min_(p in {p_0, p^*}) Q(p)$
]

The ten-step backtracking line search limits each iteration's local search. The
projection stage may perform up to `projection_passes` sweeps per proposal.
Invalid initial coordinates are rejected before search; budget exhaustion is a
search outcome and does not imply invalid chemistry or non-convergence.

== 1.14 Work accounting and complexity
<layout-complexity>

Let $n = |V_C|$, $m = |E_C|$, $r$ be the number of ring candidates, $k$ the
number of refinement iterations, and $s$ the number of projection sweeps. The
topology and seed preparation are near-linear apart from ordered-map and
candidate-search factors. A sparse quality pass is approximately
$O(n + m + q)$, where $q$ is the number of nearby cell pairs; its dense worst
case is $O(n^2 + m^2)$. Projected optimization is bounded by

$ O(k (10 (s (n + m) + n + q))) $

before the cost of objective and validation scans. This expression describes the
configured search ceiling, not a claim that every iteration reaches the bound.

The implementation charges vector lengths, candidate visits, spatial scans,
projection sweeps, and stored path work. Required ring construction uses its own
budget; optional folding, backbone routing, and projected optimization use
separate allowances. On exhaustion, optional stages return the last complete
validated layout and attach a diagnostic where appropriate.

== 1.15 Source map
<layout-source-map>

The main stages are implemented in
#link(source-root + "/mod.rs")[`layout/mod.rs`],
#link(source-root + "/placement.rs")[`placement.rs`],
#link(source-root + "/rings.rs")[`rings.rs`],
#link(source-root + "/refinement.rs")[`refinement.rs`], and
#link(source-root + "/optimization")[the projected optimizer].
The crate tests contain executable fixtures for fused rings, cages, macrocycles,
stereo, label clipping, component isolation, and long molecules.
