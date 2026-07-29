#![forbid(unsafe_code)]
//! Library entrypoint for Chitin desktop examples and integration tests.
//!
//! The production binary and examples share these modules so examples can
//! validate the real desktop shell without `#[path]`-based source inclusion.

pub mod app;
pub mod commands;
pub(crate) mod components;

/// GPUI adapter for the experimental WGPU document panel.
pub mod wgpu_panel {
  pub use crate::components::wgpu_panel::*;
}
