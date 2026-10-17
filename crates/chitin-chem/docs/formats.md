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
only one reader. Extract shared abstractions when two concrete implementations
demonstrate the same contract, not by forcing different formats into a
SMILES-shaped interface. A dedicated I/O crate can follow if dependencies or
build cost justify it; the module boundary already isolates that work.

## Current reader

`io::smiles::read_smiles(&str)` supports:

- Bare `B C N O P S F Cl Br I`, neutral with zero declared attached H,
  an explicit closed-shell reading, and permission for implicit-H inference.
- Bracketed named elements (all 118), isotope mass numbers, declared H, signed
  numeric charges, and the legacy `++`/`--` charge spelling.
- Implicit/explicit single, double and triple bonds; nested branches; dot
  disconnections; ring labels `0` through `9` and `%00` through `%99`.
- Ring-label reuse after closure, compatible orders on either closure site,
  and closure across dots. Ring labels describe edges, not ring perception.
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

## Delivery sequence and acceptance gates

1. **Restricted reader (this change).** Positioned failures, exact supported
   facts, branch/ring topology, no partial graphs, and integration with the
   existing explicit valence report. Test malformed inputs, numeric overflow,
   UTF-8 positions, unsupported syntax, and deep iterative nesting.
2. **Noncanonical writer and equivalence tests.** Design a detached writer
   input that includes all supported semantics, not only connectivity. Reject
   unknown/unrepresentable facts. Test read/write/read graph isomorphism with
   element, isotope, charge, bond and H semantics, including disconnected and
   symmetric graphs. IDs and text ordering are not chemical identity. Do not
   turn explicit H into implicit H without a stated preservation policy.
3. **Broader valence/H/radical models and ring perception.** Add declared
   rule-set coverage and deterministic reports, then supported bond/ring
   queries. Separate cycle membership from a chosen cycle-basis algorithm;
   SMILES ring closure itself does not require SSSR. Preserve uncertainty when
   spin/radical state is not determined by the available facts.
4. **Aromaticity and Kekulization.** Introduce source aromatic atom facts in
   the domain before accepting lowercase SMILES. Aromatic atom flags and
   aromatic bond labels are distinct; do not encode aromaticity solely by
   `BondOrder::Aromatic` or assume order 1.5. Support `[nH]`, aromatic/biphenyl
   connectivity, fused rings, explicit single bonds and dearomatized forms with
   a named aromaticity model and failure diagnostics.
5. **Stereo and atom classes.** Domain descriptors must use stable IDs and
   explicit reference-neighbor ordering. Parse tetrahedral `@`/`@@`, directional
   bonds and the remaining OpenSMILES stereo classes only after they can be
   retained and written correctly. Capture traversal neighbor order, including
   virtual H and ring openings, at parse time; adjacency-map order is NOT stereo
   parity. Atom classes/maps must be source annotations, never graph-local IDs.
   Preserve unknown/unspecified stereo without inventing stereocenters.
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
this first reader claims to have finished. Each stage should expand acceptance
only when data-model retention, writer behavior, diagnostics, and tests are
ready together. Preserve unsupported information by rejection until then.
