//! Low-level experimental WGPU bridge helpers for Chitin.
//!
//! This crate intentionally owns only framework-neutral GPU helpers and
//! viewport math. Scene-specific renderers, UI adapters, GPUI surface handles,
//! and future molecule-specific data models belong in their respective desktop
//! or domain crates.

pub mod bridge;
pub mod camera;

pub use bridge::{ClearRenderer, DepthTarget, RenderTargetSize};
pub use camera::{DragMode, ViewerCamera, ViewportDrag};
