# Chitin

Chitin is an agent-native computational chemistry and bioinformatics integrated development suite.

## Project Layout

- `crates/chitin-core`: domain model and non-UI application logic.
- `crates/chitin-desktop`: GPUI desktop shell.
- `assets`: shared visual assets.

## Development

On Arch Linux, install the Rust toolchain and common native build dependencies if needed:

```sh
sudo pacman -S rust base-devel pkgconf
```

Run the desktop app:

```sh
cargo run -p chitin-desktop
```

Check the workspace:

```sh
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
