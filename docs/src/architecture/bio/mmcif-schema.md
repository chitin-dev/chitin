# PDBx/mmCIF dictionary model

PDBx/mmCIF defines names, categories, loops, and primitive value types through
the official dictionary. Chitin treats that dictionary as a description of the
input language, not as an application data model.

The conceptual boundary is:

```text
dictionary names and primitive values
                  ↓
typed category view
                  ↓
biological interpretation
                  ↓
structure model
```

Dictionary metadata does not decide which identifier namespace to prefer, which
fields are required by a consumer, or how a string maps to a domain enum. Those
are scientific and application policies applied by the projection layer.

## Missing values

The CIF markers `.` and `?` represent different source states and must not be
collapsed into ordinary empty strings. A projection may require a value, keep it
optional, or emit a diagnostic depending on the role of that field.

## Loops and save frames

Loop values are interpreted by column and row. Quoted strings remain values even
when they begin with an underscore. Dictionary save frames are metadata about
items and categories; preserving them at the syntax boundary allows future
dictionary-aware features without changing the biological model.

## Type resolution

When an item declares its primitive type, that type is authoritative. A linked
item may inherit its parent's type:

$$
T(c)=
\begin{cases}
T_{\mathrm{declared}}(c), & \text{if }c\text{ declares a type},\\
T(p(c)), & \text{if }c\text{ links to parent }p(c).
\end{cases}
$$

An unresolved type or cyclic link is a dictionary error, not a reason to guess
that the value is text.

## Reproducibility

Runtime parsing uses the checked-in schema definition. Updating the external
dictionary is a deliberate maintenance operation; it should produce a reviewed
schema diff and focused projection tests. Normal builds should not require a
network request or a local dictionary download.
