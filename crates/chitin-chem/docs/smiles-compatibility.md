# Molecular SMILES compatibility — revision 2

This matrix describes executable capability, not complete format conformance
or parity with another toolkit. The reader preserves facts; chemistry models
and drawing each have their own acceptance boundary. Tests are locally authored.

| Capability | Read/write | Automatic SVG pipeline | Next gap |
| --- | --- | --- | --- |
| Organic subset, branches, fragments | Supported | Supported within valence model | Broader valence/H models |
| All 118 bracket elements | Supported | Only known H/facts or supported inference | Element-specific label/valence rules |
| Isotope, charge, declared H, explicit H vertices | Supported | Supported; no radical knowledge invented | Broader charged/open-shell models |
| Ring closure, reuse, `%00`–`%99` | Supported | Monocyclic, spiro, compatible fused systems | Bridged/nonregular embedding |
| Aromatic `b c n o p s`, bracket `as se` | Supported | Restricted C/N/O/S model below | B/P/As/Se and further charge states |
| Omitted aromatic bonds and explicit `:` | Supported | Shared integer-bond assignment | General aromaticity perception |
| Explicit `-` between aromatic atoms | Preserved at edges and ring sites | Preserved | None in this stage |
| Aromatic heterocycle `[nH]` | H declaration preserved | NH label; no extra inferred H | Broader heteroatom states |
| Tetrahedral `@`, `@@`, `@TH1`, `@TH2` | Supported with ordered graph references | Solid/hashed wedges | CIP labels and stereogenicity perception |
| `/`, `\` and other stereo classes | Explicit unsupported error | Not implemented | Double-bond and extended coordination models |
| Atom class/map `:n` inside brackets | Explicit unsupported error | Not implemented | Typed annotations independent of graph IDs |
| Wildcard/query atoms, SMARTS | Explicit unsupported error | Not implemented | Separate atom identity/query model |
| Reaction/CX extensions, record names | Explicit unsupported/syntax error | Not implemented | Separate records and selected dialects |
| Canonical SMILES | Deterministic noncanonical writer only | Not applicable | Canonical labeling, not sorted IDs |

## Current aromatic model

`perception::kekulize(graph, options)` resolves supplied labels, not aromaticity
of uppercase input. It supports neutral aromatic C/N/O/S and bracket N(+1/-1)
within two-/three-neighbor, at-most-three-sigma-coordinate states. A supplied
double bond consumes its endpoint's pi slot. Donor N/O/S do not require an
internal double bond; pyridine-like N and ordinary aromatic C do. Required pi
slots are matched globally, not by alternating each independently perceived ring.

Examples covered end to end include benzene, pyridine, pyrrole, N-substituted
pyrrole, furan, thiophene, naphthalene, indole, anthracene, biphenyl, aromatic
esters, pyridinium, and pyridone. Isotope/H/charge labels and explicit inter-ring
single bonds survive. Aromatic B/P/As/Se can be read/written but the current
integer-assignment model explicitly rejects them.

All aromatic atoms must be cyclic and aromatic bonds must have aromatic
endpoints and lie on cycles. No full Hückel/stability, radical, or sanitization
claim follows from a successful matching. An unsatisfiable pi assignment and
unsupported atomic state have separate typed errors; budget exhaustion is
neither. Aromatic draft input can therefore be read/written but fail depiction.

The model returns a graph clone with identical IDs, endpoints, and allocation
history. It clears aromatic designations and assigns integer orders in that
clone only. Aromatic heteroatom inference policies become fixed zero additional
implicit H, leaving declared H and explicit H vertices untouched. Isotope,
charge, and radical knowledge are retained. The original graph is unchanged.
The existing V1 valence model still refuses aromatic atomic/bond flags directly;
it does not silently interpret them as ordinary integer-bond input.

Search is iterative, deterministic for a given graph, noncanonical, and bounded
by `KekuleOptions::work_budget` (1,000,000 by default). Traversals/state copies
are charged; worst-case search is exponential, not a claim of a polynomial
general matching implementation. Layout and depiction share one representation
adapter with this default; callers can explicitly preprocess with another
budget and use the detached integer graph for both stages.

## Acceptance gates

- `tests/aromatic_smiles.rs`: independently checks input defaults, source spans,
  omitted/explicit bond rules, exact labelled-graph read/write/read equivalence,
  source immutability, double counts, H inference, unsupported states and budgets.
- `chitin-chemdraw/tests/aromatic.rs`: checks the automatic and manual-coordinate
  depiction paths, nitrogen H labels, typed failures, and bond source associations.
- The shared drawing corpus and `layout_regressions` example export fixed SVGs
  for visual inspection, including the preexisting nonaromatic cases.

Visual agreement is assessed chemically, not pixel-for-pixel. Integer bond
assignment must satisfy all participating atoms; glyphs and inner ring strokes
must not be missing. Remaining layout/label collision limitations are not
hidden by widening parser acceptance.

## Tetrahedral stereo boundary

`MolecularGraph` owns center-keyed `TetrahedralStereo` descriptors with four
ordered `StereoLigand` references and `TetrahedralWinding`. Ligands are graph
atoms or one virtual declared H; winding is relative to those references, not
a CIP R/S assignment. The graph checks distinct neighbors, single bonds, no
aromatic center, a fixed-zero additional-H policy, and four total directions.
It does not establish stereogenicity or infer unknown radical state.

The reader captures incoming neighbors, virtual H (after incoming, first at a
root), ring sites in source order, and subsequent branch/continuation neighbors.
Open ring sites reserve positions before their future atom IDs are known. The
writer computes its own incoming/ring/child order and flips winding for an odd
permutation. Reusing the source marker verbatim would invert configurations.

Successful incident bond addition/removal/order changes clear descriptors at
the affected endpoints; removing an atom also clears its neighbors' descriptors.
Replacing center attributes clears its descriptor conservatively. Failed edits
are atomic, and distant edits do not clear unrelated centers. Clones preserve
descriptors; consumers moving atoms between graphs must explicitly remap ligands.

Depiction plans directed center-to-ligand wedges from final coordinates in a
right-handed frame. Reflections reverse the wedge kind, rotations do not.
In skeletal mode, virtual H is resolved as the opposite of the three explicit
ligand vectors. `ChiralHydrogenMode::Explicit` instead gives a declared H on an
annotated carbon its own measured label and C–H wedge. The same signed-volume
calculation preserves configuration in both modes; the graph is not expanded.
Neighboring annotated centers do not share wedge bonds; a center without an
unambiguous independent wedge or with degenerate coordinates returns a typed
error instead of silently losing stereo. An expanded C–H mark does not need an
independent graph bond and suppresses other wedge marks at that center. This is a conservative initial
depiction boundary, not full cage-stereo rendering support.

Solid wedges are generic filled `Polygon` scene primitives; hashed wedges use
short line primitives. Both retain bond provenance, alpha, and label clearance.
SVG consumes these shapes without interpreting chirality. `wedge_width` and
`stroke_width` configure broad-end width and line thickness independently.
Generated H primitives refer to `Source::AttachedHydrogen(center)`, retaining
host provenance without inventing a graph atom or bond identifier. Existing
explicit H vertices and source SMILES remain unchanged.

`tests/stereo.rs` checks relative equivalence/enantiomers, all 24 permutations,
ring sites, virtual/explicit isotope H, invalid coordination, edits, and writer
round trips. Drawing tests add fixed geometric winding examples, reflection/
rotation checks, full penicillin input, degenerate coordinates, and scene bounds.

The default depiction H policy now selects the separate `SmilesOrganicV1`
normal-valence model, so neutral nonaromatic sulfur in this input can be drawn.
It uses N/P 3 or 5 and S 2, 4 or 6, selecting the lowest normal valence that can
accommodate input contributions. The previous `MainGroupClosedShellV1` is
unchanged and remains explicitly selectable; neither model certifies chemistry.
