#![forbid(unsafe_code)]
//! Core domain types and workspace logic for Chitin.
//!
//! `chitin-utils` is intentionally UI-independent. It owns reusable data models
//! and filesystem-facing workspace operations that can be shared by desktop,
//! command-line, and future agent/runtime crates.

/// Workspace discovery and project file tree models.
pub mod workspace;
