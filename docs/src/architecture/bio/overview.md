# Biological structure layer

`chitin-bio` is the format-independent domain layer for molecular structures. It
turns PDB and PDBx/mmCIF input into one validated structure model that can be
used by command-line tools, renderers, and future analysis packages.

The layer preserves source information, normalizes records into stable atoms,
residues, chains, models, and bonds, and reports invalid input explicitly. It
does not own windows, GPU resources, network downloads, or visual style.

## Conceptual flow

```text
PDB / PDBx-mmCIF
       ↓
format reader
       ↓
validated structure + diagnostics
       ↓
scene extraction / analysis / rendering
```

PDB and mmCIF are different serializations, not different domain models. A
caller can switch formats without changing how it addresses an atom or model.

## Stable identity and changing coordinates

Atom identity and topology are shared by coordinate states. A coordinate state
changes positions and related per-state values, but does not silently create a
second atom table:

$$
  \mathrm{structure} = \mathrm{topology} +
  \{\mathrm{coordinate\ state}_k\}_{k=1}^{K}
$$

The renderer may derive GPU data from one selected state, but the source model
remains authoritative for identifiers, metadata, and provenance.

## Information policy

The parser preserves author and label identifiers, alternate locations,
occupancy, temperature factors, element information, unit-cell metadata,
symmetry metadata, and bond provenance whenever the format provides them.
Missing, unknown, and empty values remain distinguishable where that distinction
affects interpretation.

Heuristics such as distance-based bond inference are derived data. They are
never presented as if they were explicitly recorded by the source file.

See [the structure model](./data-model.md),
[the parsing pipeline](./parsing.md), and [bond inference](./bond-inference.md)
for the main domain rules.
