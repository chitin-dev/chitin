# Chitin CLI

Command-line tools for Chitin structural biology workflows.

## Install

```bash
cargo install chitin
```

## Download an RCSB structure

```bash
chitin db rcsb download --id 4hhb --format pdb
chitin db rcsb download --id 4hhb --format mmcif
```

Without `--output`, files are saved under `~/.chitin/download` using the
selected format directory. An existing output directory receives a generated
filename; an explicit file path is used as provided.

## Inspect and validate a structure

Inspect a local PDB or mmCIF file. The format is inferred from `.pdb`, `.ent`,
`.cif`, or `.mmcif`; use `--format` for stdin or extensionless files:

```bash
chitin structure inspect ./4hhb.cif
chitin structure inspect ./4hhb.pdb --verbose
cat ./4hhb.cif | chitin structure inspect - --format mmcif
```

For scripts, request stable JSON output:

```bash
chitin structure inspect ./4hhb.cif --output json
```

Validate parser output and indexed structure invariants:

```bash
chitin structure validate ./4hhb.cif
chitin structure validate ./4hhb.cif --output json
```

## Shell completion

```bash
chitin completions bash
chitin completions zsh
chitin completions fish
chitin completions powershell
```

See the
[Chitin CLI documentation](https://github.com/chitin-dev/chitin/blob/main/docs/src/cli/rcsb-download.md)
for output-path and shell-installation details.
