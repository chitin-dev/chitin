#import "/book.typ": book-page
#show: book-page

#let source-root = "https://github.com/chitin-dev/chitin/blob/main/crates/chitin-bio/src"

= 1 PDB and PDBx/mmCIF parsing
<pdb-and-pdbxmmcif-parsing>

Both supported formats follow the same conceptual pipeline, although their
syntax is different:

1. source bytes are read from a file, stream, or memory buffer;
2. the format reader interprets records, fields, loops, and optional values;
3. semantic records are projected into common structure concepts;
4. identity, coordinate, and relationship invariants are validated;
5. the caller receives a structure snapshot and recoverable diagnostics.

The format-specific entry points are
#link(source-root + "/structure/pdb.rs")[`PdbParser`] and
#link(source-root + "/structure/mmcif.rs")[`MmcifParser`]. Both return the
format-neutral #link(source-root + "/structure/error.rs")[`StructureParseResult`]
on success.

PDB is a fixed-column format. PDBx/mmCIF is a tokenized data model with data
blocks, categories, loops, optional values, and dictionary-defined fields. The
format differences end at the reader boundary; consumers receive the same
structure concepts from either format.

== 1.1 Required and optional information
<required-and-optional-information>

Fields needed to construct a usable atom or coordinate state are required. An
optional category may be absent without making the complete structure invalid.
Unknown records and recoverable omissions are reported as diagnostics rather
than silently disappearing.

The following distinctions are preserved when relevant:

- missing value versus unknown value;
- author identifier versus label identifier;
- explicit source bond versus inferred bond;
- alternate location and model number;
- Cartesian coordinates versus crystallographic metadata.

The shared builder is responsible for turning parsed records into indexed model
tables. Its source implementation is
#link(source-root + "/structure/builder.rs")[`structure/builder.rs`]. Keeping
this projection shared prevents PDB and mmCIF from developing different
identity or validation rules.

== 1.2 Coordinates and unit cells
<coordinates-and-unit-cells>

Cartesian coordinates are stored in ångströms. Fractional coordinates and
unit-cell geometry are preserved as metadata; parsing does not implicitly
expand symmetry mates or convert fractional coordinates. The coordinate-system
contract and unit-cell equations are defined in
#link("./structure.typ#units-and-coordinate-systems")[the structure model
contract].

Cell values are validated at parse time. Zero or straight angles are rejected
because they describe a degenerate cell. A later coordinate conversion must also
reject a numerically singular basis rather than emitting NaN or infinite
coordinates.

== 1.3 Models and alternate locations
<models-and-alternate-locations>

PDB `MODEL` records and mmCIF model numbers define coordinate-bearing models.
The parser preserves their source numbers while assigning dense internal model
identifiers. A model points to a coordinate set indexed by stable atom identity.

Alternate locations are a different source concept. They describe alternative
local observations for an atom site and do not automatically create another
complete model. Consumers may choose a policy for displaying them, but that
policy must not silently change the source topology.

== 1.4 Validation philosophy
<validation-philosophy>

Validation happens at the earliest boundary that has enough information to
explain the problem. A malformed numeric value identifies its source field; an
invalid relationship identifies the affected record; a renderer should not be
the first component to discover a broken structure table.

Fatal errors prevent construction of a valid structure snapshot. Warnings and
informational diagnostics describe recoverable omissions or format details while
allowing a caller to decide whether to continue. The CLI can display all
diagnostics, while an interactive viewer can show a warning and continue when
the validated structure remains usable.
