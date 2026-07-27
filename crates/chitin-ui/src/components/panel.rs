//! Reusable multi-panel container primitives.
//!
//! IDE docking layouts are commonly represented as a binary tree: internal
//! nodes are splits, and leaf nodes are tab stacks. This module provides that
//! model plus a lightweight GPUI renderer for panel chrome. Application crates
//! keep concrete view rendering and command wiring outside `chitin-ui`.

use std::rc::Rc;

use gpui::{
  AnyElement, App, CursorStyle, Div, InteractiveElement, MouseButton, ParentElement, Pixels,
  SharedString, Styled, Window, div, prelude::*, px, relative,
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

/// Stable identifier for a tab inside a panel.
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
    let Some(index) = self.tabs.iter().position(|tab| tab.id == tab_id) else {
      return false;
    };
    let closed_was_active = self.active_tab == Some(tab_id);

    self.tabs.remove(index);

    if closed_was_active {
      self.active_tab = self
        .tabs
        .get(index)
        .or_else(|| index.checked_sub(1).and_then(|index| self.tabs.get(index)))
        .map(|tab| tab.id);
    }

    true
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

/// Optional chrome behavior for [`render_panel_container`].
#[derive(Clone, Default)]
pub struct PanelContainerConfig {
  /// Optional resize configuration used by split handles.
  resize: Option<PanelResizeConfig>,
  /// Optional callback invoked when a tab is activated.
  on_activate_tab: Option<Rc<PanelTabActivateHandler>>,
  /// Optional callback invoked when a tab close button is pressed.
  on_close_tab: Option<Rc<PanelTabCloseHandler>>,
  /// Optional renderer for the tab close icon.
  render_tab_close_icon: Option<Rc<PanelTabCloseIconRenderer>>,
  /// Optional renderer for caller-owned tab-strip actions.
  render_tab_strip_actions: Option<Rc<PanelTabStripActionsRenderer>>,
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
  let mut tab_strip = div()
    .flex()
    .items_center()
    .h(DEFAULT_PANEL_TAB_STRIP_HEIGHT)
    .min_w_0()
    .overflow_hidden()
    .bg(context.theme.background.primary)
    .child(
      div()
        .flex()
        .items_center()
        .flex_1()
        .min_w_0()
        .h_full()
        .overflow_hidden()
        .children(
          leaf
            .tabs
            .iter()
            .map(|tab| render_panel_tab(leaf.id, tab, leaf.active_tab == Some(tab.id), context)),
        ),
    );

  if let Some(render_tab_strip_actions) = context.config.render_tab_strip_actions.as_ref() {
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
  tab: &PanelTab<T>,
  active: bool,
  context: &PanelRenderContext<'_, T>,
) -> Div {
  let tab_id = tab.id;
  let tab_hover_group =
    SharedString::from(format!("panel-tab-{}-{}", panel_id.value(), tab_id.value()));
  let theme = context.theme;
  let mut tab_element = div()
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

  if active {
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

  if let Some(on_activate_tab) = context.config.on_activate_tab.as_ref() {
    let on_activate_tab = on_activate_tab.clone();
    tab_element = tab_element.on_mouse_up(MouseButton::Left, move |_, window, cx| {
      on_activate_tab(panel_id, tab_id, window, cx);
    });
  }

  tab_element
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
    close_button =
      close_button
        .cursor_pointer()
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
    .flex()
    .flex_col()
    .flex_1()
    .min_h_0()
    .min_w_0()
    .bg(theme.background.secondary);

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
