#import "/book.typ": book-page
#show: book-page

#let source-root = "https://github.com/chitin-dev/chitin/blob/main/crates/chitin-chem/src"

= 1 Chemical formats
<chemical-formats>

The format layer converts serialized records into explicit molecular facts and
provenance. It does not perform hidden sanitization. The current target for
molecular SMILES is a documented subset of OpenSMILES; SMARTS, reaction SMILES,
CXSMILES, and record files require separate data models.

== 1.1 Data flow
<format-data-flow>

```text
positioned format syntax
        -> detached facts and source provenance
        -> explicit perception policy
        -> detached analysis report

domain graph and supported annotations
        -> format-specific writer
```

`io::smiles::syntax` scans positioned tokens and `parser` assembles topology.
The writer separates attribute encoding, traversal planning, and text emission.
The implementation is GPUI-independent and can be used by desktop, browser,
batch, and future Typst/WASM adapters.

== 1.2 Reader contract
<reader-contract>

`io::smiles::read_smiles` supports branches, nested branches, fragments, ring
labels from `0` through `%99`, explicit single/double/triple bonds, aromatic
symbols, bracket elements, isotope mass numbers, charge, declared H, and
positioned tetrahedral and paired directional stereo facts where the graph can
retain them.

Errors carry UTF-8 byte ranges and typed categories. There is no partial graph
result. Syntax failure, unsupported chemistry, numeric representation failure,
and graph-topology failure remain distinct. Empty input represents an empty
graph; comments, names, and extension records are not silently consumed.

The scanner is iterative and uses heap-backed branch and ring state. For input
length $n$, ordered graph maps give worst-case assembly time $O(n log n)$ and
auxiliary storage $O(n)$. Resource limits for untrusted import belong at the
caller boundary, where byte, atom, nesting, and work budgets can be selected.

== 1.3 Writer contract
<writer-contract>

`io::smiles::write_smiles` returns a complete owned string or a typed error. It
borrows the graph and performs no perception, repair, normalization, or source
text reuse. Its guarantee is exact supplied attributes and labelled undirected
connectivity up to graph isomorphism, not equal IDs, original spelling,
canonical ordering, or chemical equivalence after normalization.

The writer uses an iterative DFS spanning forest and temporary dense indices.
Tree edges become branches or continuations; non-tree edges become ring sites.
These are serialization decisions, not ring perception. Labels are reused after
closure and more than one hundred simultaneously open labels is reported as a
capacity error rather than emitted as invalid syntax.

Unknown charge, hydrogen policy, radical knowledge, and bond orders are errors
when the current reader could not recover them. Aromatic bonds remain `:` and
aromatic atom designations remain lowercase; they are not converted to numeric
order 1.5 by the writer.

== 1.4 Planned expansion
<format-roadmap>

The staged boundary is:

1. restricted positioned SMILES reader;
2. deterministic noncanonical writer and graph-isomorphism tests;
3. broader valence, hydrogen, radical, and ring reports;
4. aromaticity and bounded Kekulé assignment;
5. additional stereo and atom-class records;
6. full molecular conformance and canonical labeling;
7. Molfile/SDF, reaction records, CXSMILES, and SMARTS with typed models.

Canonicalization is separate from writing, and a SMILES parser is not a general
format registry. New formats should preserve their own coordinates, properties,
conformers, record membership, and provenance instead of flattening them into a
graph-only return value.

== 1.5 Validation example
<format-validation-example>

```rust
use chitin_chem::io::smiles::{read_smiles, write_smiles};
use chitin_chem::perception::{analyze_valence, ValenceModel};

let parsed = read_smiles("CC(O)C")?;
let report = analyze_valence(parsed.graph(), ValenceModel::MainGroupClosedShellV1)?;
assert!(report.atoms().all(|(_, row)| row.value().is_some()));
let serialized = write_smiles(parsed.graph())?;
let reread = read_smiles(&serialized)?;
assert_eq!(reread.graph().atom_count(), 4);
# Ok::<(), Box<dyn std::error::Error>>(())
```
