#import "/book.typ": book-page
#show: book-page

#let source-root = "https://github.com/chitin-dev/chitin/blob/main/crates/chitin-chem/src"

= 1 Chemical graph model
<chemical-graph-model>

`chitin-chem` stores renderer-independent molecular facts and editable
topology. It is intentionally smaller than a full cheminformatics toolkit:
the graph records what the input states, while perception modules return
detached reports instead of silently modifying the graph.

== 1.1 Ownership boundary
<chemical-ownership>

The core types are defined in
#link(source-root + "/atom.rs")[`atom.rs`],
#link(source-root + "/bond.rs")[`bond.rs`], and
#link(source-root + "/graph.rs")[`graph.rs`]. They own:

- element, isotope, aromatic, charge, hydrogen, and radical annotations;
- labelled undirected bonds and stable graph-local atom and bond identifiers;
- deterministic adjacency and connected-component queries;
- mutation invariants that prevent dangling endpoints, self-bonds, and
  duplicate unordered pairs.

They do not own coordinates, residues, chains, pages, colors, viewport state,
SMILES source text, UI history, or inference caches. A numeric ID is meaningful
only inside the graph that allocated it; equal numbers from two graphs are not
the same atom.

== 1.2 Facts versus derived reports
<facts-versus-reports>

Parsing installs source facts. Perception consumes a graph snapshot and returns
a report keyed by the same IDs. This separation makes uncertainty explicit:
unknown charge, hydrogen policy, radical state, bond order, or element support
is not replaced by a convenient zero.

For a selected valence model, the explicit valence of atom $i$ is

$ V_i = sum_(j in N(i)) b_(i j) + H_i $

where $b_(i j)$ is the supplied bond-order contribution and $H_i$ is either a
declared hydrogen count or a value inferred by the selected policy. The formula
is evaluated only when the model has enough facts; it is not a universal
chemical law and a determined row is not a stability or quantum-chemistry
claim.

The read-only analysis entry point is
#link(source-root + "/perception/valence.rs")[`perception::analyze_valence`].
Its statuses distinguish determined, indeterminate, unsupported, and invalid
rows. Reports do not mutate atoms, add hydrogens, change bond orders, or remain
live after a graph edit.

== 1.3 Hydrogen semantics
<hydrogen-semantics>

Three different facts must not be conflated:

- an explicit H, D, or T vertex is an independent graph atom;
- `declared_hydrogens` records attached hydrogen in an atom annotation;
- `ImplicitHydrogenPolicy` controls whether an analysis may infer additional H.

`None` means that the declaration is unknown, not that it is zero. Bracket
atoms preserve this distinction. A reader must not represent the same hydrogen
as both a graph vertex and a declaration.

== 1.4 Topology and rings
<topology-and-rings>

#link(source-root + "/perception/rings.rs")[`perception::analyze_rings`] returns
bridges, articulation atoms, and vertex-biconnected edge blocks. The cycle-space
rank of a connected graph is

$ mu = E - V + 1 $

but $mu$ is not a catalogue of chemical rings and does not assign aromaticity.
The result is a detached topological report; it does not install a ring cache or
modify the graph.

== 1.5 Stereo facts
<stereo-facts>

Tetrahedral descriptors store an ordered set of graph references and a relative
winding. SMILES `@`, `@@`, `@TH1`, and `@TH2` are preserved as source facts when
the graph can represent them. They are not CIP labels, and the crate does not
infer chirality from coordinates or atom-ID order. Topology edits conservatively
clear descriptors that are no longer valid.

== 1.6 Example
<chemical-graph-example>

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

The graph is suitable as input to a parser, perception report, renderer, or
editor transaction, but none of those consumers is allowed to reinterpret an
unknown fact as a known one.
