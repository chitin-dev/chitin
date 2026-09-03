//! Low-level experimental WGPU bridge helpers for Chitin.
//!
//! This crate owns framework-neutral GPU helpers, viewport math, and reusable
//! renderers. Scientific topology and renderer-neutral scene data remain in
//! `chitin-bio`, while GPUI surface adapters remain in `chitin-desktop`.

/// Shared ownership handle for platform GPU resources.
// Native GPUI code may share resources across worker/UI boundaries.
#[cfg(not(target_arch = "wasm32"))]
pub type GpuHandle<T> = std::sync::Arc<T>;

/// Shared ownership for GPU resources on the browser's single-threaded target.
#[cfg(target_arch = "wasm32")]
pub type GpuHandle<T> = std::rc::Rc<T>;

pub mod bridge;
pub mod camera;
pub mod molecule;

pub use bridge::{ClearRenderer, DepthTarget, RenderTargetSize};
pub use camera::{DragMode, ViewerCamera, ViewportDrag};
pub use molecule::{
  AtomRepresentation, BallAndStickElementStyle, BallAndStickMaterial, BallAndStickPalette, BallAndStickStyle,
  MoleculeDebugMode, MoleculeRenderer,
};
