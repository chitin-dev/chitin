//! Composite controls assembled from reusable primitives.

/// Vertical activity-bar composition.
pub mod activity_bar;

/// Resizable workbench dock for terminal, tasks, output, and similar tools.
pub mod bottom_dock;

/// Structured command terminal assembled from terminal and input primitives.
pub mod command_terminal;

/// Independently single-selectable groups composed from select primitives.
pub mod grouped_select;

/// IDE-style multi-panel container composition.
pub mod panel;

/// Searchable quick-pick overlay composition.
pub mod quickpick;

/// Window-level stacked transient notifications.
pub mod toast;

/// Desktop window-title bar composition.
pub mod window_bar;
