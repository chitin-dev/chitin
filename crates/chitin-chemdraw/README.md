# chitin-chemdraw

Frontend-independent 2D molecular layout, depiction, and SVG export. This
crate produces renderer-neutral drawing data; it is not a GPUI widget or an
editor.

The design and algorithms are documented in the developer documentation:

- [Drawing pipeline](../../docs/src/architecture/chemdraw/overview.typ)
- [Layout algorithms](../../docs/src/architecture/chemdraw/layout.typ)

The public pipeline is:

```text
chitin-chem::MolecularGraph
        -> layout::generate
        -> depict::depict
        -> svg::export
```

The source graph is borrowed and never mutated. Atom and bond IDs remain
graph-local handles, while selection, editing, undo, windows, and UI state
belong to consuming applications.

```sh
cargo test -p chitin-chemdraw --locked
cargo clippy -p chitin-chemdraw --all-targets --locked -- -D warnings
cargo fmt --all --check
```
