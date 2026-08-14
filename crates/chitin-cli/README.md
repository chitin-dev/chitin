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
