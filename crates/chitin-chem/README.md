# chitin-chem

Renderer-independent chemical identities and editable molecular topology.
This is the foundation for future `chitin-bio` adapters and `chitin-chemdraw`,
not a complete cheminformatics toolkit or an editor UI.

## Included

- The 118 named elements, canonical symbols, and atomic-number lookup.
- Explicit isotope labels with mass-number checks (`A >= Z`). These checks do
  not assert that a nuclide exists, is stable, or has a particular atomic mass.
- Atomic isotope, formal charge, implicit H count, and unpaired-electron count.
  Unknown values remain `None`; explicit zero is a separate fact.
- Unknown, single, double, triple, and supplied aromatic bond labels.
- Editable simple undirected graphs, stable graph-local atom/bond IDs, neighbor
  queries, deterministic iteration, and connected components.
- Structural validation on mutation: endpoints must exist, self-bonds and
  duplicate unordered pairs are rejected, and atom deletion removes its bonds.

## Boundaries

`chitin-chem` does not depend on GPUI, `chitin-bio`, `chitin-chemdraw`, or a QM
engine. It owns no coordinates, residue/chain membership, pages, colors, or
viewport state. Existing biological types are not changed by this first step.

Unknown isotope means unspecified isotopic composition, not a selected most
abundant isotope. No atomic-weight or isotope-mass dataset is supplied yet;
mass number must never be substituted for atomic mass. Unpaired-electron count
is an atomic annotation, not a molecular spin multiplicity.

Adding a bond does not infer hydrogens, validate valence, or perceive aromaticity.
Even a chemically implausible draft remains editable. An aromatic bond label
records a supplied representation; it does not establish an aromaticity model.
Wedges and dashes are not encoded as bond orders. Stereo, valence, aromaticity,
format codecs, edit transactions/undo, and drawing will be implemented separately.

## Graph identity and mutation

- IDs are opaque handles in one graph, not dense offsets, source serial numbers,
  or reaction mapping numbers. Do not pass handles between unrelated graphs;
  equal numeric values can exist in different namespaces.
- Deletion never compacts or reuses IDs. Clones preserve a snapshot's handles;
  they are not a mapping between unrelated molecules.
- Atom attributes can be replaced without touching connectivity. Graph storage
  and edge endpoints are private; callers cannot create dangling references.
- Failed operations leave state, including allocation counters, unchanged.
- An edge's endpoint order is retained, although adjacency is undirected. Multiple
  chemical bonds are one edge with an order label, not parallel edges.
- Removing an atom cascades to incident bonds. The future edit/history layer
  must capture both before deletion; this mutation API is not itself an undo API.
- Ordered maps give deterministic traversal, not canonical chemical ordering.
  This small baseline favors explicit invariants over a premature arena/trait
  framework; large-system performance should be measured before changing storage.

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
Element parsing is intentionally strict (`Cl`, not `CL`); format-specific readers
will handle source normalization and `D`/`T` isotope aliases explicitly.

## Validation

```sh
cargo test -p chitin-chem --locked
cargo clippy -p chitin-chem --all-targets --locked -- -D warnings
cargo fmt --all --check
```
