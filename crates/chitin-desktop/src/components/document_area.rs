//! Main document area for opened workspace files.
//!
//! This module owns desktop-specific document presentation, including the
//! placeholder for generic files and the molecular viewport for structures.

mod commands;
mod render;
pub(crate) mod state;
pub(crate) mod structure;

#[cfg(test)]
mod tests;

pub(crate) use render::DocumentOptionsControls;
pub use render::render_document_area;
pub(crate) use state::DocumentPanelState;
pub use state::OpenedProjectDocument;
