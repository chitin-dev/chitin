//! Panel container data model.

use gpui::SharedString;

use super::layout::clamp_split_ratio;

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
    Self { branches: Vec::new() }
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
