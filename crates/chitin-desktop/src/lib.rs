#![forbid(unsafe_code)]
//! Library entrypoint for Chitin desktop examples and integration tests.
//!
//! The production binary and examples share these modules so examples can
//! validate the real desktop shell without `#[path]`-based source inclusion.

pub mod app;
pub mod commands;
pub(crate) mod features;
pub mod fonts;
pub mod keybindings;
pub mod services;
pub mod workbench;

// Preserve the public application integration entrypoints.
pub use commands::{portable as portable_command, shell_host as builtin_shell};
pub use services::tasks;

/// GPUI adapter for the experimental WGPU document panel.
pub mod wgpu_panel {
  pub use crate::features::molecule::viewport::*;
}
