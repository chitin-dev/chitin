//! Reusable tree and tree items building blocks.
//!
//! Tree and tree items should be generic to represent different tree-structure
//! data like workspace, json etc. So every function and property in this component
//! should not be related to behaviors of specific tree-structure data.

use std::rc::Rc;

use gpui::{
  App, InteractiveElement, IntoElement, MouseButton, ParentElement, SharedString, Styled, Window,
  div, px,
};

use crate::themes::{UIThemes, builtins};

/// Default indent of tree items between levels
pub const DEFAULT_TREE_INDENT: f32 = 12.0;
/// Default height of tree items
pub const DEFAULT_TREE_ROW_HEIGHT: gpui::Pixels = px(24.0);

type TreeItemClickListener = Rc<dyn Fn(&TreeItemClickEvent, &mut Window, &mut App)>;

/// Event emitted when a tree row is clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeItemClickEvent {
  /// Stable id of the clicked tree item.
  pub id: SharedString,
}

/// Semantic kind for a tree item.
///
/// Semantic token of tree item enumeration still should be abstract. So we use
/// Node and Leaf to distinguish them
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeItemKind {
  /// An item that can contain child items.
  ///
  /// Nodes are typically expandable/collapsible in the tree view and may
  /// have nested children. They often represent directories, containers,
  /// or grouping elements.
  Node,
  /// An item that cannot contain child items.
  ///
  /// Leaves are terminal items in the tree hierarchy. They represent
  /// individual files, entries, or actionable elements that cannot be
  /// expanded further.
  Leaf,
}

/// Immutable input model for a tree row.
///
/// `TreeItem` is owned UI state, not a filesystem model. This keeps `chitin-ui`
/// independent from `chitin-core`, while desktop crates can still adapt
/// `ProjectTreeEntry` or any other hierarchical data into a reusable tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeItem {
  /// Unique identifier for this tree item.
  ///
  /// Used for item lookup, equality comparison, and state tracking across
  /// tree updates. Should remain stable across re-renders.
  id: SharedString,
  /// Display text shown in the tree view.
  label: SharedString,
  /// Semantic classification of the item as either a node or leaf.
  kind: TreeItemKind,
  /// Child items contained by this node.
  ///
  /// Only relevant when `kind` is `TreeItemKind::Node`. Leaf items typically
  /// have an empty vector.
  children: Vec<TreeItem>,
  /// Whether this node's children are visible in the tree view.
  ///
  /// Only meaningful for nodes with children. Determines if the expand/collapse
  /// toggle is shown and whether descendants are rendered.
  expanded: bool,
}

impl TreeItem {
  /// Creates a tree item with no children.
  ///
  /// Items are collapsed by default so large trees can render cheaply before
  /// the caller wires persisted expansion state.
  pub fn new(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    kind: TreeItemKind,
  ) -> Self {
    Self {
      id: id.into(),
      label: label.into(),
      kind,
      children: Vec::new(),
      expanded: false,
    }
  }

  /// Replaces this item's children.
  pub fn children(mut self, children: impl IntoIterator<Item = TreeItem>) -> Self {
    self.children = children.into_iter().collect();
    self
  }

  /// Sets whether the item's children should be rendered.
  pub fn expanded(mut self, expanded: bool) -> Self {
    self.expanded = expanded;
    self
  }

  /// Returns this item's stable identity.
  pub fn id(&self) -> &SharedString {
    &self.id
  }

  /// Returns this item's visible label.
  pub fn label(&self) -> &SharedString {
    &self.label
  }

  /// Returns this item's kind.
  pub fn kind(&self) -> TreeItemKind {
    self.kind
  }
}

/// Hierarchical list component for file trees and similar sidebars.
///
/// `Tree` owns only generic presentation state. It does not read the filesystem,
/// subscribe to events, or mutate expansion state by itself; those behaviors
/// should live in the application or domain crate and feed updated `TreeItem`
/// values back into this component.
pub struct Tree {
  /// The root node of the tree hierarchy.
  ///
  /// All visible items in the tree are descendants of this root. The root
  /// itself may or may not be rendered depending on the tree's configuration.
  root: TreeItem,
  /// Visual styling applied to all tree items and interactive states.
  ///
  /// Determines colors for text, backgrounds, borders, and hover/selection
  /// states. The theme is applied uniformly across the entire tree.
  theme: UIThemes,
  /// Callback invoked when a tree item is clicked.
  ///
  /// The callback receives the clicked item's ID and its current state.
  /// Application logic can use this to navigate, select, or perform actions
  /// in response to user interaction.
  on_click: Option<TreeItemClickListener>,
}

impl Tree {
  /// Creates a tree rooted at `root`.
  pub fn new(root: TreeItem) -> Self {
    Self {
      root,
      theme: builtins::dark(),
      on_click: None,
    }
  }

  /// Overrides the theme used to render tree rows.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Registers a click listener for every tree row.
  ///
  /// The listener receives the row id. Applications can use that id to update
  /// selection, expand directories, open files, or dispatch commands.
  pub fn on_click(
    mut self,
    listener: impl Fn(&TreeItemClickEvent, &mut Window, &mut App) + 'static,
  ) -> Self {
    self.on_click = Some(Rc::new(listener));
    self
  }
}

impl IntoElement for Tree {
  type Element = gpui::Div;

  fn into_element(self) -> Self::Element {
    div()
      .flex()
      .flex_col()
      .w_full()
      .text_color(self.theme.text.primary)
      .child(render_tree_item(self.root, self.theme, 0, self.on_click))
  }
}

/// Renders a tree item and its descendants recursively.
///
/// Each row displays an expand/collapse marker for nodes, the item label,
/// and applies indentation based on nesting depth. Hover and click interactions
/// are handled with the provided theme and callback.
///
/// # Arguments
/// * `item` - The tree item to render
/// * `theme` - Visual styling for the item and descendants
/// * `depth` - Nesting level used for indentation
/// * `on_click` - Optional callback triggered when the item is clicked
///
/// # Returns
/// A `Div` element containing the rendered row and expanded children.
fn render_tree_item(
  item: TreeItem,
  theme: UIThemes,
  depth: usize,
  on_click: Option<TreeItemClickListener>,
) -> gpui::Div {
  let id = item.id;
  let label = item.label;
  let children = item.children;
  let expanded = item.expanded;

  let mut row = div()
    .flex()
    .items_center()
    .h(DEFAULT_TREE_ROW_HEIGHT)
    .pl(px(depth as f32 * DEFAULT_TREE_INDENT))
    .pr_2()
    .gap_1()
    .text_xs()
    .cursor_pointer()
    .text_color(theme.text.secondary)
    .hover(move |style| {
      style
        .bg(theme.background.hover)
        .text_color(theme.text.primary)
    })
    .child(div().flex().items_center().justify_center().w(px(12.0)))
    .child(
      div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_color(theme.text.primary)
        .child(label),
    );

  if let Some(listener) = on_click.clone() {
    row = row.on_mouse_up(MouseButton::Left, move |_, window, cx| {
      listener(&TreeItemClickEvent { id: id.clone() }, window, cx);
    });
  }

  let mut node = div().flex().flex_col().w_full().child(row);

  if expanded {
    node = node.children(
      children
        .into_iter()
        .map(move |child| render_tree_item(child, theme, depth + 1, on_click.clone())),
    );
  }

  node
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn children_should_replace_existing_children() {
    let item = TreeItem::new("root", "root", TreeItemKind::Node)
      .children([TreeItem::new("first", "first", TreeItemKind::Leaf)])
      .children([TreeItem::new("second", "second", TreeItemKind::Leaf)]);

    assert_eq!(item.children.len(), 1);
    assert_eq!(item.children[0].id(), "second");
  }
}
