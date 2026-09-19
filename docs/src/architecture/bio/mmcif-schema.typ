#import "/book.typ": book-page
#show: book-page

#let source-root = "https://github.com/chitin-dev/chitin/blob/main/crates/chitin-bio/src"

= 1 PDBx/mmCIF dictionary model
<pdbxmmcif-dictionary-model>

PDBx/mmCIF defines names, categories, loops, and primitive value types through
the official dictionary. Chitin treats that dictionary as a description of the
input language, not as an application data model.

The projection boundary is deliberately staged:

1. dictionary metadata describes names and primitive values;
2. the CIF reader exposes typed category and loop records;
3. the projection layer interprets records as biological concepts;
4. the structure builder validates and stores the normalized model.

The syntax and dictionary implementation is located in
#link(source-root + "/structure/mmcif")[`structure/mmcif`]. Dictionary metadata
does not decide which identifier namespace to prefer, which fields are required
by a consumer, or how a string maps to a domain enum. Those are scientific and
application policies applied by the projection layer.

== 1.1 Missing values
<missing-values>

The CIF markers `.` and `?` represent different source states and must not be
collapsed into ordinary empty strings. A projection may require a value, keep
it optional, or emit a diagnostic depending on the role of that field.

An absent value does not automatically invalidate the entire structure. The
projection must instead decide whether the missing field prevents a stable atom,
coordinate state, relationship, or metadata record from being constructed.

== 1.2 Loops and save frames
<loops-and-save-frames>

Loop values are interpreted by column and row. Quoted strings remain values even
when they begin with an underscore. Dictionary save frames are metadata about
items and categories; preserving them at the syntax boundary allows future
dictionary-aware features without changing the biological model.

The low-level CIF representation is defined in
#link(source-root + "/structure/mmcif/cif.rs")[`structure/mmcif/cif.rs`]. The
category-specific projection code is kept under
#link(source-root + "/structure/mmcif/categories")[`structure/mmcif/categories`].

== 1.3 Type resolution
<type-resolution>

When an item declares its primitive type, that type is authoritative. When an
item links to a parent item, the projection may inherit the parent's type after
resolving the link. Resolution stops with an explicit error if the type cannot
be found or if links form a cycle.

An unresolved type or cyclic link is a dictionary error, not a reason to guess
that the value is text. Type inheritance is an implementation detail of schema
resolution and is intentionally described in prose rather than as a formula
between software objects.

== 1.4 Reproducibility
<reproducibility>

Runtime parsing uses the checked-in schema definition. Updating the external
dictionary is a deliberate maintenance operation; it should produce a reviewed
schema diff and focused projection tests. Normal builds should not require a
network request or a local dictionary download.
