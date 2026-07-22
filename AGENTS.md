# Repository Guidelines

## Project Structure & Module Organization

Chitin is a Rust workspace. Core crates live under `crates/`:

- `crates/chitin-core`: domain and workspace data models; no GPUI dependency.
- `crates/chitin-ui`: reusable GPUI components, themes, and UI benchmarks.
- `crates/chitin-desktop`: desktop application wiring, assets, and workspace
  views.

Static assets are in `assets/`, including app logos and SVG icons. Project
planning lives in `ROADMAP.md` and draft notes may appear in `.temp/`. CI
configuration is in `.github/workflows/`, and issue/PR templates are in
`.github/`.

## Build, Test, and Development Commands

- `cargo check --workspace --locked`: compile-check the full workspace.
- `cargo test`: run unit tests and doctests.
- `cargo fmt --all --check`: verify formatting.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: run strict
  linting.
- `cargo run -p chitin-desktop -- .`: launch the desktop app with the current
  directory as the workspace.
- `cargo bench -p chitin-ui --bench tree_virtualization`: run tree
  virtualization benchmarks.

## Coding Style & Naming Conventions

Use Rust 2024 edition and two-space indentation (`rustfmt.toml`,
`.editorconfig`). Prefer clear module boundaries: reusable UI stays in
`chitin-ui`, desktop-specific assets and event wiring stay in `chitin-desktop`,
and scientific/domain logic stays outside GPUI crates. Use snake_case for
functions/modules, PascalCase for types, and descriptive enum variants.

Avoid `unwrap`, `expect`, `dbg!`, and `todo!`; workspace Clippy denies them.
Prefer builder-style component methods such as `.theme(theme)` and
`.child(element)` over broad getter/setter APIs.

## Testing Guidelines

Place focused unit tests near implementation code in `#[cfg(test)]` modules.
Public APIs should use doctests when examples clarify usage. Name tests by
behavior, for example `children_should_replace_existing_children`. For
performance-sensitive UI data paths, add Criterion benches under the relevant
crate’s `benches/` directory.

## Commit & Pull Request Guidelines

Commit history follows scoped conventional-style subjects, for example
`fix(ui): ...`, `bench(tree): ...`, and `assets(desktop): ...`. Keep commits
focused.

PRs should follow `.github/pull_request_template.md`: include a concrete
summary, related issue or roadmap area, change type, scientific correctness
notes when relevant, user impact, validation commands, screenshots/output if
useful, and risks or follow-ups.

## Architecture Notes

Follow the roadmap dependency direction: UI should not directly own heavy
scientific data, domain crates should not depend on GPUI, and long-running work
should move through async/background or future job-system boundaries.
