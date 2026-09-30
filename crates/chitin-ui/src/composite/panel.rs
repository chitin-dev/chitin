//! Application-neutral document tree and stable identifiers.
//!
//! This module contains only data and command-level topology operations.
//! GPUI Kit owns docking chrome, scrolling, resize, and drag interactions.

mod layout;
mod model;

#[cfg(test)]
mod tests;

pub use layout::{MAX_PANEL_SPLIT_RATIO, MIN_PANEL_SPLIT_RATIO};
pub use model::{
  PanelId, PanelLeaf, PanelNode, PanelSplit, PanelSplitAxis, PanelSplitBranch, PanelSplitPath, PanelSplitPlacement,
  PanelTab, PanelTabDropTarget, PanelTabId, PanelTree,
};
