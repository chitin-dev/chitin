//! Panel tree layout and mutation algorithms.

use std::rc::Rc;

use gpui::{App, Pixels, Window, px};

use super::{
  drag::PanelTabDropTarget,
  model::{
    PanelId, PanelLeaf, PanelNode, PanelSplit, PanelSplitAxis, PanelSplitBranch, PanelSplitPath, PanelSplitPlacement,
    PanelTab, PanelTabId, PanelTree,
  },
  render::DEFAULT_PANEL_SPLIT_HANDLE_SIZE,
};

/// Minimum split ratio accepted by panel resize state.
pub const MIN_PANEL_SPLIT_RATIO: f32 = 0.1;
/// Maximum split ratio accepted by panel resize state.
pub const MAX_PANEL_SPLIT_RATIO: f32 = 0.9;

/// Callback invoked when a split resize gesture starts.
pub type PanelResizeStartHandler = dyn Fn(PanelSplitPath, PanelSplitAxis, Pixels, &mut Window, &mut App);

/// Configuration for panel split resize handles.
#[derive(Clone)]
pub struct PanelResizeConfig {
  /// Width or height of the split handle.
  pub(super) handle_size: Pixels,
  /// Callback invoked when a split resize gesture starts.
  pub(super) on_resize_start: Rc<PanelResizeStartHandler>,
}

impl PanelResizeConfig {
  /// Creates panel resize configuration.
  ///
  /// # Parameters
  ///
  /// `on_resize_start` is invoked with the split path, split axis, cursor
  /// position on the resize axis, window, and app context.
  ///
  /// # Returns
  ///
  /// A [`PanelResizeConfig`] with the default handle size.
  pub fn new(
    on_resize_start: impl Fn(PanelSplitPath, PanelSplitAxis, Pixels, &mut Window, &mut App) + 'static,
  ) -> Self {
    Self {
      handle_size: DEFAULT_PANEL_SPLIT_HANDLE_SIZE,
      on_resize_start: Rc::new(on_resize_start),
    }
  }

  /// Sets the split handle size.
  ///
  /// # Parameters
  ///
  /// `handle_size` is the width or height of rendered resize handles.
  ///
  /// # Returns
  ///
  /// The updated [`PanelResizeConfig`] for builder chaining.
  pub fn handle_size(mut self, handle_size: Pixels) -> Self {
    self.handle_size = handle_size;
    self
  }
}

impl<T> PanelTree<T> {
  /// Creates a panel tree with one leaf root.
  ///
  /// # Parameters
  ///
  /// `leaf` is the root leaf panel.
  ///
  /// # Returns
  ///
  /// A [`PanelTree`] containing `leaf` as its root node.
  pub fn single_leaf(leaf: PanelLeaf<T>) -> Self {
    Self {
      root: PanelNode::Leaf(leaf),
    }
  }

  /// Activates a tab inside one panel leaf.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the leaf panel that owns the tab.
  ///
  /// `tab_id` identifies the tab to activate.
  ///
  /// # Returns
  ///
  /// `true` when the panel and tab exist; otherwise `false`.
  pub fn activate_tab(&mut self, panel_id: PanelId, tab_id: PanelTabId) -> bool {
    log::trace!("Panel {:?}, tab {:?} is activated", panel_id, tab_id);
    find_leaf_mut(&mut self.root, panel_id)
      .map(|leaf| leaf.activate_tab(tab_id))
      .unwrap_or(false)
  }

  /// Closes a tab inside one panel leaf.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the leaf panel that owns the tab.
  ///
  /// `tab_id` identifies the tab to close.
  ///
  /// # Returns
  ///
  /// `true` when the panel and tab exist and the tab was removed; otherwise
  /// `false`.
  pub fn close_tab(&mut self, panel_id: PanelId, tab_id: PanelTabId) -> bool {
    log::trace!("Panel {:?}, tab {:?} is closed", panel_id, tab_id);
    find_leaf_mut(&mut self.root, panel_id)
      .map(|leaf| leaf.close_tab(tab_id))
      .unwrap_or(false)
  }

  /// Moves an existing tab to an insertion position in a panel leaf.
  ///
  /// This operation validates the complete move before changing the tree,
  /// transfers the existing [`PanelTab`] without recreating its payload,
  /// activates it in the target panel, and reuses [`Self::remove_leaf`] when a
  /// cross-panel move empties a non-final source panel.
  ///
  /// For same-panel moves, `insertion_index` is the raw position in the
  /// pre-removal tab strip. The method normalizes positions to the right of the
  /// source tab after removing that tab.
  ///
  /// # Parameters
  ///
  /// `source_panel_id` identifies the panel that currently owns `tab_id`.
  ///
  /// `tab_id` identifies the existing tab object to transfer.
  ///
  /// `target` identifies the receiving panel and raw insertion position.
  ///
  /// # Returns
  ///
  /// `true` when the requested move is valid, including an effective no-op;
  /// otherwise `false`. Invalid moves leave the panel tree unchanged.
  pub fn move_tab(&mut self, source_panel_id: PanelId, tab_id: PanelTabId, target: PanelTabDropTarget) -> bool {
    let Some(source_leaf) = self.leaf(source_panel_id) else {
      return false;
    };
    let Some(source_index) = source_leaf.tabs.iter().position(|tab| tab.id == tab_id) else {
      return false;
    };
    let Some(target_leaf) = self.leaf(target.panel_id) else {
      return false;
    };

    if source_panel_id == target.panel_id {
      let insertion_index =
        normalize_same_panel_insertion_index(source_index, target.insertion_index, source_leaf.tabs.len());
      let Some(leaf) = self.leaf_mut(source_panel_id) else {
        return false;
      };

      if insertion_index == source_index {
        return leaf.activate_tab(tab_id);
      }

      let Some(tab) = leaf.remove_tab(tab_id) else {
        return false;
      };
      leaf.insert_tab(insertion_index, tab);
      return true;
    }

    if target_leaf.tabs.iter().any(|tab| tab.id == tab_id) {
      return false;
    }

    let target_index = target.insertion_index.min(target_leaf.tabs.len());
    let Some(tab) = self.leaf_mut(source_panel_id).and_then(|leaf| leaf.remove_tab(tab_id)) else {
      return false;
    };
    let Some(target_leaf) = self.leaf_mut(target.panel_id) else {
      return false;
    };
    target_leaf.insert_tab(target_index, tab);

    let source_is_empty = self
      .leaf(source_panel_id)
      .map(|leaf| leaf.tabs.is_empty())
      .unwrap_or(false);
    if source_is_empty && self.leaf_count() > 1 {
      self.remove_leaf(source_panel_id);
    }

    true
  }

  /// Removes one leaf panel and collapses its parent split.
  ///
  /// The sibling subtree is promoted into the removed leaf's parent position.
  /// The root leaf is not removable, so callers can preserve a final empty
  /// panel when the tree contains only one leaf.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the leaf panel to remove.
  ///
  /// # Returns
  ///
  /// `Some(PanelId)` with a remaining leaf that can receive focus after the
  /// removal; otherwise `None` when no matching removable leaf exists.
  pub fn remove_leaf(&mut self, panel_id: PanelId) -> Option<PanelId> {
    log::trace!("Panel {:?} is removed", panel_id);
    remove_leaf_node(&mut self.root, panel_id)
  }

  /// Finds a leaf panel by identifier.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the leaf panel to find.
  ///
  /// # Returns
  ///
  /// `Some(&PanelLeaf<T>)` when the panel exists; otherwise `None`.
  pub fn leaf(&self, panel_id: PanelId) -> Option<&PanelLeaf<T>> {
    find_leaf(&self.root, panel_id)
  }

  /// Finds a mutable leaf panel by identifier.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the leaf panel to find.
  ///
  /// # Returns
  ///
  /// `Some(&mut PanelLeaf<T>)` when the panel exists; otherwise `None`.
  pub fn leaf_mut(&mut self, panel_id: PanelId) -> Option<&mut PanelLeaf<T>> {
    find_leaf_mut(&mut self.root, panel_id)
  }

  /// Splits one leaf panel into two leaves.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the existing leaf to split.
  ///
  /// `axis` controls whether the resulting leaves are side-by-side or stacked.
  ///
  /// `new_leaf` is the new leaf inserted before or after the existing leaf.
  ///
  /// `placement` controls whether `new_leaf` becomes the first or second child.
  ///
  /// `ratio` is the fraction of available space assigned to the first child.
  ///
  /// # Returns
  ///
  /// `true` when a matching leaf was split; otherwise `false`.
  pub fn split_leaf(
    &mut self,
    panel_id: PanelId,
    axis: PanelSplitAxis,
    new_leaf: PanelLeaf<T>,
    placement: PanelSplitPlacement,
    ratio: f32,
  ) -> bool {
    let mut new_leaf = Some(new_leaf);
    split_leaf_node(&mut self.root, panel_id, axis, &mut new_leaf, placement, ratio)
  }

  /// Resizes a split node by path.
  ///
  /// # Parameters
  ///
  /// `path` identifies the split node to resize.
  ///
  /// `ratio` is the new first-child space ratio.
  ///
  /// # Returns
  ///
  /// `true` when a matching split was found and resized; otherwise `false`.
  pub fn resize_split(&mut self, path: &PanelSplitPath, ratio: f32) -> bool {
    resize_split_node(&mut self.root, path.branches(), ratio)
  }

  /// Returns the ratio for a split node by path.
  ///
  /// # Parameters
  ///
  /// `path` identifies the split node to inspect.
  ///
  /// # Returns
  ///
  /// `Some(f32)` with the current split ratio when the path identifies a split;
  /// otherwise `None`.
  pub fn split_ratio(&self, path: &PanelSplitPath) -> Option<f32> {
    find_split(&self.root, path.branches()).map(|split| split.ratio)
  }

  /// Returns the available size on a split node's resize axis.
  ///
  /// # Parameters
  ///
  /// `path` identifies the split node to inspect.
  ///
  /// `root_width` is the rendered width available to the root panel node.
  ///
  /// `root_height` is the rendered height available to the root panel node.
  ///
  /// # Returns
  ///
  /// `Some(Pixels)` containing the available size on the target split's axis
  /// when the path identifies a split; otherwise `None`.
  pub fn split_axis_size(&self, path: &PanelSplitPath, root_width: Pixels, root_height: Pixels) -> Option<Pixels> {
    split_axis_size_in_node(&self.root, path.branches(), root_width, root_height)
  }

  /// Returns the number of leaf panels in the tree.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// The number of leaf panels below the root node.
  pub fn leaf_count(&self) -> usize {
    count_leaves(&self.root)
  }

  /// Returns every tab id in panel-tree visual order.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// A vector of `(PanelId, PanelTabId)` pairs in render order.
  pub fn tabs_in_order(&self) -> Vec<(PanelId, PanelTabId)> {
    let mut tabs = Vec::new();
    collect_tab_ids(&self.root, &mut tabs);
    tabs
  }

  /// Returns every active tab id in panel-tree visual order.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// A vector of `(PanelId, PanelTabId)` pairs for leaves with active tabs.
  pub fn active_tabs_in_order(&self) -> Vec<(PanelId, PanelTabId)> {
    let mut tabs = Vec::new();
    collect_active_tab_ids(&self.root, &mut tabs);
    tabs
  }

  /// Finds one tab by panel and tab identifier.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the leaf panel that should contain the tab.
  ///
  /// `tab_id` identifies the tab to find.
  ///
  /// # Returns
  ///
  /// `Some(&PanelTab<T>)` when the panel and tab exist; otherwise `None`.
  pub fn find_tab(&self, panel_id: PanelId, tab_id: PanelTabId) -> Option<&PanelTab<T>> {
    self
      .leaf(panel_id)
      .and_then(|leaf| leaf.tabs.iter().find(|tab| tab.id == tab_id))
  }

  /// Returns the first active tab in panel-tree visual order.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// `Some((PanelId, &PanelTab<T>))` for the first active tab, or `None` when no
  /// panel has an active tab.
  pub fn first_active_tab(&self) -> Option<(PanelId, &PanelTab<T>)> {
    first_active_tab_in_node(&self.root)
  }
}

/// Clamps a split ratio to supported panel bounds.
///
/// # Parameters
///
/// `ratio` is the requested first-child split ratio.
///
/// # Returns
///
/// A finite ratio in the inclusive supported range.
pub(super) fn clamp_split_ratio(ratio: f32) -> f32 {
  if ratio.is_finite() {
    ratio.clamp(MIN_PANEL_SPLIT_RATIO, MAX_PANEL_SPLIT_RATIO)
  } else {
    0.5
  }
}

/// Normalizes a same-panel raw insertion index after removing the source tab.
///
/// # Parameters
///
/// `source_index` is the tab position before removal.
///
/// `raw_insertion_index` is the visual insertion position in the original
/// strip.
///
/// `tab_count` is the source panel's tab count before removal.
///
/// # Returns
///
/// The clamped insertion index in the post-removal collection.
fn normalize_same_panel_insertion_index(source_index: usize, raw_insertion_index: usize, tab_count: usize) -> usize {
  let raw_insertion_index = raw_insertion_index.min(tab_count);
  let normalized_index = if source_index < raw_insertion_index {
    raw_insertion_index.saturating_sub(1)
  } else {
    raw_insertion_index
  };

  normalized_index.min(tab_count.saturating_sub(1))
}

/// Finds an immutable leaf panel by identifier.
///
/// # Parameters
///
/// `node` is the current tree node being searched.
///
/// `panel_id` is the leaf panel identifier to find.
///
/// # Returns
///
/// `Some(&PanelLeaf<T>)` when the leaf exists; otherwise `None`.
fn find_leaf<T>(node: &PanelNode<T>, panel_id: PanelId) -> Option<&PanelLeaf<T>> {
  match node {
    PanelNode::Leaf(leaf) if leaf.id == panel_id => Some(leaf),
    PanelNode::Leaf(_) => None,
    PanelNode::Split(split) => find_leaf(&split.first, panel_id).or_else(|| find_leaf(&split.second, panel_id)),
  }
}

/// Finds a mutable leaf panel by identifier.
///
/// # Parameters
///
/// `node` is the current tree node being searched.
///
/// `panel_id` is the leaf panel identifier to find.
///
/// # Returns
///
/// `Some(&mut PanelLeaf<T>)` when the leaf exists; otherwise `None`.
fn find_leaf_mut<T>(node: &mut PanelNode<T>, panel_id: PanelId) -> Option<&mut PanelLeaf<T>> {
  match node {
    PanelNode::Leaf(leaf) if leaf.id == panel_id => Some(leaf),
    PanelNode::Leaf(_) => None,
    PanelNode::Split(split) => {
      find_leaf_mut(&mut split.first, panel_id).or_else(|| find_leaf_mut(&mut split.second, panel_id))
    }
  }
}

/// Finds an immutable split node by branch path.
///
/// # Parameters
///
/// `node` is the current tree node being searched.
///
/// `path` is the remaining branch path to the target split.
///
/// # Returns
///
/// `Some(&PanelSplit<T>)` when `path` identifies a split node; otherwise
/// `None`.
fn find_split<'a, T>(node: &'a PanelNode<T>, path: &[PanelSplitBranch]) -> Option<&'a PanelSplit<T>> {
  let PanelNode::Split(split) = node else {
    return None;
  };

  let Some((next, rest)) = path.split_first() else {
    return Some(split);
  };

  match next {
    PanelSplitBranch::First => find_split(&split.first, rest),
    PanelSplitBranch::Second => find_split(&split.second, rest),
  }
}

/// Computes the available axis size for one split node.
///
/// # Parameters
///
/// `node` is the current tree node being searched.
///
/// `path` is the remaining branch path to the target split.
///
/// `width` is the rendered width available to `node`.
///
/// `height` is the rendered height available to `node`.
///
/// # Returns
///
/// `Some(Pixels)` containing the target split's axis size, or `None` when the
/// path does not identify a split node.
fn split_axis_size_in_node<T>(
  node: &PanelNode<T>,
  path: &[PanelSplitBranch],
  width: Pixels,
  height: Pixels,
) -> Option<Pixels> {
  let PanelNode::Split(split) = node else {
    return None;
  };

  let Some((next, rest)) = path.split_first() else {
    return Some(match split.axis {
      PanelSplitAxis::Horizontal => width,
      PanelSplitAxis::Vertical => height,
    });
  };

  let first_ratio = split.ratio;
  let second_ratio = 1.0 - split.ratio;
  let (child, child_width, child_height) = match (split.axis, next) {
    (PanelSplitAxis::Horizontal, PanelSplitBranch::First) => (&split.first, px(f32::from(width) * first_ratio), height),
    (PanelSplitAxis::Horizontal, PanelSplitBranch::Second) => {
      (&split.second, px(f32::from(width) * second_ratio), height)
    }
    (PanelSplitAxis::Vertical, PanelSplitBranch::First) => (&split.first, width, px(f32::from(height) * first_ratio)),
    (PanelSplitAxis::Vertical, PanelSplitBranch::Second) => {
      (&split.second, width, px(f32::from(height) * second_ratio))
    }
  };

  split_axis_size_in_node(child, rest, child_width, child_height)
}

/// Replaces a matching leaf with a split node.
///
/// # Parameters
///
/// `node` is the current tree node being searched.
///
/// `panel_id` identifies the leaf to split.
///
/// `axis` controls the new split orientation.
///
/// `new_leaf` is the new leaf inserted around the existing leaf.
///
/// `placement` controls whether `new_leaf` is first or second.
///
/// `ratio` controls the first-child split ratio.
///
/// # Returns
///
/// `true` when a matching leaf was replaced; otherwise `false`.
fn split_leaf_node<T>(
  node: &mut PanelNode<T>,
  panel_id: PanelId,
  axis: PanelSplitAxis,
  new_leaf: &mut Option<PanelLeaf<T>>,
  placement: PanelSplitPlacement,
  ratio: f32,
) -> bool {
  match node {
    PanelNode::Leaf(leaf) if leaf.id == panel_id => {
      let Some(new_leaf) = new_leaf.take() else {
        return false;
      };
      let existing = std::mem::replace(node, PanelNode::Leaf(PanelLeaf::new(PanelId::new(0))));
      let inserted = PanelNode::Leaf(new_leaf);
      let (first, second) = match placement {
        PanelSplitPlacement::Before => (inserted, existing),
        PanelSplitPlacement::After => (existing, inserted),
      };
      let new_node = PanelSplit::new(axis, ratio, first, second);
      *node = PanelNode::Split(new_node);
      true
    }
    PanelNode::Leaf(_) => false,
    PanelNode::Split(split) => {
      if split_leaf_node(&mut split.first, panel_id, axis, new_leaf, placement, ratio) {
        return true;
      }

      split_leaf_node(&mut split.second, panel_id, axis, new_leaf, placement, ratio)
    }
  }
}

/// Removes a matching leaf below a split and promotes its sibling.
///
/// # Parameters
///
/// `node` is the current tree node being searched.
///
/// `panel_id` identifies the leaf panel to remove.
///
/// # Returns
///
/// `Some(PanelId)` with the first remaining leaf in the promoted sibling
/// subtree when a leaf was removed; otherwise `None`.
fn remove_leaf_node<T>(node: &mut PanelNode<T>, panel_id: PanelId) -> Option<PanelId> {
  let PanelNode::Split(split) = node else {
    return None;
  };
  let remove_first = matches!(split.first.as_ref(), PanelNode::Leaf(leaf) if leaf.id == panel_id);
  let remove_second = matches!(split.second.as_ref(), PanelNode::Leaf(leaf) if leaf.id == panel_id);

  if remove_first || remove_second {
    let old_node = std::mem::replace(node, PanelNode::Leaf(PanelLeaf::new(PanelId::new(0))));
    let PanelNode::Split(old_split) = old_node else {
      return None;
    };
    let promoted = if remove_first {
      *old_split.second
    } else {
      *old_split.first
    };
    let focused_panel_id = first_leaf_id(&promoted);

    *node = promoted;
    return focused_panel_id;
  }

  remove_leaf_node(&mut split.first, panel_id).or_else(|| remove_leaf_node(&mut split.second, panel_id))
}

/// Returns the first leaf id in tree order.
///
/// # Parameters
///
/// `node` is the tree node whose first leaf should be found.
///
/// # Returns
///
/// `Some(PanelId)` for the first leaf under `node`.
fn first_leaf_id<T>(node: &PanelNode<T>) -> Option<PanelId> {
  match node {
    PanelNode::Leaf(leaf) => Some(leaf.id),
    PanelNode::Split(split) => first_leaf_id(&split.first).or_else(|| first_leaf_id(&split.second)),
  }
}

/// Resizes a split node by branch path.
///
/// # Parameters
///
/// `node` is the current tree node being searched.
///
/// `path` is the remaining branch path to the target split.
///
/// `ratio` is the requested split ratio.
///
/// # Returns
///
/// `true` when a matching split was resized; otherwise `false`.
fn resize_split_node<T>(node: &mut PanelNode<T>, path: &[PanelSplitBranch], ratio: f32) -> bool {
  let PanelNode::Split(split) = node else {
    return false;
  };

  let Some((next, rest)) = path.split_first() else {
    split.ratio = clamp_split_ratio(ratio);
    return true;
  };

  match next {
    PanelSplitBranch::First => resize_split_node(&mut split.first, rest, ratio),
    PanelSplitBranch::Second => resize_split_node(&mut split.second, rest, ratio),
  }
}

/// Counts leaf panels below a node.
///
/// # Parameters
///
/// `node` is the current tree node being counted.
///
/// # Returns
///
/// The number of leaf panels below `node`.
fn count_leaves<T>(node: &PanelNode<T>) -> usize {
  match node {
    PanelNode::Leaf(_) => 1,
    PanelNode::Split(split) => count_leaves(&split.first) + count_leaves(&split.second),
  }
}

/// Appends every tab id below one node to an existing order list.
///
/// # Parameters
///
/// `node` is the panel tree node to traverse.
///
/// `tabs` receives panel and tab identifiers in visual order.
///
/// # Returns
///
/// This function returns `()` after appending identifiers.
fn collect_tab_ids<T>(node: &PanelNode<T>, tabs: &mut Vec<(PanelId, PanelTabId)>) {
  match node {
    PanelNode::Leaf(leaf) => {
      tabs.extend(leaf.tabs.iter().map(|tab| (leaf.id, tab.id)));
    }
    PanelNode::Split(split) => {
      collect_tab_ids(&split.first, tabs);
      collect_tab_ids(&split.second, tabs);
    }
  }
}

/// Appends every active tab id below one node to an existing order list.
///
/// # Parameters
///
/// `node` is the panel tree node to traverse.
///
/// `tabs` receives panel and active-tab identifiers in visual order.
///
/// # Returns
///
/// This function returns `()` after appending identifiers.
fn collect_active_tab_ids<T>(node: &PanelNode<T>, tabs: &mut Vec<(PanelId, PanelTabId)>) {
  match node {
    PanelNode::Leaf(leaf) => {
      if let Some(tab_id) = leaf.active_tab {
        tabs.push((leaf.id, tab_id));
      }
    }
    PanelNode::Split(split) => {
      collect_active_tab_ids(&split.first, tabs);
      collect_active_tab_ids(&split.second, tabs);
    }
  }
}

/// Finds the first active tab below one node in visual order.
///
/// # Parameters
///
/// `node` is the panel tree node to traverse.
///
/// # Returns
///
/// `Some((PanelId, &PanelTab<T>))` for the first active tab, or `None` when no
/// panel has an active tab.
fn first_active_tab_in_node<T>(node: &PanelNode<T>) -> Option<(PanelId, &PanelTab<T>)> {
  match node {
    PanelNode::Leaf(leaf) => leaf.active_tab().map(|tab| (leaf.id, tab)),
    PanelNode::Split(split) => {
      first_active_tab_in_node(&split.first).or_else(|| first_active_tab_in_node(&split.second))
    }
  }
}
