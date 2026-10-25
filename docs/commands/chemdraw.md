# Chemical drawing commands

The CLI and built-in shell share these commands. Omit the initial `chitin`
inside the built-in shell. Relative paths use the command's working directory.

```sh
chitin chemdraw smiles 'CC(=O)Oc1ccccc1C(=O)O' -o aspirin.svg
chitin chemdraw smiles 'COc1ccccc1' -o molecule.svg --aromatic-style circle
chitin chemdraw smiles 'CCO' -o molecule.svg --stroke-width 1.2 --force
chitin chemdraw diagram crates/chitin-command/examples/reaction.yaml -o reaction.svg
```

Both workflows accept `--bond-length`, `--stroke-width`, `--font-file`,
`--monochrome`, `--aromatic-style kekule|circle`, and
`--explicit-chiral-hydrogens`. Font files produce portable glyph outlines.
The output must be an SVG file in an existing directory. Existing files are
protected unless `--force` is supplied; input and font files remain protected.
Outputs are published atomically after rendering succeeds.

Use `-` as the SMILES or diagram input to consume standard input. Shell hosts
must supply stdin through their execution context; this is not shell piping.
Diagram input accepts JSON or YAML, is limited to 4 MiB, and requires `version: 1`.

## Diagram format

See [the complete example](../../crates/chitin-command/examples/reaction.yaml).
Each molecule has a unique `id`, a `smiles` string, an `at: [x, y]` placement,
optional positive `scale`, and optional complete `positions` in SMILES atom order.
Placements use SVG units with y down; authored atom positions use normalized
bond units with y up. Atom indices are zero-based and scoped to their molecule.

Annotations use `type: highlight|electron-arrow|reaction-arrow|lone-pair`.
An atom reference is `{molecule: id, target: {type: atom, index: 0}}`; a bond
reference uses `target: {type: bond, atoms: [0, 1]}`. Optional reference offsets
are local SVG units and follow molecule scaling. Colors are `[r, g, b, alpha]`
bytes. Electron arrows accept `head: pair|fishhook-left|fishhook-right`; reaction
arrows accept `equilibrium: true`. Arrow `style` accepts `color`, `width`,
`head-length`, `head-width`, and `clearance`. Lone-pair angles use radians.

Mechanisms are authored, not inferred. Arrow routing is not automatic. Layout
warnings are reported without treating every quality diagnostic as a failure.
