# chitin-chem

Renderer-independent chemical identities and editable molecular topology. This
is the foundation for future `chitin-bio` adapters and `chitin-chemdraw`, not a
complete cheminformatics toolkit or an editor UI.

## Included

- The 118 named elements, canonical symbols, and atomic-number lookup.
- Explicit isotope labels with mass-number checks (`A >= Z`). These checks do
  not assert that a nuclide exists, is stable, or has a particular atomic mass.
- Atomic isotope, aromatic designation, formal charge, declared attached-H count, implicit-H policy,
  and unpaired-electron count. Unknown values remain unknown; explicit zero is a
  separate fact, not a default.
- Unknown, single, double, triple, and supplied aromatic bond labels.
- Editable simple undirected graphs, stable graph-local atom/bond IDs, neighbor
  queries, deterministic iteration, and connected components.
- Structural validation on mutation: endpoints must exist, self-bonds and
  duplicate unordered pairs are rejected, and atom deletion removes its bonds.
- Read-only, atom-local closed-shell valence analysis, hydrogen composition, and
  structured diagnostics within an explicit, restricted V1 rule set.
- Read-only ring topology: bridges, articulation atoms, and vertex-biconnected
  edge blocks. Block cycle rank does not assign aromaticity or enumerate all rings.
- A restricted SMILES reader with source positions, independent of perception;
  branches, ring closure, fragments, bracket elements/isotopes/H/charges.
- A deterministic noncanonical SMILES writer with strict supplied-fact
  preservation, typed failures, reusable ring labels, and iterative traversal.
- Supplied aromatic atom designations and aromatic SMILES read/write, plus a
  bounded detached C/N/O/S Kekulé representation for integer-bond consumers.
- Ordered tetrahedral graph descriptors and SMILES `@`/`@@`/TH read/write,
  including traversal parity, ring-site order, and virtual declared H.

## Boundaries

`chitin-chem` does not depend on GPUI, `chitin-bio`, `chitin-chemdraw`, or a QM
engine. It owns no coordinates, residue/chain membership, pages, colors, or
viewport state. Existing biological types are not changed by this first step.

Unknown isotope means unspecified isotopic composition, not a selected most
abundant isotope. No atomic-weight or isotope-mass dataset is supplied yet; mass
number must never be substituted for atomic mass. Unpaired-electron count is an
atomic annotation, not a molecular spin multiplicity.

Adding a bond does not infer hydrogens, validate valence, or perceive
aromaticity. Even a chemically implausible draft remains editable. An aromatic
bond label records a supplied representation; it does not establish an
aromaticity model. Wedges and dashes are not encoded as bond orders. Tetrahedral
facts are graph annotations, not CIP assignments. Further stereo classes,
general valence and aromaticity perception, further format codecs, edit
transactions/undo, and drawing remain separate future work. The current analysis
is not full RDKit sanitization.

## SMILES and future formats

```rust
use chitin_chem::io::smiles::{read_smiles, write_smiles};
use chitin_chem::perception::{ValenceModel, analyze_valence};

let parsed = read_smiles("CC(O)C")?;
let report = analyze_valence(parsed.graph(), ValenceModel::MainGroupClosedShellV1)?;
assert_eq!(parsed.graph().atom_count(), 4);
assert!(report.atoms().all(|(_, row)| row.value().is_some()));
let serialized = write_smiles(parsed.graph())?;
let reread = read_smiles(&serialized)?;
assert_eq!(reread.graph().atom_count(), 4);
# Ok::<(), Box<dyn std::error::Error>>(())
```

The reader does not sanitize or modify inferred properties. Bracket atoms
disable implicit H and retain unknown radical state: `[C]` is not ordinary
`C`, and `[NH4+]` is accepted without claiming that its radical state has
already been perceived. Aromatic input designations are preserved independently
of perception. Unsupported directional/non-tetrahedral stereo, query/extension information
is rejected with a positioned error, never discarded. Record names/whitespace
are not silently removed. Empty input represents an empty graph.

The writer preserves all supplied atomic attributes and bond orders, up to
graph isomorphism, without consulting reader provenance or inferring facts.
It does not promise canonical SMILES or the original input spelling. Unknown
charge/H, unsupported bond orders, fixed nonzero implicit H, and facts that
the current reader cannot recover are explicit errors, not silent defaults.
In particular, a known radical count on a bracket atom (even zero) cannot yet
survive this strict round trip. Explicit H vertices are never collapsed.

See [Chemical format architecture](docs/formats.md) for the complete current
capability boundary and staged path to full molecular OpenSMILES, writers,
aromaticity, stereochemistry, canonicalization, and other typed format records.
The [versioned compatibility matrix](docs/smiles-compatibility.md) distinguishes
codec support from chemical-model and SVG support; lowercase B/P/As/Se notation
is representable even though the current drawing model does not cover it.

For supported aromatic inputs, explicitly call `perception::kekulize` with
`KekuleOptions` to obtain integer bonds. Source facts/IDs remain unchanged;
aromaticity perception and general sanitization are not implied.

## Hydrogen semantics

The three H contributions are distinct:

1. Neighbor H vertices are independent graph atoms, including deuterium/tritium.
2. `declared_hydrogens` records attached H as an atomic attribute, such as the
   four H declared by `[NH4+]`, without adding four graph vertices.
3. Additional implicit H are obtained from `ImplicitHydrogenPolicy`:
   `Unspecified`, `Infer`, or `Fixed(n)`. `Fixed(0)` disables inference.

`None` for the declared count is not an established zero. Callers must state
`Some(0)` when they know no attribute-level H are declared. A source reader must
not represent the same hydrogen both as a vertex and as a declaration.

The existing `with_implicit_hydrogens(Some(n))` builder selects `Fixed(n)`;
`None` resets the policy to `Unspecified`. `implicit_hydrogens()` returns only
fixed input counts, never calculated counts. Selecting `Infer` replaces any
fixed count. Inferred results belong exclusively to the analysis report.

## Read-only valence analysis

`perception::analyze_valence(&graph, ValenceModel::MainGroupClosedShellV1)`
returns a detached report keyed by stable atom IDs. Every atom has one status:
`Determined`, `Indeterminate`, `Unsupported`, or `Invalid`, plus structured
diagnostics. Only determined rows expose valence and H counts.

V1 accepts single/double/triple bonds and explicitly zero unpaired electrons. It
uses one target valence per supported element/charge pair:

| Input                   | Target valence |
| ----------------------- | -------------- |
| Neutral H, F, Cl, Br, I | 1              |
| Neutral B, N            | 3              |
| Neutral C, Si           | 4              |
| Neutral O               | 2              |
| N(+1)                   | 4              |
| O(-1)                   | 1              |

Explicit valence is the sum of incident bond orders and declared attached H.
`Infer` fills the supported target; `Fixed(n)` uses exactly the supplied count.
Total valence includes those implicit H. V1 requires its target to be satisfied:
too many is `ExcessValence`, too few with a fixed count is `UnsatisfiedValence`.
These diagnoses describe a violation of **this model**, not universal chemistry.
H vertices may participate in ordinary bonds, but V1 does not attach declared or
additional implicit H to H itself (`HydrogenAsHost`).

Unknown charge, radical state, H declaration/policy, or bond order blocks a
determined result; no unknown facts are replaced by zeros. Metals, P/S and other
unlisted elements, unlisted charge states, nonzero radicals, and aromatic bond
labels are explicitly unsupported. Aromatic bonds are never treated as 1.5.
Atomic aromatic designations also block V1, including on explicit integer bonds;
choose a detached Kekulé representation first rather than ignoring the flags.
Unsupported takes precedence over missing facts in the status, while all reasons
are retained. Chemical-invalid outcomes require complete supported input.

Reports do not mutate atoms, add H, change bonds/IDs, or install a property
cache. They are snapshots, not self-updating views: recompute after edits. One
bad fragment does not suppress other rows, and a locally determined result is
not proof of molecular stability or suitability for a quantum calculation.

The separately selected `SmilesOrganicV1` adds neutral N/P normal valences 3/5
and S 2/4/6. It chooses the lowest value not below known input contributions;
missing facts, unknown radicals, aromatic labels, and unlisted charge states
still block inference. This does not widen the original main-group V1 contract.

## Tetrahedral configurations

`graph.tetrahedral(center)` exposes four ordered atom/H references and their
relative winding. `set_tetrahedral` validates connectivity and single-bond
coordination, not stereogenicity. Parsing reserves ring-opening positions and
writing corrects odd reference permutations. Unknown stereo stays absent; no
chirality is inferred from coordinates or atom ID order.

Incident topology/order edits and center-attribute replacement conservatively
clear affected descriptors. Cloning preserves them; transfer to another graph
requires explicit reference remapping. See the
[compatibility matrix](docs/smiles-compatibility.md#tetrahedral-stereo-boundary)
for virtual-H, mutation, writer, and initial wedge-rendering limits.

The separation of input facts from detached calculations is informed by
[COSMolKit's valence layer](https://github.com/cosmol-studio/COSMolKit/blob/main/crates/cosmolkit-core/src/valence.rs).
V1 is a small independent rule set, not a claim of RDKit/COSMolKit behavioral
parity, a complete SMILES interpretation, or a full sanitization pipeline.

## Alignment references

The [OpenSMILES specification](https://opensmiles.org/opensmiles.html) defines
the intended molecular notation semantics. Local source audits used
[SmilesDrawer](https://github.com/reymond-group/smilesDrawer) (the parser grammar,
atomic H handling, and SVG aromatic rendering) and
[COSMolKit](https://github.com/cosmol-studio/COSMolKit) (the separation of
source designations, integer assignment, and chemical validation) as capability
references, not dependencies or conformance oracles. New regression fixtures
and the bounded assignment implementation are locally authored. This is not
a claim of full behavioral or visual parity with either project.

## Graph identity and mutation

- IDs are opaque handles in one graph, not dense offsets, source serial numbers,
  or reaction mapping numbers. Do not pass handles between unrelated graphs;
  equal numeric values can exist in different namespaces.
- Deletion never compacts or reuses IDs. Clones preserve a snapshot's handles;
  they are not a mapping between unrelated molecules.
- Atom attributes can be replaced without touching connectivity. Graph storage
  and edge endpoints are private; callers cannot create dangling references.
- Failed operations leave state, including allocation counters, unchanged.
- An edge's endpoint order is retained, although adjacency is undirected.
  Multiple chemical bonds are one edge with an order label, not parallel edges.
- Removing an atom cascades to incident bonds. The future edit/history layer
  must capture both before deletion; this mutation API is not itself an undo
  API.
- Ordered maps give deterministic traversal, not canonical chemical ordering.
  This small baseline favors explicit invariants over a premature arena/trait
  framework; large-system performance should be measured before changing
  storage.

## Example

```rust
use chitin_chem::{Atom, BondOrder, Element, MolecularGraph};

let mut graph = MolecularGraph::new();
let carbon = graph.add_atom(Atom::new(Element::Carbon))?;
let oxygen = graph.add_atom(
  Atom::new(Element::Oxygen).with_formal_charge(Some(0)),
)?;
graph.add_bond(carbon, oxygen, BondOrder::Single)?;
assert_eq!(graph.connected_components(), vec![vec![carbon, oxygen]]);
# Ok::<(), chitin_chem::GraphError>(())
```

Element names and symbols follow the
[IUPAC periodic table](https://iupac.org/what-we-do/periodic-table-of-elements/).
Element parsing is intentionally strict (`Cl`, not `CL`); format-specific
readers will handle source normalization and `D`/`T` isotope aliases explicitly.

## Validation

```sh
cargo test -p chitin-chem --locked
cargo clippy -p chitin-chem --all-targets --locked -- -D warnings
cargo fmt --all --check
```
