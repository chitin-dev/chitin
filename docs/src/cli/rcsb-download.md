# Downloading RCSB structures

The `chitin` command-line tool can download structures from the RCSB Protein
Data Bank in PDB or PDBx/mmCIF format.

## Basic usage

```bash
chitin db rcsb download --id 4hhb --format pdb
```

The PDB identifier must contain exactly four ASCII letters or digits.
Identifiers are normalized to uppercase, so `4hhb` and `4HHB` refer to the same
entry.

Multiple identifiers can be separated by commas:

```bash
chitin db rcsb download --id 4hhb,1yth,6vxx --format pdb
```

Downloads are processed sequentially. Each completed file is kept if a later
identifier fails.

`--format` accepts:

- `pdb` for the legacy PDB text format;
- `mmcif` for the PDBx/mmCIF format.

The default format is `pdb`:

```bash
chitin db rcsb download --id 4hhb
```

`databases` is an alias for `db`:

```bash
chitin databases rcsb download --id 4hhb --format mmcif
```

## Output location

Without `--output`, files are saved under the user home directory:

```text
~/.chitin/download/pdb/4HHB.pdb
~/.chitin/download/mmcif/4HHB.cif
```

The directory is created automatically when it does not exist.

### Explicit file path

When `--output` names a file, the file is written exactly at that path:

```bash
chitin db rcsb download \
  --id 4hhb \
  --format mmcif \
  --output ./structures/hemoglobin.cif
```

The parent directory is created automatically.

### Existing directory

When `--output` points to an existing directory, Chitin generates the filename
from the PDB identifier and format:

```bash
chitin db rcsb download --id 4hhb --output ./structures
```

This writes:

```text
./structures/4HHB.pdb
```

For multiple identifiers, `--output` must be an existing directory or a
directory-style path that does not already name a file:

```bash
chitin db rcsb download \
  --id 4hhb,1yth \
  --output ./structures
```

This writes `./structures/4HHB.pdb` and `./structures/1YTH.pdb`. A single
explicit file path cannot be used for multiple identifiers because it would
cause the downloads to overwrite one another.

An existing directory is detected from the filesystem. A path that does not
exist is treated as an explicit file path, even when it has no extension:

```bash
chitin db rcsb download --id 4hhb --output ./structures/hemoglobin
```

This writes to `./structures/hemoglobin`.

## Progress display

The CLI starts with an indeterminate activity indicator while the response size
is unknown. Once RCSB provides a total response size, the indicator changes to a
byte-based progress bar. This avoids displaying a misleading percentage before
the total size is known.

After a successful download, the CLI prints the canonical identifier, format,
and final path:

```text
✓ Downloaded 4HHB (PDB)
✓ Saved to /home/user/.chitin/download/pdb/4HHB.pdb
```

## Errors

Invalid identifiers fail before a network request:

```text
error: invalid PDB ID: PDB ID must be 4 characters, got 3
```

If RCSB cannot find an entry or the network request fails, the command exits
with a non-zero status and reports the provider error. Existing output files may
be replaced when the download succeeds.
