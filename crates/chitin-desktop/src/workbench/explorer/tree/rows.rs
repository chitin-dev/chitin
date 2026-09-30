//! Desktop-owned projection rows for the controlled workspace tree.

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
