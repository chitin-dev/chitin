//! Low-level experimental WGPU bridge helpers for Chitin.
//!
//! This crate owns platform-aware GPU handles and reusable render-target
//! resources. Molecular representations remain in `chitin-molecule-renderer`,
//! while UI surface adapters remain in their frontend crates.

/// Shared ownership handle for platform GPU resources.
// Native GPUI code may share resources across worker/UI boundaries.
#[cfg(not(target_arch = "wasm32"))]
pub type GpuHandle<T> = std::sync::Arc<T>;

/// Shared ownership for GPU resources on the browser's single-threaded target.
#[cfg(target_arch = "wasm32")]
pub type GpuHandle<T> = std::rc::Rc<T>;

pub mod bridge;

pub use bridge::{ClearRenderer, DepthTarget, RenderTargetSize};
