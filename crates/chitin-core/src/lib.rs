#![forbid(unsafe_code)]
//! Core domain types and workspace logic for Chitin.
//!
//! `chitin-core` is intentionally UI-independent. It owns reusable data models
//! and filesystem-facing workspace operations that can be shared by desktop,
//! command-line, and future agent/runtime crates.

/// Workspace discovery and project file tree models.
pub mod workspace;

/// Small product summary used by early application surfaces.
///
/// This is deliberately lightweight until Chitin has a richer project metadata
/// model. UI crates can use it for placeholder titles and status text without
/// depending on desktop-specific state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSummary {
  /// Product name shown in top-level UI.
  pub product_name: &'static str,
  /// Short product focus line shown in placeholder surfaces.
  pub focus: &'static str,
}

impl Default for WorkspaceSummary {
  fn default() -> Self {
    Self {
      product_name: "Chitin",
      focus: "Computational chemistry and bioinformatics",
    }
  }
}
