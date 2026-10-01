//! Desktop document command tree and stable identifiers.
//!
//! This module contains only data and command-level topology operations.
//! GPUI Kit owns docking chrome, scrolling, resize, and drag interactions.

mod model;
mod operations;

#[cfg(test)]
mod tests;

pub use model::{
  PanelId, PanelLeaf, PanelNode, PanelSplit, PanelSplitAxis, PanelSplitBranch, PanelSplitPath, PanelSplitPlacement,
  PanelTab, PanelTabDropTarget, PanelTabId, PanelTree,
};
pub use operations::{MAX_PANEL_SPLIT_RATIO, MIN_PANEL_SPLIT_RATIO};
