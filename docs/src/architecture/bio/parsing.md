# PDB and PDBx/mmCIF parsing

Both supported formats follow the same conceptual pipeline:

```text
source bytes
    ↓
format interpretation
    ↓
semantic records
    ↓
validated structure
    + diagnostics
```

PDB is a fixed-column format. PDBx/mmCIF is a tokenized data model with data
blocks, categories, loops, optional values, and dictionary-defined fields. The
format differences end at the reader boundary; consumers receive the same
structure concepts from either format.

## Required and optional information

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

## Coordinates and unit cells

Cartesian coordinates are stored in ångströms. Fractional coordinates are
described by a unit-cell basis and are not implicitly expanded during parsing.
For fractional coordinates (\mathbf{f}), conversion is:

$$
\mathbf{x} = A\mathbf{f}.
$$

For a conventional triclinic cell:

$$
A =
\begin{bmatrix}
a & b\cos\gamma & c\cos\beta \\
0 & b\sin\gamma &
c\dfrac{\cos\alpha-\cos\beta\cos\gamma}{\sin\gamma} \\
0 & 0 & c\sqrt{1-u^2-v^2}
\end{bmatrix},
$$

where

$$
u=\cos\beta,
\qquad
v=\frac{\cos\alpha-\cos\beta\cos\gamma}{\sin\gamma}.
$$

Angles equal to (0^\circ) or (180^\circ) are rejected because they make the
cell degenerate. A conversion pass must also reject a numerically singular
basis rather than emitting NaN or infinite coordinates.

## Validation philosophy

Validation happens at the earliest boundary that has enough information to
explain the problem. A malformed numeric value identifies its source field; an
invalid relationship identifies the affected record; a renderer should not be
the first component to discover a broken structure table.
