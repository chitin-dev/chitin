//! Semantic events for the reusable selector input primitive.

use gpui::SharedString;

/// Semantic events emitted by [`super::SelectInputState`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectInputEvent {
  /// The selected option changed.
  SelectionChange {
    /// Selected option identifier, or `None` after a cleared option list.
    selected_id: Option<SharedString>,
  },
  /// The options popup opened or closed.
  OpenChange {
    /// Whether the popup is visible.
    open: bool,
  },
  /// Selector availability changed.
  DisabledChange {
    /// Whether the selector is disabled.
    disabled: bool,
  },
  /// The selector received keyboard focus.
  Focus,
  /// The selector lost keyboard focus.
  Blur,
}
