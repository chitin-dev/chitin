#import "@preview/fletcher:0.5.8" as fletcher: diagram, edge, node
#import "/book.typ": book-page
#show: book-page
#let source-root = "https://github.com/chitin-dev/chitin/blob/main/crates/chitin-bio/src"

= 1 Biological structure layer
<biological-structure-layer>

`chitin-bio` is the format-independent domain layer for molecular structures. It
converts PDB and PDBx/mmCIF input into a validated structure model that can be
consumed by command-line tools, renderers, and future analysis packages.

The layer has three responsibilities:

- interpret supported biological structure formats;
- normalize format-specific records into stable domain concepts;
- validate relationships and preserve diagnostics and source provenance.

It deliberately does not own windows, GPU resources, network downloads, user
interface state, or visual style. Those concerns belong to the application,
renderer, and infrastructure layers that consume this model.

== 1.1 Source map
<source-map>

The public module boundary is declared in
#link(source-root + "/lib.rs")[`src/lib.rs`]. It exposes the three main
subsystems without coupling them to the desktop or browser applications:

- #link(source-root + "/structure/mod.rs")[`structure`] contains parsers,
  normalized models, diagnostics, and renderer-neutral scene extraction;
- #link(source-root + "/chemistry/mod.rs")[`chemistry`] contains chemical
  derivations such as geometric bond inference;
- #link(source-root + "/surface/mod.rs")[`surface`] contains molecular-surface
  algorithms and their mesh artifacts.

The following table connects the conceptual pipeline to the current source
entry points:

#table(
  columns: (1.4fr, 2fr, 3fr),
  [Stage], [Source entry point], [Role],

  [Format parsing],
  [#link(source-root + "/structure/pdb.rs")[`PdbParser`] and
   #link(source-root + "/structure/mmcif.rs")[`MmcifParser`]],
  [Read PDB and PDBx/mmCIF input into a common parse result.],

  [Validation result],
  [#link(source-root + "/structure/error.rs")[`StructureParseResult`] and
   diagnostics],
  [Carry the validated structure together with recoverable diagnostics and
   format-specific errors.],

  [Stable model],
  [#link(source-root + "/structure/model.rs")[`Atom` `Residue` `Chain`
   `Model` `CoordinateSet` `Bond`]],
  [Represent identity, topology, coordinates, and metadata independently of
   presentation.],

  [Scene extraction],
  [#link(source-root + "/structure/scene.rs")[`StructureScene`]],
  [Convert the selected model and connectivity into renderer-neutral instances,
   bounds, and polymer traces.],

  [Bond derivation],
  [#link(source-root + "/chemistry/bond_inference.rs")[`infer_bonds`]],
  [Produce explicitly derived bonds while retaining their inference provenance.],
  [Surface derivation],

  [#link(source-root + "/surface/implicit/mod.rs")[implicit surface] and
   #link(source-root + "/surface/msms.rs")[MSMS surface]],
  [Produce surface-domain artifacts that can later be uploaded or converted by
   a renderer.],
)

== 1.2 Responsibilities and boundaries
<responsibilities-and-boundaries>

The biological structure layer is the boundary between serialized structure
files and the rest of Chitin. Its input is a byte stream or a format-specific
reader input. Its output is a structure snapshot together with diagnostics. The
output is independent of whether the source was PDB or PDBx/mmCIF.

The pipeline consists of six stages:

1. *Serialized structure*: PDB or PDBx/mmCIF bytes enter the domain layer.
2. *Format reader*: the appropriate reader interprets the source format and its
  record vocabulary.
3. *Semantic records*: format-specific records are projected into atoms,
  residues, chains, models, bonds, and metadata.
4. *Identity and relationship validation*: references, coordinates, and
  cross-record invariants are checked.
5. *Validated structure snapshot and diagnostics*: consumers receive a stable
  model together with recoverable warnings and source information.
6. *Analysis, scene extraction, or rendering*: downstream systems derive the
  data they need without reinterpreting the original file.

The reader owns syntax and format vocabulary. The domain model owns stable
identities, relationships, coordinate states, and source metadata. A renderer
owns the conversion from a validated snapshot to GPU-friendly geometry. This
separation prevents a rendering decision from changing the meaning of a source
record.

The boundary also defines failure ownership. A malformed coordinate is a parsing
or validation error; a missing GPU buffer is a rendering error; a network
timeout is an infrastructure error. The biological layer should not turn one
category into another merely to make a downstream API convenient.

== 1.3 Conceptual flow
<conceptual-flow>

#figure(
  diagram(
    node-stroke: 1pt,
    node-corner-radius: 4pt,
    node((0, 0), [Raw PDB / PDBx-mmCIF file]),
    edge("-|>"),
    node((0, 0.8), [format reader (validated structure and diagnostics)]),
    edge("-|>"),
    node((0, 1.6), [scene extraction / analysis / rendering]),
  ),
  caption: [The biological structure pipeline. A format reader interprets raw
    PDB or PDBx/mmCIF records and produces a `StructureParseResult`. The result
    contains a validated structure snapshot and diagnostics that can be consumed
    by analysis and rendering without knowing which serialization was used.],
)

The stages are intentionally ordered:

1. The input reader identifies records according to the source format.
2. The reader converts records into common atoms, residues, chains, models,
  bonds, and metadata.
3. Cross-record relationships are validated before the result is exposed to
  consumers.
4. Derived data, such as inferred bonds or renderable surfaces, is built from
  the validated snapshot and remains distinguishable from source data.

PDB and mmCIF are different serializations, not different domain models. A
caller can switch formats without changing how it addresses an atom, residue,
chain, or coordinate state.

== 1.4 Structure snapshot and stable identity
<structure-snapshot-and-stable-identity>

A structure snapshot describes one logical molecular input. It contains stable
identity and topology tables together with one or more coordinate states:

Structure snapshot:

- atom identity and element information
- residue and chain membership
- source and derived bonds
- coordinate states / models
- unit-cell and symmetry metadata
- source identifiers and diagnostics

*Atoms belong to residues, residues belong to chains, and bonds connect atoms*.
These relationships describe what the structure is. Coordinates describe where
the structure is in a particular model or state. Keeping those concepts separate
is important for multi-model PDB files, NMR ensembles, alternate locations, and
future trajectory data.

== 1.5 Stable identity and changing coordinates
<stable-identity-and-changing-coordinates>

Atom identity and topology are shared by coordinate states. A coordinate state
changes positions and related per-state values, but does not silently create a
second atom table.

For example, the alpha carbon of a residue may have one position in model 1 and
another position in model 2. Both positions refer to the same logical atom: its
element, residue membership, chain membership, source identifier, and bond
relationships do not change merely because the active coordinate state changes.

Conceptually, the model has one stable identity and topology layer, together
with one coordinate position array for each active state. The coordinate arrays
are indexed by the stable atom identities; they are not independent molecules.

Each active coordinate state must provide a position for every atom it
addresses. If a source format cannot provide a complete shared identity model,
the parser must represent that limitation explicitly rather than silently
merging unrelated atoms.

The renderer may derive GPU data from one selected state, but the source model
remains authoritative for identifiers, metadata, and provenance. Changing the
selected state should invalidate position-dependent geometry, bounds, and
spatial acceleration structures; it should not invalidate atom identity or
source connectivity.

== 1.6 Format normalization
<format-normalization>

PDB and PDBx/mmCIF expose similar scientific concepts through different
mechanisms. PDB uses fixed-width records and columns, while PDBx/mmCIF uses data
blocks, categories, loops, optional values, and dictionary-defined fields. The
readers preserve these distinctions while projecting both formats into the same
domain concepts.

The normalization policy is:

- preserve author and label identifiers instead of choosing one and discarding
  the other;
- keep model numbers, alternate locations, occupancy, and temperature factors
  when present;
- distinguish missing, unknown, and empty values where the distinction affects
  interpretation;
- preserve unit-cell, symmetry, and biological-assembly metadata;
- keep explicit source connectivity separate from inferred connectivity.

Normalization is not the same as guessing. A field that is absent in the input
must not be fabricated simply because a renderer would prefer a value. Derived
values are marked as derived and retain enough provenance to explain how they
were produced.

== 1.7 Validation and diagnostics
<validation-and-diagnostics>

Validation happens at the earliest boundary that has enough information to
explain the problem. Syntax errors are reported by the format reader. Semantic
errors are reported when records are normalized into domain relationships.
Consumers should not be the first components to discover a broken structure
table.

At minimum, a successful structure snapshot must satisfy these invariants:

- every atom refers to an existing residue;
- every residue refers to an existing chain;
- every bond endpoint refers to an existing atom;
- every active coordinate state has valid coordinates for its addressed atoms;
- coordinate and unit-cell values are finite and physically meaningful;
- source identifiers remain traceable to the records from which they came.

Diagnostics describe recoverable omissions and format details without changing
the validity of the structure. A fatal error is reserved for a condition that
would make identity, relationships, or coordinates ambiguous or unsafe to
consume.

This distinction lets callers choose an appropriate policy. A command-line
inspection command can display every diagnostic, while an interactive viewer can
show a warning and continue when the validated structure is still usable.

== 1.8 Derived data and provenance
<derived-data-and-provenance>

The structure snapshot is the source of truth. Algorithms may derive additional
data for analysis or rendering, including:

- distance-based bond inference;
- spatial grids and neighbor queries;
- polymer and secondary-structure annotations;
- atom, cartoon, and surface geometry;
- bounds and spatial acceleration structures.

Derived data must remain identifiable as derived. For example, a geometrically
inferred bond should not be presented as if it had been explicitly recorded in
the input file. A derived geometry object should retain the atom, residue, or
chain identities that produced it so picking, diagnostics, and later analysis
can map it back to the domain model.

When the source snapshot changes, consumers should invalidate only the derived
data affected by that change. A coordinate change invalidates position-dependent
geometry; a topology change invalidates connectivity-dependent representations;
a style change should invalidate presentation data but not the biological
snapshot itself.

== 1.9 Consumers and dependency direction
<consumers-and-dependency-direction>

The layer is designed to support several consumers without making any of them
the owner of biological meaning:

#figure(
  diagram(
    node-stroke: 1pt,
    node-corner-radius: 4pt,
    node((0, 0), [Validated structure]),
    node((3, -1.5), [Command-line inspection and validation]),
    node((3, -0.5), [Analysis and derived algorithms]),
    node((3, 0.5), [Renderer-neutral scene extraction]),
    node((3, 1.5), [Desktop and browser presentation]),
    edge((0, 0), (3, -1.5), "-|>"),
    edge((0, 0), (3, -0.5), "-|>"),
    edge((0, 0), (3, 0.5), "-|>"),
    edge((0, 0), (3, 1.5), "-|>"),
  ),
  caption: [One validated structure snapshot can feed inspection, analysis,
  scene extraction, and application presentation.]
)

The dependency direction is one-way. UI and GPU crates may depend on the domain
model, but the domain model must not depend on GPUI, wgpu, browser APIs, or a
particular file-download mechanism. This keeps parsing and validation usable in
tests, command-line tools, desktop applications, and WebAssembly workers.

Rendering consumes a validated structure or a renderer-neutral scene. It does
not modify source records or decide whether a parsed value was scientifically
valid. Representation choices such as stick, cartoon, or surface are visual
policies applied after the biological structure has been validated.

== 1.10 Design consequences
<design-consequences>

This architecture leads to several practical rules:

- parsers should return structured errors and diagnostics rather than panic;
- consumers should use stable domain identities instead of array positions that
  can change when a state or representation is rebuilt;
- coordinate-state changes should be incremental where possible;
- source and inferred connectivity must remain distinguishable;
- visual layers may be composed without duplicating the source structure;
- format-specific behavior belongs in readers or explicit conversion steps, not
  in renderer code.

These rules allow Chitin to add new readers, trajectory states, analysis
algorithms, and presentation backends without changing the meaning of existing
structure data.
