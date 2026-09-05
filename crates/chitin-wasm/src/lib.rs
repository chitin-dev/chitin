#![forbid(unsafe_code)]
//! Browser bridge for Chitin's biological parsers and WGPU molecule renderer.

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
pub use web::{MoleculeViewer, create_viewer};
