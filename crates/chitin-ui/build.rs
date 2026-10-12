//! Invalidates the embedded asset bundle when files are added, changed, or removed.
//!
//! RustEmbed tracks files already included in a build. Watching the asset
//! directory also lets Cargo detect new icons before compiling the next bundle.

fn main() {
  // Track directory changes too: embedding existing files alone misses newly added icons.
  println!("cargo:rerun-if-changed=../../assets");
}
