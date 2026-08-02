//! Reusable GPUI components and visual themes for Chitin.
//!
//! `chitin-ui` is intended to stay application- and domain-neutral. It provides
//! composable controls, layout primitives, and theme data that can be reused by
//! Chitin desktop and published independently.

/// Reusable GPUI component primitives.
pub mod components;
/// Composite controls assembled from reusable primitives.
pub mod composite;
/// Low-level, application-neutral controls.
pub mod primitive;
/// Theme structures and built-in palettes.
pub mod themes;
