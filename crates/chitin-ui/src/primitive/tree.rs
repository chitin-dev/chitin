//! Generic virtualized tree layout.
//!
//! This primitive owns the reusable virtual-list and scroll mechanics, while
//! callers provide domain rows and their presentation. It deliberately has no
//! selection, activation, or theme behavior to avoid imposing application
//! policy on filesystem, database, or scientific-data trees.

use std::rc::Rc;

use gpui::{
  App, ElementId, IntoElement, ParentElement, ScrollStrategy, Styled, UniformListScrollHandle, Window, div, px,
  uniform_list,
};

/// Default indent between adjacent tree depths.
pub const DEFAULT_TREE_INDENT: f32 = 12.0;
/// Default height for tree rows rendered by callers.
pub const DEFAULT_TREE_ROW_HEIGHT: gpui::Pixels = px(24.0);

/// Persistent scroll state owned by a virtualized tree.
#[derive(Clone, Debug)]
pub struct TreeState {
  scroll_handle: UniformListScrollHandle,
}

impl TreeState {
  /// Creates a tree state with a new virtual-list scroll handle.
  pub fn new() -> Self {
    Self::default()
  }

  /// Reveals a row using the requested viewport alignment.
  pub fn reveal_row(&self, row_index: usize, strategy: ScrollStrategy) {
    self.scroll_handle.scroll_to_item(row_index, strategy);
  }
}

impl Default for TreeState {
  /// Creates tree state with no prior scroll position.
  fn default() -> Self {
    Self {
      scroll_handle: UniformListScrollHandle::new(),
    }
  }
}

/// One item row in a flattened tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeItemRow<T> {
  /// Caller-owned item payload.
  pub data: T,
  /// Whether this row's node is expanded.
  pub expanded: bool,
  /// Zero-based nesting level used for indentation.
  pub depth: usize,
}

/// One non-interactive row in a flattened tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeMessageRow {
  /// Status text shown on this row.
  pub label: gpui::SharedString,
  /// Zero-based nesting level used for indentation.
  pub depth: usize,
}

/// One row in a virtualized tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeRow<T> {
  /// A real tree item row.
  Item(TreeItemRow<T>),
  /// A non-interactive status row.
  Message(TreeMessageRow),
}

/// Renders rows through GPUI's uniform virtual list.
///
/// `TreeState` retains viewport scroll position; `render_row` supplies the
/// domain-specific row visual and any domain action wiring.
pub fn virtual_tree_rows<T, R>(
  id: impl Into<ElementId>,
  rows: Vec<TreeRow<T>>,
  state: &TreeState,
  render_row: impl Fn(TreeRow<T>, &mut Window, &mut App) -> R + 'static,
) -> gpui::Div
where
  T: Clone + 'static,
  R: IntoElement,
{
  let id = id.into();
  let rows = Rc::new(rows);
  let row_count = rows.len();

  let list = uniform_list(id, row_count, move |range, window, cx| {
    range
      .filter_map(|index| rows.get(index).cloned())
      .map(|row| render_row(row, window, cx))
      .collect()
  })
  .track_scroll(&state.scroll_handle);

  div().flex().flex_1().min_h_0().w_full().child(list.size_full())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[derive(Clone, Debug, PartialEq, Eq)]
  struct TestPayload {
    id: &'static str,
  }

  #[test]
  fn tree_row_should_preserve_payload_and_depth() {
    let row = TreeRow::Item(TreeItemRow {
      data: TestPayload { id: "root" },
      expanded: true,
      depth: 2,
    });

    let TreeRow::Item(row) = row else {
      panic!("expected item row");
    };
    assert_eq!(row.data.id, "root");
    assert!(row.expanded);
    assert_eq!(row.depth, 2);
  }

  #[test]
  fn tree_message_row_should_store_status_text_and_depth() {
    let row = TreeRow::<TestPayload>::Message(TreeMessageRow {
      label: "Loading...".into(),
      depth: 1,
    });

    let TreeRow::Message(row) = row else {
      panic!("expected message row");
    };
    assert_eq!(row.label.as_ref(), "Loading...");
    assert_eq!(row.depth, 1);
  }
}
