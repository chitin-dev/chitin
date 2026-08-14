# Chitin developer and CI command shortcuts.
#
# Keep recipes as thin wrappers around Cargo and mdBook commands. CI should use
# the same recipes as local development so that the two paths cannot drift.

default:
  @just --list

fmt:
  cargo fmt --all

fmt-check:
  cargo fmt --all --check

check:
  cargo check --workspace --locked

clippy:
  cargo clippy --workspace --all-targets --locked -- -D warnings

test:
  cargo test --workspace --locked

# The Rust workflow intentionally mirrors .github/workflows/check.yml.
ci: fmt-check check clippy

verify: fmt-check check clippy test

bio-test:
  cargo test -p chitin-bio --locked

bio-online:
  cargo test -p chitin-bio --test rcsb_online downloads_configured_structures_and_parses_them -- --ignored --nocapture

desktop path=".":
  cargo run -p chitin-desktop -- "{{path}}"

showcase:
  cargo run -p chitin-ui --example primitive-showcase

cli *args:
  cargo run -p chitin -- {{args}}

docs-build:
  mdbook build docs

docs-serve:
  mdbook serve docs

build-desktop-release:
  cargo build -p chitin-desktop --release --locked

build-cli-release:
  cargo build -p chitin --release --locked

build-release: build-desktop-release build-cli-release
