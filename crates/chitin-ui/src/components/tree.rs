//! Reusable virtual tree row primitives.
//!
//! This module deliberately does not define a domain tree model. Application
//! crates provide their own data payloads, expansion state, icons, and event
//! behavior, then use [`TreeRow`] and [`virtual_tree_rows`] for shared
//! viewport-bounded rendering.

use std::rc::Rc;

use gpui::{App, ElementId, IntoElement, ParentElement, Styled, Window, div, px, uniform_list};

/// Default indent of tree items between levels.
pub const DEFAULT_TREE_INDENT: f32 = 12.0;
/// Default height of tree rows.
pub const DEFAULT_TREE_ROW_HEIGHT: gpui::Pixels = px(24.0);

/// One item row in a flattened tree.
///
/// `data` is caller-owned payload. Desktop workspace trees use this to keep
/// non-lossy filesystem paths, while other callers can store JSON paths,
/// command ids, or domain object identifiers.
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
///
/// Message rows are useful for loading, empty, or error states that should
/// participate in virtual scrolling without pretending to be real tree items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeMessageRow {
  /// Status text shown on this row.
  pub label: gpui::SharedString,
  /// Zero-based nesting level used for indentation.
  pub depth: usize,
}

/// One row in a virtualized tree.
///
/// This enum is intentionally generic so applications can share Chitin's
/// virtual-list infrastructure without forcing their domain data into string
/// identifiers or a fixed tree node type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeRow<T> {
  /// A real tree item row.
  Item(TreeItemRow<T>),
  /// A non-interactive status row.
  Message(TreeMessageRow),
}

/// Renders `rows` with GPUI's uniform virtual list.
///
/// The caller provides both the row payload type and the row renderer. This
/// function owns only shared virtual-list mechanics: stable element id,
/// viewport-bounded row selection, and full-size list layout.
pub fn virtual_tree_rows<T, R>(
  id: impl Into<ElementId>,
  rows: Vec<TreeRow<T>>,
  render_row: impl Fn(TreeRow<T>, &mut Window, &mut App) -> R + 'static,
) -> gpui::Div
where
  T: Clone + 'static,
  R: IntoElement,
{
  let id = id.into();
  let log_id = id.clone();
  let rows = Rc::new(rows);
  let row_count = rows.len();

  div().flex().flex_1().min_h_0().w_full().child(
    uniform_list(id, row_count, move |range, window, cx| {
      log::debug!("Virtual tree rows range is {:?}, from id {}", range, log_id);
      range
        .filter_map(|index| rows.get(index).cloned())
        .map(|row| render_row(row, window, cx))
        .collect()
    })
    .size_full(),
  )
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
