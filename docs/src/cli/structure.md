# Inspecting and validating structures

The `chitin structure` commands read local PDB and PDBx/mmCIF files through
the same public readers used by the rest of the project.

## Inspect

```bash
chitin structure inspect ./4hhb.cif
chitin structure inspect ./4hhb.pdb
```

The format is inferred from these extensions:

- `.pdb` and `.ent` use the PDB reader;
- `.cif` and `.mmcif` use the mmCIF reader.

Use `--format` for an extensionless file or standard input:

```bash
cat ./4hhb.cif | chitin structure inspect - --format mmcif
chitin structure inspect ./download --format pdb
```

Text output is intended for humans and uses terminal colors when supported by
the terminal. It includes model, chain, residue, atom, bond, polymer,
secondary-structure, and assembly counts.

Use `--verbose` to include metadata, chain identifiers, and diagnostics:

```bash
chitin structure inspect ./4hhb.cif --verbose
```

For scripts, use JSON. JSON output never contains ANSI color sequences:

```bash
chitin structure inspect ./4hhb.cif --output json
```

## Validate

`validate` parses the file and checks the shared structure-table invariants:

```bash
chitin structure validate ./4hhb.cif
```

Validation checks include:

- model-to-coordinate and model-to-chain references;
- chain-to-residue references;
- residue-to-atom references;
- bond endpoints;
- secondary-structure endpoints and chain consistency;
- coordinate vector lengths;
- polymer sequence ordering.

The command exits successfully only when parsing and validation both succeed.
Use JSON when the result is consumed by another program:

```bash
chitin structure validate ./4hhb.cif --output json
```
