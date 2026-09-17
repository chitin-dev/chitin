#![forbid(unsafe_code)]
//! Stand-alone visualizer for analytical MSMS construction stages.
//!
//! Run with `cargo run --example msms-debug -- structure.cif` and use the
//! left/right controls to inspect reduced-surface topology, analytical patch
//! families, singularity handling, and the final display tessellation.

#[path = "msms-debug/mod.rs"]
mod msms_debug;

fn main() {
  msms_debug::run();
}
