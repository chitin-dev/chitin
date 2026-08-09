//! Composite multi-panel container.
//!
//! IDE docking layouts are commonly represented as a binary tree: internal
//! nodes are splits, and leaf nodes are tab stacks. This module provides that
//! model, interaction state, and a lightweight GPUI renderer for panel chrome.
//! Application crates keep concrete view rendering and command wiring outside
//! `chitin-ui`.

mod drag;
mod layout;
mod model;
mod render;
mod scroll;

#[cfg(test)]
mod tests;

pub use drag::{
  PanelTabDrag, PanelTabDragConfig, PanelTabDragStartHandler, PanelTabDragState, PanelTabDragTargetHandler,
  PanelTabDropHandler, PanelTabDropTarget, panel_tab_insertion_index,
};
pub use layout::{MAX_PANEL_SPLIT_RATIO, MIN_PANEL_SPLIT_RATIO, PanelResizeConfig, PanelResizeStartHandler};
pub use model::{
  PanelId, PanelLeaf, PanelNode, PanelSplit, PanelSplitAxis, PanelSplitBranch, PanelSplitPath, PanelSplitPlacement,
  PanelTab, PanelTabId, PanelTree,
};
pub use render::{
  DEFAULT_PANEL_SPLIT_HANDLE_SIZE, DEFAULT_PANEL_TAB_CLOSE_BUTTON_SIZE, DEFAULT_PANEL_TAB_STRIP_HEIGHT,
  DEFAULT_PANEL_TAB_TRAILING_ACTION_WIDTH, PanelContainerConfig, PanelTabActivateHandler, PanelTabCloseHandler,
  PanelTabCloseIconRenderer, PanelTabStripActionsRenderer, render_panel_container,
};
pub use scroll::PanelTabScrollState;
