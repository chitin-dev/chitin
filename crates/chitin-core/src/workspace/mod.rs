//! Workspace loading, errors, and project tree models.
//!
//! This module exposes the public workspace API used by desktop and CLI
//! frontends. It keeps filesystem traversal and error reporting in `chitin-core`
//! so UI crates can remain rendering-focused.

mod error;
mod tree;

pub use error::ProjectWorkspaceError;
pub use tree::{ProjectTree, ProjectTreeEntry, ProjectTreeEntryKind, ProjectWorkspace};
