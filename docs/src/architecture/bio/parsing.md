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

Cartesian coordinates are stored in ångströms. Fractional coordinates and
unit-cell geometry are preserved as metadata; parsing does not implicitly expand
symmetry mates or convert fractional coordinates. The coordinate-system
contract and unit-cell equations are defined once in
[the structure model contract](./structure.md#units-and-coordinate-systems).

Cell values are still validated at parse time. Zero or straight angles are
rejected immediately because they describe a degenerate cell; a later coordinate
conversion must also reject a numerically singular basis rather than emitting
NaN or infinite coordinates.

## Validation philosophy

Validation happens at the earliest boundary that has enough information to
explain the problem. A malformed numeric value identifies its source field; an
invalid relationship identifies the affected record; a renderer should not be
the first component to discover a broken structure table.
