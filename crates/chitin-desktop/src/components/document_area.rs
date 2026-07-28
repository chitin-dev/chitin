//! Main document area for opened workspace files.
//!
//! This module owns desktop-specific document presentation. It intentionally
//! starts with a placeholder view so file-tree activation can be wired before a
//! real editor, molecule viewer, or structure viewer is introduced.

mod commands;
mod render;
mod state;

#[cfg(test)]
mod tests;

pub use render::render_document_area;
pub(crate) use state::DocumentPanelState;
pub use state::OpenedProjectDocument;
