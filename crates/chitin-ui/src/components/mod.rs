//! Reusable UI components built on GPUI.
//!
//! Components in this module are domain-neutral building blocks. Application
//! crates provide assets, command wiring, and domain-specific state.

/// Vertical workbench activity bar components.
pub mod activity_bar;
/// Sidebar shell and subsection components.
pub mod sidebar;
/// Generic hierarchical tree components.
pub mod tree;
/// Custom window title bar components.
pub mod window_bar;
