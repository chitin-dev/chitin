//! Reusable multi-panel container primitives.
//!
//! IDE docking layouts are commonly represented as a binary tree: internal
//! nodes are splits, and leaf nodes are tab stacks. This module provides that
//! model plus a lightweight GPUI renderer for panel chrome. Application crates
//! keep concrete view rendering and command wiring outside `chitin-ui`.

use std::rc::Rc;

use gpui::{
  AnyElement, App, Bounds, Context, CursorStyle, Div, InteractiveElement, MouseButton,
  ParentElement, Pixels, SharedString, Styled, Window, div, prelude::*, px, relative,
};

use crate::themes::UIThemes;

/// Minimum split ratio accepted by panel resize state.
pub const MIN_PANEL_SPLIT_RATIO: f32 = 0.1;
/// Maximum split ratio accepted by panel resize state.
pub const MAX_PANEL_SPLIT_RATIO: f32 = 0.9;
/// Default width of a split resize handle.
pub const DEFAULT_PANEL_SPLIT_HANDLE_SIZE: Pixels = px(4.0);
/// Default height of tab strip
pub const DEFAULT_PANEL_TAB_STRIP_HEIGHT: Pixels = px(34.0);
/// Default square size reserved for a panel tab close button.
pub const DEFAULT_PANEL_TAB_CLOSE_BUTTON_SIZE: Pixels = px(18.0);
/// Default width reserved for trailing tab actions.
pub const DEFAULT_PANEL_TAB_TRAILING_ACTION_WIDTH: Pixels = DEFAULT_PANEL_TAB_STRIP_HEIGHT;

/// Callback invoked when a split resize gesture starts.
pub type PanelResizeStartHandler =
  dyn Fn(PanelSplitPath, PanelSplitAxis, Pixels, &mut Window, &mut App);
/// Callback invoked when a tab is activated.
pub type PanelTabActivateHandler = dyn Fn(PanelId, PanelTabId, &mut Window, &mut App);
/// Callback invoked when a tab close button is pressed.
pub type PanelTabCloseHandler = dyn Fn(PanelId, PanelTabId, &mut Window, &mut App);
/// Callback invoked when GPUI starts a panel-tab drag.
pub type PanelTabDragStartHandler = dyn Fn(PanelTabDrag, &mut Window, &mut App);
/// Callback invoked when a panel-tab drag enters a new insertion position.
pub type PanelTabDragTargetHandler = dyn Fn(PanelTabDropTarget, &mut Window, &mut App);
/// Callback invoked when a panel tab is dropped on a valid tab strip.
pub type PanelTabDropHandler = dyn Fn(PanelTabDrag, PanelId, &mut Window, &mut App);
/// Renderer for the icon shown inside one tab close button.
pub type PanelTabCloseIconRenderer = dyn Fn(UIThemes) -> AnyElement;
/// Renderer for caller-owned actions placed at the end of a panel tab strip.
pub type PanelTabStripActionsRenderer = dyn Fn(PanelId) -> AnyElement;

/// Stable identifier for a leaf panel.
///
/// Panel IDs are intentionally numeric and application-owned. They should be
/// stable for a session so commands, focus state, and future docking operations
/// can refer to a leaf without depending on its title or visible position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PanelId(u64);

impl PanelId {
  /// Creates a panel identifier from a numeric value.
  ///
  /// # Parameters
  ///
  /// `value` is the caller-owned identifier value.
  ///
  /// # Returns
  ///
  /// A [`PanelId`] wrapping `value`.
  pub const fn new(value: u64) -> Self {
    Self(value)
  }

  /// Returns the raw numeric panel identifier.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// The numeric identifier stored in this [`PanelId`].
  pub const fn value(self) -> u64 {
    self.0
  }
}

/// Stable identifier for a tab inside one panel container.
///
/// Applications should keep these identifiers unique across the container so a
/// drag payload remains unambiguous while a tab moves between panel leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PanelTabId(u64);

impl PanelTabId {
  /// Creates a tab identifier from a numeric value.
  ///
  /// # Parameters
  ///
  /// `value` is the caller-owned identifier value.
  ///
  /// # Returns
  ///
  /// A [`PanelTabId`] wrapping `value`.
  pub const fn new(value: u64) -> Self {
    Self(value)
  }

  /// Returns the raw numeric tab identifier.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// The numeric identifier stored in this [`PanelTabId`].
  pub const fn value(self) -> u64 {
    self.0
  }
}

/// Stable payload carried by GPUI while one panel tab is being dragged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelTabDrag {
  /// Panel that owns the tab when the drag starts.
  pub source_panel_id: PanelId,
  /// Stable identifier of the dragged tab.
  pub tab_id: PanelTabId,
  /// Tab position when the drag starts.
  pub source_index: usize,
  /// Tab title rendered by the lightweight drag preview.
  pub title: SharedString,
}

/// Proposed insertion position in an existing panel tab strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelTabDropTarget {
  /// Panel that will receive the dragged tab.
  pub panel_id: PanelId,
  /// Raw insertion position before same-panel removal normalization.
  pub insertion_index: usize,
}

/// Temporary presentation state for one active panel-tab drag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelTabDragState {
  /// Stable drag payload created when GPUI crosses its drag threshold.
  pub drag: PanelTabDrag,
  /// Current valid tab-strip target, if the pointer is over one.
  pub drop_target: Option<PanelTabDropTarget>,
}

/// Axis used by one binary split node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelSplitAxis {
  /// Split children left and right.
  Horizontal,
  /// Split children top and bottom.
  Vertical,
}

/// Placement for a new leaf when splitting an existing panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelSplitPlacement {
  /// Place the new leaf before the existing leaf.
  Before,
  /// Place the new leaf after the existing leaf.
  After,
}

/// Branch selector inside a split path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PanelSplitBranch {
  /// Traverse into the first split child.
  First,
  /// Traverse into the second split child.
  Second,
}

/// Path from the root panel node to a split node.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct PanelSplitPath {
  /// Branches traversed before reaching the split.
  branches: Vec<PanelSplitBranch>,
}

impl PanelSplitPath {
  /// Creates the root split path.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A [`PanelSplitPath`] that identifies the root node when it is a split.
  pub fn root() -> Self {
    Self {
      branches: Vec::new(),
    }
  }

  /// Returns a child path below this path. This function will not modify the
  /// content of `self.branches`, it returns branch new [`PanelSplitPath`]
  ///
  /// # Parameters
  ///
  /// `branch` identifies the split child to append.
  ///
  /// # Returns
  ///
  /// A new [`PanelSplitPath`] containing this path plus `branch`.
  pub fn child(&self, branch: PanelSplitBranch) -> Self {
    let mut branches = self.branches.clone();
    branches.push(branch);
    Self { branches }
  }

  /// Returns the branches in this path.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// A borrowed slice of branch selectors.
  pub fn branches(&self) -> &[PanelSplitBranch] {
    &self.branches
  }
}

/// One tab hosted by a leaf panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelTab<T> {
  /// Stable tab identifier.
  pub id: PanelTabId,
  /// Title shown in the panel tab strip.
  pub title: SharedString,
  /// Caller-owned view payload.
  pub payload: T,
}

impl<T> PanelTab<T> {
  /// Creates a panel tab.
  ///
  /// # Parameters
  ///
  /// `id` is the stable tab identifier.
  ///
  /// `title` is the label rendered in the tab strip.
  ///
  /// `payload` is the caller-owned view payload associated with the tab.
  ///
  /// # Returns
  ///
  /// A [`PanelTab`] containing the provided metadata and payload.
  pub fn new(id: PanelTabId, title: impl Into<SharedString>, payload: T) -> Self {
    Self {
      id,
      title: title.into(),
      payload,
    }
  }
}

/// A leaf panel containing a tab stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelLeaf<T> {
  /// Stable panel identifier.
  pub id: PanelId,
  /// Tabs hosted by this panel.
  pub tabs: Vec<PanelTab<T>>,
  /// Active tab shown in the panel body.
  pub active_tab: Option<PanelTabId>,
}

impl<T> PanelLeaf<T> {
  /// Creates an empty leaf panel.
  ///
  /// # Parameters
  ///
  /// `id` is the stable panel identifier.
  ///
  /// # Returns
  ///
  /// A [`PanelLeaf`] with no tabs and no active tab.
  pub fn new(id: PanelId) -> Self {
    Self {
      id,
      tabs: Vec::new(),
      active_tab: None,
    }
  }

  /// Appends a tab to this panel.
  ///
  /// The first tab added to an empty panel becomes active automatically.
  ///
  /// # Parameters
  ///
  /// `tab` is the tab to append.
  ///
  /// # Returns
  ///
  /// The updated [`PanelLeaf`] for builder chaining.
  pub fn tab(mut self, tab: PanelTab<T>) -> Self {
    self.add_tab(tab);
    self
  }

  /// Appends a tab to this panel.
  ///
  /// # Parameters
  ///
  /// `tab` is the tab to append.
  ///
  /// # Returns
  ///
  /// This function returns `()` and mutates the tab stack.
  pub fn add_tab(&mut self, tab: PanelTab<T>) {
    if self.active_tab.is_none() {
      self.active_tab = Some(tab.id);
    }

    self.tabs.push(tab);
  }

  /// Inserts a tab at a requested position and activates it.
  ///
  /// # Parameters
  ///
  /// `index` is clamped to the current tab count, so `usize::MAX` appends.
  ///
  /// `tab` is the existing tab object transferred into this panel.
  ///
  /// # Returns
  ///
  /// The final insertion index.
  pub fn insert_tab(&mut self, index: usize, tab: PanelTab<T>) -> usize {
    let index = index.min(self.tabs.len());
    self.active_tab = Some(tab.id);
    self.tabs.insert(index, tab);
    index
  }

  /// Activates a tab in this panel.
  ///
  /// # Parameters
  ///
  /// `tab_id` is the tab identifier to activate.
  ///
  /// # Returns
  ///
  /// `true` when the tab exists and was made active; otherwise `false`.
  pub fn activate_tab(&mut self, tab_id: PanelTabId) -> bool {
    if self.tabs.iter().any(|tab| tab.id == tab_id) {
      self.active_tab = Some(tab_id);
      return true;
    }

    false
  }

  /// Closes a tab in this panel.
  ///
  /// If the closed tab was active, the next tab at the same index becomes
  /// active. When there is no following tab, the previous tab becomes active.
  /// Closing the last tab leaves the panel without an active tab.
  ///
  /// # Parameters
  ///
  /// `tab_id` is the tab identifier to remove.
  ///
  /// # Returns
  ///
  /// `true` when the tab existed and was removed; otherwise `false`.
  pub fn close_tab(&mut self, tab_id: PanelTabId) -> bool {
    self.remove_tab(tab_id).is_some()
  }

  /// Removes and returns a tab while repairing active-tab selection.
  ///
  /// The fallback policy matches ordinary close behavior: when the removed tab
  /// was active, the tab now at the same index is selected, or the preceding
  /// tab when the removed tab was last.
  ///
  /// # Parameters
  ///
  /// `tab_id` identifies the tab whose ownership should leave this panel.
  ///
  /// # Returns
  ///
  /// `Some(PanelTab<T>)` when the tab existed; otherwise `None`.
  pub fn remove_tab(&mut self, tab_id: PanelTabId) -> Option<PanelTab<T>> {
    let index = self.tabs.iter().position(|tab| tab.id == tab_id)?;
    let closed_was_active = self.active_tab == Some(tab_id);

    let tab = self.tabs.remove(index);

    if closed_was_active {
      self.active_tab = self
        .tabs
        .get(index)
        .or_else(|| index.checked_sub(1).and_then(|index| self.tabs.get(index)))
        .map(|tab| tab.id);
    }

    Some(tab)
  }

  /// Returns this panel's active tab.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// `Some(&PanelTab<T>)` when an active tab exists; otherwise `None`.
  pub fn active_tab(&self) -> Option<&PanelTab<T>> {
    self
      .active_tab
      .and_then(|active_tab| self.tabs.iter().find(|tab| tab.id == active_tab))
  }
}

/// Internal split node in a panel tree.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelSplit<T> {
  /// Split direction for the two children.
  pub axis: PanelSplitAxis,
  /// Fraction of available space assigned to the first child.
  pub ratio: f32,
  /// First child in the split.
  pub first: Box<PanelNode<T>>,
  /// Second child in the split.
  pub second: Box<PanelNode<T>>,
}

impl<T> PanelSplit<T> {
  /// Creates a split from two child nodes.
  ///
  /// # Parameters
  ///
  /// `axis` controls whether children are placed side-by-side or stacked.
  ///
  /// `ratio` is the fraction of available space assigned to the first child.
  ///
  /// `first` is the first split child.
  ///
  /// `second` is the second split child.
  ///
  /// # Returns
  ///
  /// A [`PanelSplit`] with its ratio clamped to supported bounds.
  pub fn new(axis: PanelSplitAxis, ratio: f32, first: PanelNode<T>, second: PanelNode<T>) -> Self {
    Self {
      axis,
      ratio: clamp_split_ratio(ratio),
      first: Box::new(first),
      second: Box::new(second),
    }
  }
}

/// One node in the binary panel tree.
#[derive(Clone, Debug, PartialEq)]
pub enum PanelNode<T> {
  /// Leaf node containing one tab stack.
  Leaf(PanelLeaf<T>),
  /// Internal node splitting space between two child nodes.
  Split(PanelSplit<T>),
}

/// Binary panel tree containing split nodes and tab-stack leaves.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelTree<T> {
  /// Root node of the panel tree.
  pub root: PanelNode<T>,
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
  pub fn move_tab(
    &mut self,
    source_panel_id: PanelId,
    tab_id: PanelTabId,
    target: PanelTabDropTarget,
  ) -> bool {
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
      let insertion_index = normalize_same_panel_insertion_index(
        source_index,
        target.insertion_index,
        source_leaf.tabs.len(),
      );
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
    let Some(tab) = self
      .leaf_mut(source_panel_id)
      .and_then(|leaf| leaf.remove_tab(tab_id))
    else {
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
    split_leaf_node(
      &mut self.root,
      panel_id,
      axis,
      &mut new_leaf,
      placement,
      ratio,
    )
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
  pub fn split_axis_size(
    &self,
    path: &PanelSplitPath,
    root_width: Pixels,
    root_height: Pixels,
  ) -> Option<Pixels> {
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
}

/// Configuration for panel split resize handles.
#[derive(Clone)]
pub struct PanelResizeConfig {
  /// Width or height of the split handle.
  handle_size: Pixels,
  /// Callback invoked when a split resize gesture starts.
  on_resize_start: Rc<PanelResizeStartHandler>,
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

/// Callbacks and temporary presentation state for panel-tab dragging.
#[derive(Clone)]
pub struct PanelTabDragConfig {
  /// Current drag session used to render source and target feedback.
  state: Option<PanelTabDragState>,
  /// Callback invoked after GPUI crosses its pointer movement threshold.
  on_start: Rc<PanelTabDragStartHandler>,
  /// Callback invoked when actual tab bounds produce a drop target.
  on_target: Rc<PanelTabDragTargetHandler>,
  /// Callback invoked when GPUI drops the payload on a valid tab strip.
  on_drop: Rc<PanelTabDropHandler>,
}

impl PanelTabDragConfig {
  /// Creates tab drag configuration from application-owned callbacks.
  ///
  /// # Parameters
  ///
  /// `on_start` begins the application's temporary drag session.
  ///
  /// `on_target` records the current panel and raw insertion position.
  ///
  /// `on_drop` commits a payload released over a valid tab strip.
  ///
  /// # Returns
  ///
  /// A [`PanelTabDragConfig`] without an active presentation state.
  pub fn new(
    on_start: Rc<PanelTabDragStartHandler>,
    on_target: Rc<PanelTabDragTargetHandler>,
    on_drop: Rc<PanelTabDropHandler>,
  ) -> Self {
    Self {
      state: None,
      on_start,
      on_target,
      on_drop,
    }
  }

  /// Sets the current temporary drag state used by panel rendering.
  ///
  /// # Parameters
  ///
  /// `state` identifies the dragged tab and current valid insertion target.
  ///
  /// # Returns
  ///
  /// The updated [`PanelTabDragConfig`] for builder chaining.
  pub fn state(mut self, state: Option<PanelTabDragState>) -> Self {
    self.state = state;
    self
  }
}

/// Optional chrome behavior for [`render_panel_container`].
#[derive(Clone, Default)]
pub struct PanelContainerConfig {
  /// Optional resize configuration used by split handles.
  resize: Option<PanelResizeConfig>,
  /// Current focused panel id, normally transferred by upper component
  /// [`DocumentPanelState`]
  focused_panel_id: Option<PanelId>,
  /// Optional callback invoked when a tab is activated.
  on_activate_tab: Option<Rc<PanelTabActivateHandler>>,
  /// Optional callback invoked when a tab close button is pressed.
  on_close_tab: Option<Rc<PanelTabCloseHandler>>,
  /// Optional renderer for the tab close icon.
  render_tab_close_icon: Option<Rc<PanelTabCloseIconRenderer>>,
  /// Optional renderer for caller-owned tab-strip actions.
  render_tab_strip_actions: Option<Rc<PanelTabStripActionsRenderer>>,
  /// Optional native GPUI tab drag-and-drop behavior.
  tab_drag: Option<PanelTabDragConfig>,
}

impl PanelContainerConfig {
  /// Creates an empty panel container configuration.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A [`PanelContainerConfig`] with optional behaviors disabled.
  pub fn new() -> Self {
    Self::default()
  }

  /// Enables split resize handles.
  ///
  /// # Parameters
  ///
  /// `resize` configures resize handle size and resize-start callbacks.
  ///
  /// # Returns
  ///
  /// The updated [`PanelContainerConfig`] for builder chaining.
  pub fn resize(mut self, resize: PanelResizeConfig) -> Self {
    self.resize = Some(resize);
    self
  }

  /// Sets current focused panel id used to compare the panel container config
  /// with it, used to draw activated tab in focused panel differently.
  ///
  /// # Parameters
  ///
  /// `panel_id` configures current focused panel id
  ///
  /// # Returns
  ///
  /// The updated [`PanelContainerConfig`] for builder chaining.
  pub fn focused_panel_id(mut self, panel_id: PanelId) -> Self {
    self.focused_panel_id = Some(panel_id);
    self
  }

  /// Sets the tab activation callback.
  ///
  /// # Parameters
  ///
  /// `handler` is invoked when a rendered tab is pressed.
  ///
  /// # Returns
  ///
  /// The updated [`PanelContainerConfig`] for builder chaining.
  pub fn on_activate_tab(mut self, handler: Rc<PanelTabActivateHandler>) -> Self {
    self.on_activate_tab = Some(handler);
    self
  }

  /// Sets the tab close callback.
  ///
  /// # Parameters
  ///
  /// `handler` is invoked when a rendered tab close button is pressed.
  ///
  /// # Returns
  ///
  /// The updated [`PanelContainerConfig`] for builder chaining.
  pub fn on_close_tab(mut self, handler: Rc<PanelTabCloseHandler>) -> Self {
    self.on_close_tab = Some(handler);
    self
  }

  /// Sets the tab close icon renderer.
  ///
  /// # Parameters
  ///
  /// `renderer` produces the visual icon inside the reusable close button slot.
  ///
  /// # Returns
  ///
  /// The updated [`PanelContainerConfig`] for builder chaining.
  pub fn render_tab_close_icon(mut self, renderer: Rc<PanelTabCloseIconRenderer>) -> Self {
    self.render_tab_close_icon = Some(renderer);
    self
  }

  /// Sets the tab-strip action renderer.
  ///
  /// # Parameters
  ///
  /// `renderer` produces caller-owned controls at the right end of each panel
  /// tab strip.
  ///
  /// # Returns
  ///
  /// The updated [`PanelContainerConfig`] for builder chaining.
  pub fn render_tab_strip_actions(mut self, renderer: Rc<PanelTabStripActionsRenderer>) -> Self {
    self.render_tab_strip_actions = Some(renderer);
    self
  }

  /// Enables native GPUI drag-and-drop for panel tabs.
  ///
  /// # Parameters
  ///
  /// `tab_drag` supplies temporary state plus start, target, and drop callbacks.
  ///
  /// # Returns
  ///
  /// The updated [`PanelContainerConfig`] for builder chaining.
  pub fn tab_drag(mut self, tab_drag: PanelTabDragConfig) -> Self {
    self.tab_drag = Some(tab_drag);
    self
  }
}

/// Shared dependencies used while rendering one panel tree.
struct PanelRenderContext<'a, T> {
  /// Theme tokens used by panel chrome.
  theme: UIThemes,
  /// Optional chrome behavior used while rendering.
  config: &'a PanelContainerConfig,
  /// Renderer for active tab body content.
  render_body: &'a dyn Fn(&PanelTab<T>) -> AnyElement,
}

/// Lightweight GPUI view displayed under the pointer during a tab drag.
struct PanelTabDragPreview {
  /// Title copied from the stable drag payload.
  title: SharedString,
  /// Theme used to match existing panel chrome.
  theme: UIThemes,
}

impl Render for PanelTabDragPreview {
  /// Renders a compact tab-title preview without duplicating panel content.
  ///
  /// # Parameters
  ///
  /// `window` is unused because the preview has no window-specific state.
  ///
  /// `cx` is unused because the preview has no mutable entity state.
  ///
  /// # Returns
  ///
  /// A GPUI element styled from the current UI theme.
  fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
    div()
      .max_w(px(220.0))
      .px_3()
      .h(DEFAULT_PANEL_TAB_STRIP_HEIGHT)
      .flex()
      .items_center()
      .rounded_sm()
      .border_1()
      .border_color(self.theme.border.focus)
      .bg(self.theme.background.secondary)
      .text_color(self.theme.text.primary)
      .text_xs()
      .shadow_md()
      .child(div().min_w_0().truncate().child(self.title.clone()))
  }
}

/// Renders a binary panel tree.
///
/// The renderer owns neutral panel chrome: tab strips, active tab styling,
/// split panes, and resize handles. The caller owns concrete view rendering by
/// mapping active tabs to body elements.
///
/// # Parameters
///
/// `tree` is the panel tree to render.
///
/// `theme` supplies visual tokens for panel chrome.
///
/// `config` contains optional resize, tab activation, close, icon, and
/// tab-strip action behavior.
///
/// `render_body` renders the body for one active tab.
///
/// # Returns
///
/// A GPUI `Div` containing the rendered panel tree.
pub fn render_panel_container<T>(
  tree: &PanelTree<T>,
  theme: UIThemes,
  config: PanelContainerConfig,
  render_body: &dyn Fn(&PanelTab<T>) -> AnyElement,
) -> Div {
  let context = PanelRenderContext {
    theme,
    config: &config,
    render_body,
  };

  render_panel_node(&tree.root, &PanelSplitPath::root(), &context)
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
fn clamp_split_ratio(ratio: f32) -> f32 {
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
fn normalize_same_panel_insertion_index(
  source_index: usize,
  raw_insertion_index: usize,
  tab_count: usize,
) -> usize {
  let raw_insertion_index = raw_insertion_index.min(tab_count);
  let normalized_index = if source_index < raw_insertion_index {
    raw_insertion_index.saturating_sub(1)
  } else {
    raw_insertion_index
  };

  normalized_index.min(tab_count.saturating_sub(1))
}

/// Calculates an insertion position from actual rendered tab bounds.
///
/// Bounds must be supplied in visual order. The first tab whose midpoint lies
/// to the right of `pointer_x` determines the insertion position; a pointer
/// after every midpoint appends after the final tab.
///
/// # Parameters
///
/// `tab_bounds` contains actual logical-pixel bounds in strip order.
///
/// `pointer_x` is the pointer's horizontal logical-pixel coordinate.
///
/// # Returns
///
/// An insertion index from `0..=tab_bounds.len()`. Empty bounds return `0`.
pub fn panel_tab_insertion_index(tab_bounds: &[Bounds<Pixels>], pointer_x: Pixels) -> usize {
  tab_bounds
    .iter()
    .position(|bounds| pointer_x < bounds.origin.x + bounds.size.width / 2.0)
    .unwrap_or(tab_bounds.len())
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
    PanelNode::Split(split) => {
      find_leaf(&split.first, panel_id).or_else(|| find_leaf(&split.second, panel_id))
    }
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
    PanelNode::Split(split) => find_leaf_mut(&mut split.first, panel_id)
      .or_else(|| find_leaf_mut(&mut split.second, panel_id)),
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
fn find_split<'a, T>(
  node: &'a PanelNode<T>,
  path: &[PanelSplitBranch],
) -> Option<&'a PanelSplit<T>> {
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
    (PanelSplitAxis::Horizontal, PanelSplitBranch::First) => {
      (&split.first, px(f32::from(width) * first_ratio), height)
    }
    (PanelSplitAxis::Horizontal, PanelSplitBranch::Second) => {
      (&split.second, px(f32::from(width) * second_ratio), height)
    }
    (PanelSplitAxis::Vertical, PanelSplitBranch::First) => {
      (&split.first, width, px(f32::from(height) * first_ratio))
    }
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

      split_leaf_node(
        &mut split.second,
        panel_id,
        axis,
        new_leaf,
        placement,
        ratio,
      )
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

  remove_leaf_node(&mut split.first, panel_id)
    .or_else(|| remove_leaf_node(&mut split.second, panel_id))
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

/// Renders one panel tree node.
///
/// # Parameters
///
/// `node` is the current panel node to render.
///
/// `path` is the split path to `node`.
///
/// `context` contains theme tokens, callbacks, and active tab body rendering.
///
/// # Returns
///
/// A GPUI `Div` for the rendered node.
fn render_panel_node<T>(
  node: &PanelNode<T>,
  path: &PanelSplitPath,
  context: &PanelRenderContext<'_, T>,
) -> Div {
  match node {
    PanelNode::Leaf(leaf) => render_panel_leaf(leaf, context),
    PanelNode::Split(split) => render_panel_split(split, path, context),
  }
}

/// Renders one split node and its handle.
///
/// # Parameters
///
/// `split` is the split node to render.
///
/// `path` identifies `split` for resize callbacks.
///
/// `context` contains theme tokens, callbacks, and active tab body rendering.
///
/// # Returns
///
/// A GPUI `Div` for the split node.
fn render_panel_split<T>(
  split: &PanelSplit<T>,
  path: &PanelSplitPath,
  context: &PanelRenderContext<'_, T>,
) -> Div {
  let first = render_panel_node(&split.first, &path.child(PanelSplitBranch::First), context)
    .flex_basis(relative(split.ratio))
    .flex_shrink()
    .min_w_0()
    .min_h_0();

  let second = render_panel_node(
    &split.second,
    &path.child(PanelSplitBranch::Second),
    context,
  )
  .flex_basis(relative(1.0 - split.ratio))
  .flex_shrink()
  .min_w_0()
  .min_h_0();

  div()
    .flex()
    .size_full()
    .min_w_0()
    .min_h_0()
    .when(split.axis == PanelSplitAxis::Vertical, |panel| {
      panel.flex_col()
    })
    .child(first)
    .child(render_panel_split_handle(
      split.axis,
      path,
      context.theme,
      context.config.resize.as_ref(),
    ))
    .child(second)
}

/// Renders one split resize handle.
///
/// # Parameters
///
/// `axis` controls handle orientation and cursor.
///
/// `path` identifies the split for resize callbacks.
///
/// `theme` supplies visual tokens.
///
/// `resize` optionally enables resize-start behavior.
///
/// # Returns
///
/// A GPUI `Div` for the resize handle.
fn render_panel_split_handle(
  axis: PanelSplitAxis,
  path: &PanelSplitPath,
  theme: UIThemes,
  resize: Option<&PanelResizeConfig>,
) -> Div {
  let handle_size = resize
    .map(|resize| resize.handle_size)
    .unwrap_or(DEFAULT_PANEL_SPLIT_HANDLE_SIZE);
  let mut handle = div()
    .flex_none()
    .bg(theme.border.primary)
    .hover(move |style| style.bg(theme.border.focus));

  handle = match axis {
    PanelSplitAxis::Horizontal => handle
      .w(handle_size)
      .h_full()
      .cursor(CursorStyle::ResizeLeftRight),
    PanelSplitAxis::Vertical => handle
      .h(handle_size)
      .w_full()
      .cursor(CursorStyle::ResizeUpDown),
  };

  if let Some(resize) = resize {
    let path = path.clone();
    let on_resize_start = resize.on_resize_start.clone();
    handle = handle.on_mouse_down(MouseButton::Left, move |event, window, cx| {
      let cursor_position = match axis {
        PanelSplitAxis::Horizontal => event.position.x,
        PanelSplitAxis::Vertical => event.position.y,
      };
      on_resize_start(path.clone(), axis, cursor_position, window, cx);
    });
  }

  handle
}

/// Renders one leaf panel and its active tab body.
///
/// # Parameters
///
/// `leaf` is the leaf panel to render.
///
/// `context` contains theme tokens, callbacks, and active tab body rendering.
///
/// # Returns
///
/// A GPUI `Div` for the leaf panel.
fn render_panel_leaf<T>(leaf: &PanelLeaf<T>, context: &PanelRenderContext<'_, T>) -> Div {
  div()
    .flex()
    .flex_col()
    .size_full()
    .min_w_0()
    .min_h_0()
    .bg(context.theme.background.primary)
    .child(render_panel_tab_strip(leaf, context))
    .child(render_panel_body(leaf, context.theme, context.render_body))
}

/// Renders the tab strip for one leaf panel.
///
/// # Parameters
///
/// `leaf` is the leaf panel whose tabs are rendered.
///
/// `context` contains theme tokens, callbacks, and optional tab-strip actions.
///
/// # Returns
///
/// A GPUI `Div` containing tab buttons.
fn render_panel_tab_strip<T>(leaf: &PanelLeaf<T>, context: &PanelRenderContext<'_, T>) -> Div {
  let panel_focused = context.config.focused_panel_id == Some(leaf.id);
  let active_drop_target = context
    .config
    .tab_drag
    .as_ref()
    .and_then(|config| config.state.as_ref())
    .and_then(|state| state.drop_target)
    .filter(|target| target.panel_id == leaf.id);
  let mut tab_lane = div()
    .relative()
    .flex()
    .items_center()
    .flex_1()
    .min_w_0()
    .h_full()
    .overflow_hidden()
    .children(leaf.tabs.iter().enumerate().map(|(index, tab)| {
      render_panel_tab(
        leaf.id,
        index,
        tab,
        panel_focused,
        leaf.active_tab == Some(tab.id),
        context,
      )
    }))
    .when(
      active_drop_target.is_some_and(|target| target.insertion_index >= leaf.tabs.len())
        && !leaf.tabs.is_empty(),
      |lane| {
        lane.child(render_panel_tab_insertion_marker(
          context.theme.border.focus.into(),
        ))
      },
    )
    .when(
      active_drop_target.is_some() && leaf.tabs.is_empty(),
      |lane| {
        lane.child(
          div()
            .absolute()
            .inset_0()
            .border_1()
            .border_color(context.theme.border.focus),
        )
      },
    );

  if let Some(tab_drag) = context.config.tab_drag.as_ref() {
    let on_target = tab_drag.on_target.clone();
    let on_drop = tab_drag.on_drop.clone();
    let panel_id = leaf.id;
    let insertion_index = leaf.tabs.len();
    tab_lane = tab_lane
      .on_drag_move::<PanelTabDrag>(move |event, window, cx| {
        if event.bounds.contains(&event.event.position) {
          on_target(
            PanelTabDropTarget {
              panel_id,
              insertion_index,
            },
            window,
            cx,
          );
        }
      })
      .on_drop(move |drag: &PanelTabDrag, window, cx| {
        on_drop(drag.clone(), panel_id, window, cx);
      });
  }

  let mut tab_strip = div()
    .flex()
    .items_center()
    .h(DEFAULT_PANEL_TAB_STRIP_HEIGHT)
    .min_w_0()
    .overflow_hidden()
    .bg(context.theme.background.primary)
    .child(tab_lane);

  if let Some(render_tab_strip_actions) = context.config.render_tab_strip_actions.as_ref()
    && panel_focused
  {
    tab_strip = tab_strip.child(render_tab_strip_actions(leaf.id));
  }

  tab_strip
}

/// Renders one tab button.
///
/// # Parameters
///
/// `panel_id` identifies the leaf panel containing the tab.
///
/// `tab` is the tab metadata to render.
///
/// `active` controls active tab styling.
///
/// `context` contains theme tokens, callbacks, and optional icon rendering.
///
/// # Returns
///
/// A GPUI `Div` for the tab button.
fn render_panel_tab<T>(
  panel_id: PanelId,
  tab_index: usize,
  tab: &PanelTab<T>,
  panel_focused: bool,
  active: bool,
  context: &PanelRenderContext<'_, T>,
) -> impl IntoElement {
  let tab_id = tab.id;
  let tab_hover_group =
    SharedString::from(format!("panel-tab-{}-{}", panel_id.value(), tab_id.value()));
  let theme = context.theme;
  let drag_state = context
    .config
    .tab_drag
    .as_ref()
    .and_then(|config| config.state.as_ref());
  let dragged = drag_state
    .is_some_and(|state| state.drag.source_panel_id == panel_id && state.drag.tab_id == tab_id);
  let insertion_before = drag_state
    .and_then(|state| state.drop_target)
    .is_some_and(|target| target.panel_id == panel_id && target.insertion_index == tab_index);
  let mut tab_element = div()
    .id(tab_hover_group.clone())
    .relative()
    .group(tab_hover_group.clone())
    .flex()
    .items_center()
    .h_full()
    .max_w(px(220.0))
    .min_w(px(72.0))
    .border_r_1()
    .border_color(theme.border.primary)
    .text_xs()
    .cursor_pointer()
    .text_color(if active {
      theme.text.primary
    } else {
      theme.text.secondary
    })
    .bg(if active {
      theme.background.secondary
    } else {
      theme.background.primary
    })
    .hover(move |style| style.bg(theme.background.secondary))
    .when(dragged, |tab| tab.opacity(0.55))
    .child(
      div()
        .flex()
        .items_center()
        .flex_1()
        .min_w_0()
        .h_full()
        .pl_3()
        .child(div().min_w_0().truncate().child(tab.title.clone())),
    )
    .child(
      div()
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .h_full()
        .w(DEFAULT_PANEL_TAB_TRAILING_ACTION_WIDTH)
        .child(render_panel_tab_close_button(
          panel_id,
          tab_id,
          active,
          tab_hover_group,
          context,
        )),
    );

  if insertion_before {
    tab_element = tab_element.child(render_panel_tab_insertion_marker(theme.border.focus.into()));
  }

  if panel_focused && active {
    tab_element = tab_element.child(
      div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(1.0))
        .bg(theme.border.focus),
    );
  }

  if let Some(tab_drag) = context.config.tab_drag.as_ref() {
    let drag = PanelTabDrag {
      source_panel_id: panel_id,
      tab_id,
      source_index: tab_index,
      title: tab.title.clone(),
    };
    let on_start = tab_drag.on_start.clone();
    tab_element = tab_element
      .cursor_move()
      .on_drag(drag, move |drag, _, window, cx| {
        on_start(drag.clone(), window, cx);
        cx.new(|_| PanelTabDragPreview {
          title: drag.title.clone(),
          theme,
        })
      });
  }

  if let Some(tab_drag) = context.config.tab_drag.as_ref() {
    let on_target = tab_drag.on_target.clone();
    tab_element = tab_element.on_drag_move::<PanelTabDrag>(move |event, window, cx| {
      if !event.bounds.contains(&event.event.position) {
        return;
      }

      let local_index =
        panel_tab_insertion_index(std::slice::from_ref(&event.bounds), event.event.position.x);
      on_target(
        PanelTabDropTarget {
          panel_id,
          insertion_index: tab_index + local_index,
        },
        window,
        cx,
      );
    });
  }

  if let Some(on_activate_tab) = context.config.on_activate_tab.as_ref() {
    let on_activate_tab = on_activate_tab.clone();
    tab_element = tab_element.on_click(move |_, window, cx| {
      on_activate_tab(panel_id, tab_id, window, cx);
    });
  }

  tab_element
}

/// Renders an overlay marker at one tab insertion boundary.
///
/// # Parameters
///
/// `color` is the theme color painted by the insertion marker.
///
/// # Returns
///
/// A zero-width relative element whose absolute child paints a two-pixel
/// vertical marker without changing tab-strip geometry.
fn render_panel_tab_insertion_marker(color: gpui::Hsla) -> Div {
  div().relative().w_0().h_full().child(
    div()
      .absolute()
      .left_0()
      .top_0()
      .bottom_0()
      .w(px(2.0))
      .bg(color),
  )
}

/// Renders the reserved close button slot for one panel tab.
///
/// The slot is always present so tab titles keep a stable width. Inactive tabs
/// start transparent and become visible when their tab hover group is hovered;
/// active tabs keep the close affordance visible.
///
/// # Parameters
///
/// `panel_id` identifies the leaf panel that owns the tab.
///
/// `tab_id` identifies the tab that should close when the button is pressed.
///
/// `active` controls whether the close glyph is visible by default.
///
/// `tab_hover_group` identifies the GPUI hover group shared with the tab.
///
/// `context` contains theme tokens, close callbacks, and optional icon
/// rendering.
///
/// # Returns
///
/// A GPUI `Div` containing the close button slot.
fn render_panel_tab_close_button<T>(
  panel_id: PanelId,
  tab_id: PanelTabId,
  active: bool,
  tab_hover_group: SharedString,
  context: &PanelRenderContext<'_, T>,
) -> Div {
  let theme = context.theme;
  let close_icon = context
    .config
    .render_tab_close_icon
    .as_ref()
    .map(|render_icon| render_icon(theme))
    .unwrap_or_else(|| div().text_xs().child("x").into_any_element());
  let mut close_button = div()
    .flex()
    .items_center()
    .justify_center()
    .flex_none()
    .size(DEFAULT_PANEL_TAB_CLOSE_BUTTON_SIZE)
    .rounded_sm()
    .text_color(if active {
      theme.text.primary
    } else {
      theme.text.secondary
    })
    .opacity(if active { 1.0 } else { 0.0 })
    .group_hover(tab_hover_group, |style| style.opacity(1.0))
    .hover(move |style| {
      style
        .bg(theme.background.hover)
        .text_color(theme.text.primary)
    })
    .child(close_icon);

  if let Some(on_close_tab) = context.config.on_close_tab.as_ref() {
    let on_close_tab = on_close_tab.clone();
    close_button = close_button
      .cursor_pointer()
      .on_mouse_down(MouseButton::Left, |_, _, cx| {
        cx.stop_propagation();
      })
      .on_mouse_up(MouseButton::Left, move |_, window, cx| {
        cx.stop_propagation();
        on_close_tab(panel_id, tab_id, window, cx);
      });
  }

  close_button
}

/// Renders the body region for one leaf panel.
///
/// # Parameters
///
/// `leaf` is the leaf panel whose active tab body should render.
///
/// `theme` supplies visual tokens.
///
/// `render_body` renders active tab content.
///
/// # Returns
///
/// A GPUI `Div` for the panel body.
fn render_panel_body<T>(
  leaf: &PanelLeaf<T>,
  theme: UIThemes,
  render_body: &dyn Fn(&PanelTab<T>) -> AnyElement,
) -> Div {
  let body = div()
    .relative()
    .flex()
    .flex_col()
    .flex_1()
    .min_h_0()
    .min_w_0()
    .bg(theme.background.secondary)
    .child(
      // here we use an ui trick, using a child gap at the top of body to fill
      // the gap
      div()
        .absolute()
        .left_0()
        .right_0()
        .top(px(-1.0))
        .h(px(2.0))
        .bg(theme.background.secondary),
    );

  match leaf.active_tab() {
    Some(tab) => body.child(render_body(tab)),
    None => body.child(
      div()
        .flex()
        .items_center()
        .justify_center()
        .size_full()
        .text_sm()
        .text_color(theme.text.secondary)
        .child("No tabs"),
    ),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Creates a root panel containing four stable test tabs.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A [`PanelTree`] containing tabs A through D in one root leaf.
  fn four_tab_tree() -> PanelTree<&'static str> {
    PanelTree::single_leaf(
      PanelLeaf::new(PanelId::new(1))
        .tab(PanelTab::new(PanelTabId::new(1), "A", "state-a"))
        .tab(PanelTab::new(PanelTabId::new(2), "B", "state-b"))
        .tab(PanelTab::new(PanelTabId::new(3), "C", "state-c"))
        .tab(PanelTab::new(PanelTabId::new(4), "D", "state-d")),
    )
  }

  /// Returns tab identifiers from a requested test panel.
  ///
  /// # Parameters
  ///
  /// `tree` is the test panel tree to inspect.
  ///
  /// `panel_id` identifies the leaf whose tab order should be returned.
  ///
  /// # Returns
  ///
  /// Tab identifiers in their current visual order, or an empty vector when
  /// the panel does not exist.
  fn tab_ids(tree: &PanelTree<&'static str>, panel_id: PanelId) -> Vec<PanelTabId> {
    tree
      .leaf(panel_id)
      .map(|leaf| leaf.tabs.iter().map(|tab| tab.id).collect())
      .unwrap_or_default()
  }

  /// Verifies that a leaf activates its first inserted tab.
  #[test]
  fn panel_leaf_should_activate_first_inserted_tab() {
    let leaf = PanelLeaf::new(PanelId::new(1)).tab(PanelTab::new(PanelTabId::new(7), "Code", ()));

    assert_eq!(leaf.active_tab, Some(PanelTabId::new(7)));
  }

  /// Verifies that tab activation is scoped to the requested panel.
  #[test]
  fn panel_tree_should_activate_tab_in_target_leaf() {
    let mut tree = PanelTree::single_leaf(
      PanelLeaf::new(PanelId::new(1))
        .tab(PanelTab::new(PanelTabId::new(1), "Editor", ()))
        .tab(PanelTab::new(PanelTabId::new(2), "Preview", ())),
    );

    assert!(tree.activate_tab(PanelId::new(1), PanelTabId::new(2)));
    assert!(matches!(
      tree.root,
      PanelNode::Leaf(PanelLeaf {
        active_tab: Some(PanelTabId(2)),
        ..
      })
    ));
  }

  /// Verifies that closing the active tab activates the next available tab.
  #[test]
  fn panel_tree_should_activate_next_tab_when_closing_active_tab() {
    let mut tree = PanelTree::single_leaf(
      PanelLeaf::new(PanelId::new(1))
        .tab(PanelTab::new(PanelTabId::new(1), "Editor", ()))
        .tab(PanelTab::new(PanelTabId::new(2), "Preview", ()))
        .tab(PanelTab::new(PanelTabId::new(3), "Structure", ())),
    );
    assert!(tree.activate_tab(PanelId::new(1), PanelTabId::new(2)));

    assert!(tree.close_tab(PanelId::new(1), PanelTabId::new(2)));

    let Some(leaf) = tree.leaf(PanelId::new(1)) else {
      panic!("leaf should exist");
    };
    assert_eq!(leaf.active_tab, Some(PanelTabId::new(3)));
  }

  /// Verifies that closing an inactive tab preserves the active tab.
  #[test]
  fn panel_tree_should_preserve_active_tab_when_closing_inactive_tab() {
    let mut tree = PanelTree::single_leaf(
      PanelLeaf::new(PanelId::new(1))
        .tab(PanelTab::new(PanelTabId::new(1), "Editor", ()))
        .tab(PanelTab::new(PanelTabId::new(2), "Preview", ())),
    );

    assert!(tree.close_tab(PanelId::new(1), PanelTabId::new(2)));

    let Some(leaf) = tree.leaf(PanelId::new(1)) else {
      panic!("leaf should exist");
    };
    assert_eq!(leaf.active_tab, Some(PanelTabId::new(1)));
  }

  /// Verifies moving a middle tab to the left uses the raw strip position.
  #[test]
  fn panel_tree_should_move_middle_tab_left() {
    let mut tree = four_tab_tree();

    assert!(tree.move_tab(
      PanelId::new(1),
      PanelTabId::new(3),
      PanelTabDropTarget {
        panel_id: PanelId::new(1),
        insertion_index: 1,
      },
    ));

    assert_eq!(
      tab_ids(&tree, PanelId::new(1)),
      vec![
        PanelTabId::new(1),
        PanelTabId::new(3),
        PanelTabId::new(2),
        PanelTabId::new(4)
      ]
    );
  }

  /// Verifies moving a middle tab right normalizes its post-removal index.
  #[test]
  fn panel_tree_should_move_middle_tab_right() {
    let mut tree = four_tab_tree();

    assert!(tree.move_tab(
      PanelId::new(1),
      PanelTabId::new(2),
      PanelTabDropTarget {
        panel_id: PanelId::new(1),
        insertion_index: 4,
      },
    ));

    assert_eq!(
      tab_ids(&tree, PanelId::new(1)),
      vec![
        PanelTabId::new(1),
        PanelTabId::new(3),
        PanelTabId::new(4),
        PanelTabId::new(2)
      ]
    );
  }

  /// Verifies the first tab can move to the visual end of its strip.
  #[test]
  fn panel_tree_should_move_first_tab_to_end() {
    let mut tree = four_tab_tree();

    assert!(tree.move_tab(
      PanelId::new(1),
      PanelTabId::new(1),
      PanelTabDropTarget {
        panel_id: PanelId::new(1),
        insertion_index: 4,
      },
    ));

    assert_eq!(
      tab_ids(&tree, PanelId::new(1)),
      vec![
        PanelTabId::new(2),
        PanelTabId::new(3),
        PanelTabId::new(4),
        PanelTabId::new(1)
      ]
    );
  }

  /// Verifies the final tab can move to the beginning.
  #[test]
  fn panel_tree_should_move_last_tab_to_beginning() {
    let mut tree = four_tab_tree();

    assert!(tree.move_tab(
      PanelId::new(1),
      PanelTabId::new(4),
      PanelTabDropTarget {
        panel_id: PanelId::new(1),
        insertion_index: 0,
      },
    ));

    assert_eq!(
      tab_ids(&tree, PanelId::new(1)),
      vec![
        PanelTabId::new(4),
        PanelTabId::new(1),
        PanelTabId::new(2),
        PanelTabId::new(3)
      ]
    );
  }

  /// Verifies an effective original-position drop does not change order.
  #[test]
  fn panel_tree_should_keep_order_for_effective_same_position() {
    let mut tree = four_tab_tree();

    assert!(tree.move_tab(
      PanelId::new(1),
      PanelTabId::new(2),
      PanelTabDropTarget {
        panel_id: PanelId::new(1),
        insertion_index: 2,
      },
    ));

    assert_eq!(
      tab_ids(&tree, PanelId::new(1)),
      vec![
        PanelTabId::new(1),
        PanelTabId::new(2),
        PanelTabId::new(3),
        PanelTabId::new(4)
      ]
    );
  }

  /// Verifies dropping immediately before a tab's original position is a no-op.
  #[test]
  fn panel_tree_should_keep_order_when_dropped_before_original_position() {
    let mut tree = four_tab_tree();

    assert!(tree.move_tab(
      PanelId::new(1),
      PanelTabId::new(2),
      PanelTabDropTarget {
        panel_id: PanelId::new(1),
        insertion_index: 1,
      },
    ));

    assert_eq!(
      tab_ids(&tree, PanelId::new(1)),
      vec![
        PanelTabId::new(1),
        PanelTabId::new(2),
        PanelTabId::new(3),
        PanelTabId::new(4)
      ]
    );
  }

  /// Verifies a same-panel move leaves the moved tab active.
  #[test]
  fn panel_tree_should_activate_reordered_tab() {
    let mut tree = four_tab_tree();

    assert!(tree.move_tab(
      PanelId::new(1),
      PanelTabId::new(3),
      PanelTabDropTarget {
        panel_id: PanelId::new(1),
        insertion_index: 0,
      },
    ));

    assert_eq!(
      tree.leaf(PanelId::new(1)).and_then(|leaf| leaf.active_tab),
      Some(PanelTabId::new(3))
    );
  }

  /// Verifies cross-panel movement transfers one owned tab and repairs source focus.
  #[test]
  fn panel_tree_should_move_active_tab_between_panels() {
    let mut tree = four_tab_tree();
    assert!(tree.activate_tab(PanelId::new(1), PanelTabId::new(2)));
    assert!(
      tree.split_leaf(
        PanelId::new(1),
        PanelSplitAxis::Horizontal,
        PanelLeaf::new(PanelId::new(2))
          .tab(PanelTab::new(PanelTabId::new(5), "E", "state-e"))
          .tab(PanelTab::new(PanelTabId::new(6), "F", "state-f")),
        PanelSplitPlacement::After,
        0.5,
      )
    );

    assert!(tree.move_tab(
      PanelId::new(1),
      PanelTabId::new(2),
      PanelTabDropTarget {
        panel_id: PanelId::new(2),
        insertion_index: 1,
      },
    ));

    assert_eq!(
      tab_ids(&tree, PanelId::new(2)),
      vec![PanelTabId::new(5), PanelTabId::new(2), PanelTabId::new(6)]
    );
    assert_eq!(
      tree.leaf(PanelId::new(1)).and_then(|leaf| leaf.active_tab),
      Some(PanelTabId::new(3))
    );
    assert_eq!(
      tree.leaf(PanelId::new(2)).and_then(|leaf| leaf.active_tab),
      Some(PanelTabId::new(2))
    );
    assert_eq!(
      tree
        .leaf(PanelId::new(2))
        .and_then(|leaf| leaf.active_tab())
        .map(|tab| tab.payload),
      Some("state-b")
    );
  }

  /// Verifies an inactive move preserves the source panel's active tab.
  #[test]
  fn panel_tree_should_preserve_source_active_tab_when_moving_inactive_tab() {
    let mut tree = four_tab_tree();
    assert!(tree.split_leaf(
      PanelId::new(1),
      PanelSplitAxis::Horizontal,
      PanelLeaf::new(PanelId::new(2)),
      PanelSplitPlacement::After,
      0.5,
    ));

    assert!(tree.move_tab(
      PanelId::new(1),
      PanelTabId::new(3),
      PanelTabDropTarget {
        panel_id: PanelId::new(2),
        insertion_index: 0,
      },
    ));

    assert_eq!(
      tree.leaf(PanelId::new(1)).and_then(|leaf| leaf.active_tab),
      Some(PanelTabId::new(1))
    );
  }

  /// Verifies moving into an empty panel activates its only tab.
  #[test]
  fn panel_tree_should_move_tab_into_empty_panel() {
    let mut tree = four_tab_tree();
    assert!(tree.split_leaf(
      PanelId::new(1),
      PanelSplitAxis::Horizontal,
      PanelLeaf::new(PanelId::new(2)),
      PanelSplitPlacement::After,
      0.5,
    ));

    assert!(tree.move_tab(
      PanelId::new(1),
      PanelTabId::new(4),
      PanelTabDropTarget {
        panel_id: PanelId::new(2),
        insertion_index: 0,
      },
    ));

    let Some(target) = tree.leaf(PanelId::new(2)) else {
      panic!("empty target panel should survive");
    };
    assert_eq!(target.tabs.len(), 1);
    assert_eq!(target.active_tab, Some(PanelTabId::new(4)));
  }

  /// Verifies moving a final source tab collapses its split around the target.
  #[test]
  fn panel_tree_should_remove_empty_source_and_preserve_adjacent_target() {
    let mut tree = PanelTree::single_leaf(PanelLeaf::new(PanelId::new(1)).tab(PanelTab::new(
      PanelTabId::new(1),
      "A",
      "state-a",
    )));
    assert!(tree.split_leaf(
      PanelId::new(1),
      PanelSplitAxis::Horizontal,
      PanelLeaf::new(PanelId::new(2)).tab(PanelTab::new(PanelTabId::new(2), "B", "state-b",)),
      PanelSplitPlacement::After,
      0.5,
    ));

    assert!(tree.move_tab(
      PanelId::new(1),
      PanelTabId::new(1),
      PanelTabDropTarget {
        panel_id: PanelId::new(2),
        insertion_index: 1,
      },
    ));

    assert_eq!(tree.leaf_count(), 1);
    assert!(tree.leaf(PanelId::new(1)).is_none());
    assert_eq!(
      tab_ids(&tree, PanelId::new(2)),
      vec![PanelTabId::new(2), PanelTabId::new(1)]
    );
  }

  /// Verifies duplicate target identities reject a move without mutation.
  #[test]
  fn panel_tree_should_reject_duplicate_target_tab_id_atomically() {
    let mut tree = PanelTree::single_leaf(PanelLeaf::new(PanelId::new(1)).tab(PanelTab::new(
      PanelTabId::new(1),
      "A",
      (),
    )));
    assert!(tree.split_leaf(
      PanelId::new(1),
      PanelSplitAxis::Horizontal,
      PanelLeaf::new(PanelId::new(2)).tab(PanelTab::new(PanelTabId::new(1), "B", ())),
      PanelSplitPlacement::After,
      0.5,
    ));
    let before = tree.clone();

    assert!(!tree.move_tab(
      PanelId::new(1),
      PanelTabId::new(1),
      PanelTabDropTarget {
        panel_id: PanelId::new(2),
        insertion_index: 0,
      },
    ));

    assert_eq!(tree, before);
  }

  /// Verifies a stale target panel rejects a move without changing the tree.
  #[test]
  fn panel_tree_should_reject_stale_target_atomically() {
    let mut tree = four_tab_tree();
    let before = tree.clone();

    assert!(!tree.move_tab(
      PanelId::new(1),
      PanelTabId::new(2),
      PanelTabDropTarget {
        panel_id: PanelId::new(99),
        insertion_index: 0,
      },
    ));

    assert_eq!(tree, before);
  }

  /// Verifies midpoint hit testing handles variable rendered tab widths.
  #[test]
  fn panel_tab_insertion_index_should_use_actual_variable_width_bounds() {
    let bounds = [
      Bounds {
        origin: gpui::point(px(10.0), px(0.0)),
        size: gpui::size(px(80.0), px(34.0)),
      },
      Bounds {
        origin: gpui::point(px(90.0), px(0.0)),
        size: gpui::size(px(200.0), px(34.0)),
      },
      Bounds {
        origin: gpui::point(px(290.0), px(0.0)),
        size: gpui::size(px(72.0), px(34.0)),
      },
    ];

    assert_eq!(panel_tab_insertion_index(&bounds, px(49.0)), 0);
    assert_eq!(panel_tab_insertion_index(&bounds, px(50.0)), 1);
    assert_eq!(panel_tab_insertion_index(&bounds, px(189.0)), 1);
    assert_eq!(panel_tab_insertion_index(&bounds, px(190.0)), 2);
    assert_eq!(panel_tab_insertion_index(&bounds, px(325.0)), 2);
    assert_eq!(panel_tab_insertion_index(&bounds, px(326.0)), 3);
    assert_eq!(panel_tab_insertion_index(&[], px(100.0)), 0);
  }

  /// Verifies that immutable leaf lookup returns the requested panel.
  #[test]
  fn panel_tree_should_find_leaf_by_id() {
    let tree = PanelTree::single_leaf(PanelLeaf::new(PanelId::new(1)).tab(PanelTab::new(
      PanelTabId::new(1),
      "Editor",
      (),
    )));

    let Some(leaf) = tree.leaf(PanelId::new(1)) else {
      panic!("leaf should exist");
    };

    assert_eq!(leaf.id, PanelId::new(1));
  }

  /// Verifies that mutable leaf lookup returns the requested panel.
  #[test]
  fn panel_tree_should_find_mutable_leaf_by_id() {
    let mut tree = PanelTree::single_leaf(PanelLeaf::new(PanelId::new(1)));

    let Some(leaf) = tree.leaf_mut(PanelId::new(1)) else {
      panic!("leaf should exist");
    };
    leaf.add_tab(PanelTab::new(PanelTabId::new(1), "Editor", ()));

    assert_eq!(
      tree.leaf(PanelId::new(1)).map(|leaf| leaf.tabs.len()),
      Some(1)
    );
  }

  /// Verifies that splitting a leaf creates a binary split node.
  #[test]
  fn panel_tree_should_split_leaf_into_binary_node() {
    let mut tree = PanelTree::single_leaf(PanelLeaf::new(PanelId::new(1)).tab(PanelTab::new(
      PanelTabId::new(1),
      "Editor",
      (),
    )));

    assert!(tree.split_leaf(
      PanelId::new(1),
      PanelSplitAxis::Horizontal,
      PanelLeaf::new(PanelId::new(2)).tab(PanelTab::new(PanelTabId::new(2), "Preview", ())),
      PanelSplitPlacement::After,
      0.65,
    ));
    assert_eq!(tree.leaf_count(), 2);
  }

  /// Verifies that removing a split leaf promotes its sibling.
  #[test]
  fn panel_tree_should_collapse_split_when_removing_leaf() {
    let mut tree = PanelTree {
      root: PanelNode::Split(PanelSplit::new(
        PanelSplitAxis::Horizontal,
        0.5,
        PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(1))),
        PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(2))),
      )),
    };

    assert_eq!(tree.remove_leaf(PanelId::new(2)), Some(PanelId::new(1)));

    assert_eq!(tree.leaf_count(), 1);
    assert!(matches!(
      tree.root,
      PanelNode::Leaf(PanelLeaf { id: PanelId(1), .. })
    ));
  }

  /// Verifies that removing a leaf can promote an entire sibling subtree.
  #[test]
  fn panel_tree_should_promote_sibling_subtree_when_removing_leaf() {
    let mut tree = PanelTree {
      root: PanelNode::Split(PanelSplit::new(
        PanelSplitAxis::Horizontal,
        0.5,
        PanelNode::Split(PanelSplit::new(
          PanelSplitAxis::Vertical,
          0.5,
          PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(1))),
          PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(2))),
        )),
        PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(3))),
      )),
    };

    assert_eq!(tree.remove_leaf(PanelId::new(3)), Some(PanelId::new(1)));

    assert_eq!(tree.leaf_count(), 2);
    assert!(tree.leaf(PanelId::new(1)).is_some());
    assert!(tree.leaf(PanelId::new(2)).is_some());
    assert!(tree.leaf(PanelId::new(3)).is_none());
  }

  /// Verifies that the root leaf is not removable.
  #[test]
  fn panel_tree_should_not_remove_only_root_leaf() {
    let mut tree = PanelTree::single_leaf(PanelLeaf::<()>::new(PanelId::new(1)));

    assert_eq!(tree.remove_leaf(PanelId::new(1)), None);

    assert_eq!(tree.leaf_count(), 1);
    assert!(tree.leaf(PanelId::new(1)).is_some());
  }

  /// Verifies that split resizing clamps ratios to supported bounds.
  #[test]
  fn panel_tree_should_clamp_split_resize_ratio() {
    let mut tree = PanelTree {
      root: PanelNode::Split(PanelSplit::new(
        PanelSplitAxis::Horizontal,
        0.5,
        PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(1))),
        PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(2))),
      )),
    };

    assert!(tree.resize_split(&PanelSplitPath::root(), 2.0));
    assert!(matches!(
      tree.root,
      PanelNode::Split(PanelSplit {
        ratio: MAX_PANEL_SPLIT_RATIO,
        ..
      })
    ));
  }

  /// Verifies that split ratio lookup returns the target split ratio.
  #[test]
  fn panel_tree_should_return_split_ratio_by_path() {
    let tree = PanelTree {
      root: PanelNode::Split(PanelSplit::new(
        PanelSplitAxis::Horizontal,
        0.35,
        PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(1))),
        PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(2))),
      )),
    };

    assert_eq!(tree.split_ratio(&PanelSplitPath::root()), Some(0.35));
  }

  /// Verifies that nested split sizing follows split ratios.
  #[test]
  fn panel_tree_should_return_nested_split_axis_size() {
    let tree = PanelTree {
      root: PanelNode::Split(PanelSplit::new(
        PanelSplitAxis::Horizontal,
        0.25,
        PanelNode::Split(PanelSplit::new(
          PanelSplitAxis::Vertical,
          0.5,
          PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(1))),
          PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(2))),
        )),
        PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(3))),
      )),
    };

    assert_eq!(
      tree.split_axis_size(
        &PanelSplitPath::root().child(PanelSplitBranch::First),
        px(800.0),
        px(600.0),
      ),
      Some(px(600.0))
    );
  }
}
