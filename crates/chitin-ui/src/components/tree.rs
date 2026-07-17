//! Reusable tree and tree items building blocks.
//!
//! Tree and tree items should be generic to represent different tree-structure
//! data like workspace, json etc. So every function and property in this component
//! should not be related to behaviors of specific tree-structure data.

use std::rc::Rc;

use gpui::{
  App, InteractiveElement, IntoElement, MouseButton, ParentElement, SharedString, Styled, Window,
  div, px, uniform_list,
};

use crate::themes::{UIThemes, builtins};

/// Default indent of tree items between levels
pub const DEFAULT_TREE_INDENT: f32 = 12.0;
/// Default height of tree items
pub const DEFAULT_TREE_ROW_HEIGHT: gpui::Pixels = px(24.0);

type TreeItemClickListener = Rc<dyn Fn(&TreeItemClickEvent, &mut Window, &mut App)>;

/// One row in a flattened visible tree.
///
/// `Tree` converts expanded hierarchical data into this linear form before
/// handing rows to GPUI's virtual list renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleTreeItem {
  /// Stable id of the item rendered on this row.
  pub id: SharedString,
  /// Display text shown on this row.
  pub label: SharedString,
  /// Semantic classification of the row item.
  pub kind: TreeItemKind,
  /// Whether this row's node is expanded.
  pub expanded: bool,
  /// Zero-based nesting level used for indentation.
  pub depth: usize,
}

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
    let rows = Rc::new(visible_tree_items(&self.root));
    let row_count = rows.len();
    let theme = self.theme;
    let on_click = self.on_click;

    div()
      .flex()
      .flex_1()
      .min_h_0()
      .w_full()
      .text_color(theme.text.primary)
      .child(
        uniform_list("tree-rows", row_count, move |range, _, _| {
          range
            .filter_map(|index| rows.get(index).cloned())
            .map(|row| render_tree_row(row, theme, on_click.clone()))
            .collect()
        })
        .size_full(),
      )
  }
}

/// Flattens an expanded tree into rows suitable for virtual list rendering.
///
/// Collapsed descendants are skipped, so the returned vector represents the
/// rows that can appear in the viewport.
pub fn visible_tree_items(root: &TreeItem) -> Vec<VisibleTreeItem> {
  let mut rows = Vec::new();
  collect_visible_tree_items(root, 0, &mut rows);
  rows
}

fn collect_visible_tree_items(item: &TreeItem, depth: usize, rows: &mut Vec<VisibleTreeItem>) {
  rows.push(VisibleTreeItem {
    id: item.id.clone(),
    label: item.label.clone(),
    kind: item.kind,
    expanded: item.expanded,
    depth,
  });

  if item.expanded {
    for child in &item.children {
      collect_visible_tree_items(child, depth + 1, rows);
    }
  }
}

/// Renders one virtualized tree row.
///
/// Each row displays a generic leading slot, the item label, and indentation
/// based on nesting depth. Hover and click interactions are handled with the
/// provided theme and callback. The row spans the full available width so hover
/// styling and pointer hitboxes do not shrink to the text content.
///
/// # Arguments
/// * `row` - The flattened row to render
/// * `theme` - Visual styling for the item and descendants
/// * `on_click` - Optional callback triggered when the item is clicked
///
/// # Returns
/// A `Div` element containing the rendered row.
fn render_tree_row(
  row: VisibleTreeItem,
  theme: UIThemes,
  on_click: Option<TreeItemClickListener>,
) -> gpui::Div {
  let id = row.id;
  let label = row.label;

  let mut row = div()
    .flex()
    .items_center()
    // Keep hover background and pointer hitbox full-width inside uniform_list.
    .w_full()
    .h(DEFAULT_TREE_ROW_HEIGHT)
    .pl(px(row.depth as f32 * DEFAULT_TREE_INDENT))
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

  row
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

  #[test]
  fn visible_tree_items_should_skip_collapsed_descendants() {
    let tree = TreeItem::new("root", "root", TreeItemKind::Node)
      .children([
        TreeItem::new("expanded", "expanded", TreeItemKind::Node)
          .expanded(true)
          .children([TreeItem::new("child", "child", TreeItemKind::Leaf)]),
        TreeItem::new("collapsed", "collapsed", TreeItemKind::Node).children([TreeItem::new(
          "hidden",
          "hidden",
          TreeItemKind::Leaf,
        )]),
      ])
      .expanded(true);

    let rows = visible_tree_items(&tree);

    let labels = rows
      .iter()
      .map(|row| row.label.as_ref())
      .collect::<Vec<_>>();
    assert_eq!(labels, ["root", "expanded", "child", "collapsed"]);
  }
}
