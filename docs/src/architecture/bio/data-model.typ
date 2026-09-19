#import "/book.typ": book-page
#show: book-page

#let source-root = "https://github.com/chitin-dev/chitin/blob/main/crates/chitin-bio/src"

= 1 Structure data model
<structure-data-model>

The structure model is a validated snapshot of molecular identity, topology,
coordinates, and crystallographic metadata. It is independent of GPUI, wgpu,
browser APIs, and file-download mechanisms.

== 1.1 Main concepts
<main-concepts>

The snapshot contains four related groups of data:

- *identity*: atoms, elements, residues, chains, and source identifiers;
- *topology*: source and inferred bonds, bond order, and bond provenance;
- *state*: coordinate-bearing models and their Cartesian positions;
- *metadata*: unit-cell, symmetry, biological-assembly, and source information.

Atoms belong to residues, residues belong to chains, and bonds connect atoms.
The references between these records are stable within a structure snapshot.
They are used by parsers, analysis algorithms, scene extraction, selection, and
rendering.

The public model types are defined in
#link(source-root + "/structure/model.rs")[`structure/model.rs`]. The module
also exposes typed identifiers such as `AtomId`, `ResidueId`, `ChainId`,
`ModelId`, and `CoordinateSetId` so consumers do not need to use untyped array
indices as scientific identities.

== 1.2 Identity versus state
<identity-versus-state>

The distinction between identity and state comes from a basic property of
structure files: one molecular identity can be observed at more than one set of
coordinates. This is common in NMR ensembles, where several conformers are
stored as `MODEL` records, but it is not limited to NMR. It also applies to
multi-model PDB or mmCIF files, multiple experimental models, and future
trajectory or conformational data.

The parser therefore stores two different kinds of information:

- *identity and topology*: atoms, elements, residues, chains, bond endpoints,
  bond provenance, and source identifiers;
- *coordinate state*: Cartesian positions for a particular model, together with
  its source model number and chain membership.

In the implementation, [`Model` and `CoordinateSet`](/home/ashgrey/Github/chitin/crates/chitin-bio/src/structure/model.rs)
make this relationship explicit. A `Model` stores a `coordinate_set_id`, while
the corresponding `CoordinateSet` stores positions indexed by `AtomId`. The
atom table is not duplicated into a new independent topology table for every
model.

For atom $a$ and coordinate state $k$, the Cartesian position is:

$ bold(x)_(k,a) = (x_(k,a), y_(k,a), z_(k,a)) in bb(R)^3 $

Changing $k$ changes $bold(x)_(k,a)$, not the atom's identifier, element,
residue membership, chain membership, or source bonds. Model 1's alpha carbon
and model 2's alpha carbon are two observations of the same logical atom when
the source establishes that correspondence.

=== 1.2.1 Why NMR commonly produces multiple states
<nmr-and-multiple-states>

NMR structures are often deposited as ensembles because the experiment can
support several conformations that satisfy the measured restraints. A typical
file repeats the same atom and residue topology inside several `MODEL` blocks
while changing the coordinates. The PDB reader projects those blocks into
separate models, and the shared builder keeps their topology aligned.

The PDB entry point is
#link(source-root + "/structure/pdb.rs")[`PdbParser::parse_bytes`]. The mmCIF
reader applies the same model concept to atom-site model numbers through
#link(source-root + "/structure/mmcif.rs")[`MmcifParser::parse_bytes`]. NMR is
therefore an important example, not a special case in the data model.

=== 1.2.2 What is and is not a coordinate state
<coordinate-state-boundary>

A coordinate state is a complete coordinate assignment associated with one
model. It should not be confused with every kind of positional variation in a
structure file:

- alternate locations may describe mutually exclusive local conformations and
  remain source-level atom information; they do not automatically become
  independent `Model` values;
- crystallographic symmetry operations describe generated copies of a source
  structure and do not automatically duplicate the source atom table;
- a biological assembly describes how source chains can be arranged and remains
  metadata until an explicit assembly-expansion operation is requested;
- a trajectory frame is a natural future coordinate state, but it should still
  reference stable identity and topology rather than recreate them.

This boundary prevents a viewer from treating every position-like field as a
new molecule. It also leaves room for alternate-location selection and symmetry
expansion without changing the meaning of `Model` and `CoordinateSet`.

=== 1.2.3 Consequences for consumers
<identity-state-consumers>

When a caller selects a different state, it should update only data that
depends on coordinates. The renderer may rebuild atom instances, cartoon
traces, bond geometry, surface meshes, bounds, and spatial acceleration data.
It should not rebuild the atom identity table, residue and chain references, or
source bond provenance solely because the selected coordinates changed.

The renderer-neutral scene builder follows this boundary in
#link(source-root + "/structure/scene.rs")[`structure/scene.rs`]. A pick result
can identify an `AtomId` or `ResidueId` regardless of which coordinate state is
displayed, while the displayed position is resolved through the selected
`CoordinateSet`.

== 1.3 References and invariants
<references-and-invariants>

A valid structure satisfies these reference invariants:

- every atom has an existing residue owner;
- every residue has an existing chain owner;
- every bond endpoint refers to an existing atom;
- every model refers to one coordinate set;
- every coordinate set is indexed consistently with the atom table.

These are data-model invariants, so they are expressed as prose rather than as
equations between software objects. Invalid references are parse failures, not
deferred renderer errors.

The builder and model validation code lives in
#link(source-root + "/structure/builder.rs")[`structure/builder.rs`] and
#link(source-root + "/structure/model.rs")[`structure/model.rs`]. A successful
parse returns the validated structure together with recoverable diagnostics;
see #link(source-root + "/structure/error.rs")[`structure/error.rs`].

== 1.4 Coordinate and unit-cell conventions
<coordinate-and-unit-cell-conventions>

Cartesian coordinates are stored in ångström units and use a right-handed
coordinate frame. Fractional coordinates use the unit-cell basis:

$ bold(x) = A bold(f) $

Here $bold(x)$ is the Cartesian position, $bold(f)$ is the fractional position,
and $A$ is the unit-cell basis matrix. The conversion is defined in more detail
by the #link("./structure.typ#units-and-coordinate-systems")[structure model
contract].

Unit-cell edge lengths $a$, $b$, and $c$ are measured in ångströms. The angles
$alpha$, $beta$, and $gamma$ are measured in degrees. The valid range is:

$ a > 0 and b > 0 and c > 0 $

$ 0 degree < alpha < 180 degree $

$ 0 degree < beta < 180 degree $

$ 0 degree < gamma < 180 degree $

The basis must be non-singular. Its volume is positive:

$ V = abs(det(A)) > 0 $

Reading unit-cell metadata does not automatically expand symmetry mates or
duplicate atoms. Such an expansion must be an explicit operation with its own
identity and provenance policy.

== 1.5 Derived data and provenance
<derived-data-and-provenance>

Source bonds and inferred bonds remain distinguishable. A renderer may draw
both, while analysis tools can restrict themselves to source connectivity.
Derived geometry retains the identities of the atoms, residues, or chains that
produced it so that selection, diagnostics, and picking can map back to the
domain model.

When the source snapshot changes, consumers should invalidate only the derived
data affected by that change. A coordinate change invalidates position-dependent
geometry; a topology change invalidates connectivity-dependent representations;
a style change invalidates presentation data but not the biological snapshot.
