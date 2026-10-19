# chitin-chem

Renderer-independent chemical identities, topology, and detached perception
reports. The crate is the foundation for `chitin-chemdraw` and future
biological adapters; it is not a UI, editor, or complete cheminformatics
toolkit.

The architecture and current format boundary are documented in the developer
documentation:

- [Chemical graph model](../../docs/src/architecture/chem/overview.typ)
- [Chemical formats](../../docs/src/architecture/chem/formats.typ)
- [SMILES compatibility](../../docs/src/architecture/chem/smiles-compatibility.typ)

The crate provides stable graph-local atom and bond IDs, element and isotope
facts, bond labels, explicit stereo annotations, connected components, ring
topology, a restricted positioned SMILES reader and deterministic noncanonical
writer, plus detached valence and Kekulé reports.

It deliberately does not infer missing chemistry while parsing. Unknown facts,
unsupported states, and invalid results of a selected perception model remain
distinct diagnostics.

```sh
cargo test -p chitin-chem --locked
cargo clippy -p chitin-chem --all-targets --locked -- -D warnings
cargo fmt --all --check
```
