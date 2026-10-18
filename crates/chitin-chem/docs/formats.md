# Chemical format architecture and implementation plan

## Goal and compatibility contract

Chitin will implement chemical formats natively. The target for molecular
SMILES is the complete [OpenSMILES specification](https://opensmiles.org/opensmiles.html),
with documented, explicitly selected extensions rather than an ambiguous claim
to support every vendor dialect. SMARTS, reaction SMILES, CXSMILES, and SMILES
record files are separate capabilities, not implied by a molecular reader.
COSMolKit is an architectural reference, not a dependency or conformance oracle.

The current implementation is deliberately incomplete. Its public API must
report unsupported information, never accept an input by deleting stereo,
aromaticity, atom classes, query predicates, or extension fields. Acceptance is
not a chemical validity, stability, or quantum-calculation suitability claim.

## Ownership

```text
io::<format> syntax + source locations
    -> detached molecular facts + format provenance
    -> explicit perception model(s) -> detached reports

domain graph + supported annotations -> io::<format> writer
```

- `Atom`, `Bond`, and `MolecularGraph` own format-independent chemical input
  and connectivity. They do not own a SMILES parser, canonical ranks, ring
  numbers, UI state, or inference caches.
- `io::smiles::syntax` scans positioned tokens; `parser` assembles topology.
  Neither invokes perception. Both are private so grammar implementation can
  evolve without freezing a premature public AST.
- `io::smiles::writer` separates attribute encoding, a private traversal plan,
  and text emission. `profile` is the shared lexical capability boundary for
  both directions, not a chemical valence table. The writer accepts a borrowed
  graph, which currently contains all supported chemical facts; it does not
  require a reader snapshot or reuse old source text. Add a typed borrowed
  annotation view when new domain semantics actually require it.
- `ParsedSmiles` owns an immutable graph, exact source text, atom token spans,
  and bond source sites. Ring bonds have two sites; omitted ordinary bond
  symbols have empty spans. Provenance is not a general mutable annotation bag.
  Consuming the snapshot for editing drops its provenance, preventing stale
  source maps from masquerading as current topology metadata.
- `perception` owns named chemistry policies. Missing facts, unsupported
  chemistry, and invalid valence are distinct from malformed source syntax.
  Parsing must not acquire hidden sanitization, property-cache, or live-molecule
  installation side effects.
- Future format records own titles, properties, conformers, coordinates, and
  file-level metadata in typed structures alongside the graph. A codec cannot
  silently flatten those data into a graph-only return type.
- Codecs stay GPUI-independent. The desktop/import boundary chooses the reader,
  reports diagnostics, runs background work, and commits a successful result.

No universal codec trait, format registry, or additional crate is needed with
only one format. Extract shared abstractions when two concrete implementations
demonstrate the same contract, not by forcing different formats into a
SMILES-shaped interface. A dedicated I/O crate can follow if dependencies or
build cost justify it; the module boundary already isolates that work.

## Current reader

`io::smiles::read_smiles(&str)` supports:

- Bare `B C N O P S F Cl Br I`, neutral with zero declared attached H,
  an explicit closed-shell reading, and permission for implicit-H inference.
- Bracketed named elements (all 118), isotope mass numbers, declared H, signed
  numeric charges, and the legacy `++`/`--` charge spelling.
- Bare aromatic `b c n o p s`, bracketed aromatic symbols including `as`/`se`,
  and explicit `:` bonds. These are retained input designations, not perceived
  aromaticity or an assertion that a Kekulé representation exists.
- Implicit/explicit single, double and triple bonds; nested branches; dot
  disconnections; ring labels `0` through `9` and `%00` through `%99`.
- Ring-label reuse after closure, compatible orders on either closure site,
  and closure across dots. Ring labels describe edges, not ring perception.
- Omitted bonds between two aromatic atoms default to aromatic; all other
  omitted bonds default to single. Explicit `-` overrides this at ordinary
  edges and either ring-closure site. Writing retains that distinction.
- Empty input as an empty graph. Leading/trailing whitespace, names, line
  endings, comments and extension text are not silently consumed.

Numeric spellings follow the OpenSMILES grammar: at most three isotope digits,
one attached-H digit, and two numeric-charge digits. Oversized fields are
explicit read errors even though domain integers could store larger values.
Acceptance of these ranges is not a claim of chemical plausibility or a complete
OpenSMILES conformance profile. The isotope model additionally requires
mass number >= atomic number; unsupported isotope labels are not normalized.

Brackets make absent charge/H explicit zero and disable additional implicit H.
They do **not** establish the number of unpaired electrons: `[C]`, `[CH3]`,
`[CH4]`, and `[NH4+]` retain an unknown radical state at read time. Consequently
the current V1 valence analysis reports them indeterminate until a caller or
future dedicated perception model establishes that fact. It must not label
`[CH3]` closed-shell merely because the parser accepted it. Declared H does not
create graph vertices; explicit `[H]` and isotope-labelled H remain vertices.

Errors carry UTF-8 byte ranges and structured categories for syntax,
unsupported features, numeric representation, element/isotope representation,
and graph topology. There is no partial success result. Ring/branch errors
point at the relevant opening site; end-of-input errors may have empty spans.
The scanner completes before graph construction; this is fail-fast, not a
multi-error recovery or best-diagnostic-priority API. Ring labels must precede
an atom's branches, as specified by the formal grammar.

The parser is iterative with heap-backed branch/ring state. For input length
`n`, graph ordered maps make assembly O(n log n) in the worst case; auxiliary
storage is O(n). It does not use recursive descent or a quadratic rescanning
loop. Future untrusted file import should add explicit caller-selected resource
budgets (bytes/atoms/nesting) at its boundary, not silently truncate molecules.

## Current noncanonical writer

`io::smiles::write_smiles(&graph)` returns an owned string or a structured
`SmilesWriteError`. It borrows the graph and performs no perception, repairs,
chemical normalization, or source-dependent rewriting. Empty graphs write as
empty strings. Error locations use input atom/bond handles, not obsolete source
offsets. Output is returned only after complete success, never streamed as a
partial molecule to a caller-owned sink.

The round-trip contract is **exact supplied attributes and labelled undirected
connectivity up to graph isomorphism**. It is not an assertion of equal IDs,
byte-for-byte source spelling, chemical equivalence after normalization, or
identical radical/H knowledge after throwing away annotations. In particular:

- Bare organic-subset atoms are emitted only if every fact matches the reader:
  no isotope, known neutral charge, known zero declared H, known zero unpaired
  electrons, and `Infer` policy.
- Bracket atoms require known charge, known declared H, `Fixed(0)` additional
  implicit H, and an unresolved radical state. The shared numeric limits apply.
  All named elements and isotope-labelled H vertices are retained.
- Unknown charge/H and unspecified H policy are errors, not assumed zeros.
  `Fixed(n > 0)` implicit H is rejected rather than merged into declared H.
  A known bracket radical count, **including zero**, is rejected because the
  current reader cannot recover that fact. Broader chemical interpretation
  belongs to the later model/perception work, not an implicit writer fix-up.
- Unknown bond orders are rejected; aromatic orders are written as `:`. Aromatic
  atomic designations use lowercase symbols; aromatic singles are explicit `-`.
  A valence-invalid draft using
  representable facts may still serialize: writing does not certify chemistry.
- Input spellings such as `[CH0]`, `[CH1]`, `[C+0]`, and `[Fe++]` may become
  `[C]`, `[CH]`, `[C]`, and `[Fe+2]` respectively; supplied attributes are retained.

Traversal uses a deterministic iterative DFS spanning forest over temporary
dense indices. Graph IDs are neither compacted nor changed. Tree edges become
branches/continuations; non-tree edges become ring sites before an atom's
branches. These edges are serialization bookkeeping, not perceived rings or a
canonical ranking. Atom insertion order may change output; source ordering
and canonical SMILES are not promised.

The emitter reuses labels `0` through `%99` after closure, closing existing
sites before opening new ones at the same atom. The total number of cycles is
not capped at 100. If this traversal needs more than 100 simultaneously open
labels, it returns `RingLabelCapacity`, not an invalid-chemistry diagnosis.
A different traversal may need fewer labels; automatic alternate traversal or
extended ring-number dialects are not implemented.

Planning uses O(V + E) temporary storage, with ordered ID lookups and adjacency
sorting bounded by O((V + E) log(V + E)) time. Emission is iterative and linear
in plan/output size apart from operations on the bounded free-label set.
Both deep branches and long chains use heap-backed stacks, not recursion.

Tests use an independent **test-only** labelled-graph isomorphism search. It
compares every Atom attribute and edge/nonedge order without assuming matching
IDs. The matcher is intentionally not a production substructure/canonicalization
API; exhaustive search is limited to small fixtures. Tests include permuted and
sparse IDs, all 118 bracket elements, explicit/isotope H, branches, dense and
bridged graphs, label reuse/exhaustion, deep iterative traversal, negative
equivalence checks, and all 4^6 absent/single/double/triple edge assignments on
four annotated atoms. Large stress cases use structural checks instead of
exponential isomorphism searches.

## Delivery sequence and acceptance gates

1. **Restricted reader (implemented).** Positioned failures, exact supported
   facts, branch/ring topology, no partial graphs, and integration with the
   existing explicit valence report. Test malformed inputs, numeric overflow,
   UTF-8 positions, unsupported syntax, and deep iterative nesting.
2. **Noncanonical writer and equivalence tests (implemented for this profile).**
   A detached writer input includes all supported semantics, not only
   connectivity. Reject unknown/unrepresentable facts. Test read/write/read graph isomorphism with
   element, isotope, charge, bond and H semantics, including disconnected and
   symmetric graphs. IDs and text ordering are not chemical identity. Do not
   turn explicit H into implicit H without a stated preservation policy.
3. **Broader valence/H/radical models and ring perception.** Add declared
   rule-set coverage and deterministic reports, then supported bond/ring
   queries. Separate cycle membership from a chosen cycle-basis algorithm;
   SMILES ring closure itself does not require SSSR. Preserve uncertainty when
   spin/radical state is not determined by the available facts.
4. **Aromaticity and Kekulization (partially implemented).** Source aromatic
   atom facts and bond labels now round-trip independently. The bounded
   `perception::kekulize` model resolves C/N/O/S and common N charges into a
   detached integer-bond representation; it does not run general aromaticity
   perception, assign a numeric order 1.5, or validate every molecular state.
   See [compatibility matrix](smiles-compatibility.md) for exact coverage.
5. **Stereo and atom classes (partially implemented).** Tetrahedral `@`/`@@`
   and `@TH1`/`@TH2` now use stable graph references, source ring-site order,
   virtual H and parity-correct noncanonical writing. Generic polygon/line
   scenes encode solid/hashed wedges from final coordinates. Paired directional
   bonds now retain relative double-bond references and use shared-edge parity
   constraints when writing. Partial directions, extended stereo classes and
   CIP labeling remain future work. Atom classes/maps
   must be source annotations, never graph-local IDs. Unknown/unspecified stereo
   stays absent rather than being inferred from adjacency or coordinates.
6. **Full molecular conformance and canonicalization.** Publish a versioned
   capability matrix and specification-derived positive/negative fixtures.
   Cross-check independently licensed corpora with explicit provenance; third
   party agreement alone does not define correctness. Resolve remaining
   OpenSMILES constructs (including wildcard atoms via an explicit domain atom
   identity, not a fake element). Canonical labeling is a separate algorithm,
   not sorted IDs, a DFS writer, or a synonym for chemical standardization.
   Test symmetry, atom permutations, disconnected order, and stereo retention.
7. **Other formats on the same facts, not the same grammar.** Add Molfile/SDF
   V2000/V3000 typed records for coordinates, properties, stereo, query features
   and multiple records. Add reaction records with distinct reactant/agent/
   product membership and mapping validation before reaction SMILES/RXN.
   CXSMILES extensions require typed preservation and an explicit dialect.
   SMARTS requires a query/predicate model, not relaxed molecular SMILES rules.

Full molecular SMILES support is a planned compatibility target, not something
this restricted codec claims to have finished. Each stage should expand acceptance
only when data-model retention, writer behavior, diagnostics, and tests are
ready together. Preserve unsupported information by rejection until then.
