//! Shared Chitin presentation components built on GPUI Kit.
//!
//! Kit owns general-purpose controls and theme colors. This crate contains
//! shared workbench surfaces, specialized views, widgets, and bundled assets.

/// Bundled Chitin assets and GPUI Kit asset fallback.
pub mod assets;
/// Shared initialization and theme policy.
pub mod theme;
/// Specialized stateful presentation views.
pub mod views;
/// Reusable widgets built on GPUI Kit controls.
pub mod widgets;
/// Workbench surfaces, docking chrome, and activity-bar composition.
pub mod workbench;

pub use theme::init;
