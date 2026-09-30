//! Resizable bottom workbench overlay dock.
//!
//! The dock is independent from document tabs and split panels. Applications
//! select an active dock item, then supply that item's content to [`BottomDock`].
//! Kit's vertical resizable group lives in an absolute overlay with an empty,
//! transparent upper pane and the dock in its lower pane. The document remains
//! outside that group, so dock resizing never changes document-area allocation.
//! Kit owns pointer tracking and drag state; Chitin only remembers dock height.

mod model;
mod render;

pub use model::{
  BottomDockItemId, BottomDockState, DEFAULT_BOTTOM_DOCK_HEIGHT, DEFAULT_BOTTOM_DOCK_MIN_HEIGHT,
  DEFAULT_CENTER_AREA_MIN_HEIGHT,
};
pub use render::BottomDock;
