//! Molecular representations and framework-neutral viewport interaction.
//!
//! This crate converts renderer-neutral [`chitin_bio::structure::StructureScene`]
//! data into WGPU resources. Surface ownership and reusable render targets
//! remain in `chitin-wgpu`.
//!
//! The public [`MoleculeRenderer`] API is shared by the desktop GPUI adapter
//! and the browser WASM bridge. Callers provide a
//! [`chitin_bio::structure::StructureScene`], select a
//! [`AtomRepresentation`], and retain ownership of the target surface.

pub mod camera;
mod cartoon;
pub mod molecule;

pub use camera::{DragMode, ViewerCamera, ViewportDrag};
pub use molecule::{
  AtomRepresentation, BallAndStickElementStyle, BallAndStickMaterial, BallAndStickPalette, BallAndStickStyle,
  MoleculeDebugMode, MoleculeRenderer,
};
