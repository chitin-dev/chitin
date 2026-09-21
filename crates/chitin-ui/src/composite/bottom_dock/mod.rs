//! Resizable bottom workbench overlay dock.
//!
//! The dock is independent from document tabs and split panels. Applications
//! select an active dock item, then supply that item's content to [`BottomDock`].
//! The rendered dock is absolutely positioned over its containing workbench
//! layer, so it does not participate in document-area size allocation.

mod model;
mod render;

pub use model::{
  BottomDockItemId, BottomDockState, DEFAULT_BOTTOM_DOCK_HEIGHT, DEFAULT_BOTTOM_DOCK_MIN_HEIGHT,
  DEFAULT_CENTER_AREA_MIN_HEIGHT,
};
pub use render::{BottomDock, BottomDockResizeConfig};
