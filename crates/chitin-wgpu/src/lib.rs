//! Low-level experimental WGPU bridge helpers for Chitin.
//!
//! This crate owns framework-neutral GPU helpers, viewport math, and reusable
//! renderers. Scientific topology and renderer-neutral scene data remain in
//! `chitin-bio`, while GPUI surface adapters remain in `chitin-desktop`.

pub mod bridge;
pub mod camera;
pub mod molecule;

pub use bridge::{ClearRenderer, DepthTarget, RenderTargetSize};
pub use camera::{DragMode, ViewerCamera, ViewportDrag};
pub use molecule::{
  AtomRepresentation, BallAndStickElementStyle, BallAndStickMaterial, BallAndStickPalette, BallAndStickStyle,
  MoleculeDebugMode, MoleculeRenderer,
};
