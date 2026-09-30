//! Main document area for opened workspace files.
//!
//! This module owns desktop-specific document presentation, including the
//! placeholder for generic files and the molecular viewport for structures.

mod commands;
pub(crate) mod dock;
pub mod layout;
mod render;
pub(crate) mod state;

#[cfg(test)]
mod tests;

pub use render::render_document_area;
pub(crate) use state::DocumentPanelState;
pub use state::OpenedProjectDocument;
