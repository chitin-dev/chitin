//! Composite controls assembled from reusable primitives.

/// Vertical activity-bar composition.
pub mod activity_bar;

/// Resizable workbench dock for terminal, tasks, output, and similar tools.
pub mod bottom_dock;

/// Independently single-selectable groups composed from select primitives.
pub mod grouped_select;
/// Icon-bearing option data for GPUI Kit selectors.
pub mod select_item;

/// Stable document identifiers and command-level layout data (no UI interactions).
pub mod panel;

/// Window-level stacked transient notifications.
pub mod toast;
