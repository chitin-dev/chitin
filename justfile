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

databases-test:
  cargo test -p chitin-databases --features test-support --locked

# The Rust workflow intentionally mirrors .github/workflows/check.yml.
ci: fmt-check check clippy test databases-test

verify: fmt-check check clippy test databases-test

bio-test:
  cargo test -p chitin-bio --locked

bio-online:
  cargo test -p chitin-bio --test rcsb_online downloads_configured_structures_and_parses_them -- --ignored --nocapture

# Parse flat PDB/mmCIF fixtures without network access.
bio-local path="crates/chitin-bio/tests/fixtures/rcsb":
  CHITIN_BIO_FIXTURE_ROOT="{{path}}" cargo test -p chitin-bio --test rcsb_local -- --nocapture

# Requires the ignored local PDBx dictionary beside the generated schema.
generate-mmcif-schema:
  cargo run -p chitin-mmcif-schema --locked
  rustfmt --edition 2024 crates/chitin-bio/src/structure/mmcif/schema.rs

desktop path=".":
  cargo run -p chitin-desktop -- "{{path}}"

wgpu-example path="." *args:
  cargo run --example chitin-wgpu-desktop -- "{{path}}" {{args}}

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
