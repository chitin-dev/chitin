#import "/book.typ": book-page
#show: book-page

= 1 SMILES compatibility
<smiles-compatibility>

This matrix describes executable capability, not complete OpenSMILES conformance
or parity with another toolkit. Reading preserves facts; valence, aromaticity,
stereo, and drawing each have an independent acceptance boundary.

== 1.1 Capability matrix
<smiles-capability-matrix>

#table(
  columns: (1.8fr, 1.2fr, 2.2fr),
  table.header[*Capability*][*Current behavior*][*Remaining boundary*],
  [Organic subset, branches, fragments], [Read/write], [Broader valence and H models],
  [All 118 bracket elements], [Read/write], [Element-specific depiction and valence rules],
  [Isotope, charge, declared H, explicit H vertices], [Read/write], [Broader charged and open-shell models],
  [Ring closure, reuse, `%00`–`%99`], [Read/write], [General cages and multi-face nonregular embedding],
  [Aromatic `b c n o p s`, `as`, `se`], [Read/write], [Broader integer-bond assignment],
  [Tetrahedral `@`, `@@`, `@TH1`, `@TH2`], [Read/write], [CIP labels and stereogenicity perception],
  [Paired directional double-bond markers], [Partial support], [Complete cyclic constraint solving],
  [Partial directions and extended stereo], [Rejected explicitly], [Typed partial-state support],
  [Atom classes, maps, SMARTS queries], [Rejected explicitly], [Separate annotation/query model],
  [Reaction and CX extensions], [Rejected explicitly], [Separate record and dialect models],
  [Canonical SMILES], [Not provided], [Canonical labeling algorithm],
)

== 1.2 Aromatic input and Kekulé assignment
<aromatic-kekule>

`perception::kekulize` resolves supplied aromatic labels in a detached graph. It
does not infer aromaticity from uppercase input and does not assign a fractional
bond order. Neutral aromatic C/N/O/S and selected charged nitrogen states are
handled by a bounded global matching of required pi slots. Supplied double bonds
consume endpoint slots, while donor heteroatoms may satisfy the model without an
internal double bond.

The returned graph keeps IDs, endpoints, isotopes, charge, radical knowledge,
declared H, and explicit H vertices. It clears aromatic flags and assigns
integer orders only in the clone. The source graph is unchanged. Unsatisfiable
pi assignments, unsupported atom states, and work-budget exhaustion have
different typed errors.

The assignment is iterative and deterministic for a fixed graph and budget.
Its worst-case search is exponential; the budget is an explicit correctness and
resource boundary, not a claim of a polynomial general aromaticity algorithm.

== 1.3 Valence boundary
<valence-boundary>

The `MainGroupClosedShellV1` model covers a restricted set of common neutral
and charged states. It reports an atom as determined only when all relevant
facts are known and the selected target is satisfied. Metals, unsupported
charge states, nonzero radicals, aromatic flags, and unknown bond orders remain
unsupported or indeterminate rather than being guessed.

This model is not full sanitization, a stability proof, a force field, or a
quantum-chemistry preparation step. The separate `SmilesOrganicV1` profile
extends selected neutral N/P/S targets without changing the original contract.

== 1.4 Stereo and drawing boundary
<stereo-drawing-boundary>

The graph can retain tetrahedral references and relative double-bond directions,
but it does not infer CIP priority or stereogenicity. `chitin-chemdraw` consumes
these facts when it can construct a valid 2D depiction. It may reject an
unsupported cyclic constraint or retain a diagnostic; it must not silently
discard a source stereo fact to obtain a visually convenient layout.

For exact executable behavior, inspect
#link("https://github.com/chitin-dev/chitin/tree/main/crates/chitin-chem/tests")[the
crate integration tests] and the source modules linked from the
#link("./formats.typ")[format architecture].
