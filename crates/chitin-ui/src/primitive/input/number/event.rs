//! Semantic events for the reusable numeric input primitive.

use gpui::SharedString;

/// Semantic events emitted by [`super::NumberInputState`].
#[derive(Clone, Debug, PartialEq)]
pub enum NumberInputEvent {
  /// The editable numeric text changed, including temporarily invalid drafts.
  DraftChange {
    /// Current editing text.
    draft: SharedString,
  },
  /// The parsed value changed after a draft update.
  ValueChange {
    /// Parsed finite numeric value, or `None` for an incomplete or invalid draft.
    value: Option<f64>,
  },
  /// The user completed an edit by submitting or leaving the control.
  Commit {
    /// The final draft text submitted by the user.
    draft: SharedString,
    /// Parsed finite numeric value, or `None` when the draft is not yet valid.
    value: Option<f64>,
  },
  /// Editing availability changed.
  DisabledChange {
    /// Whether numeric editing is disabled.
    disabled: bool,
  },
  /// Read-only availability changed.
  ReadOnlyChange {
    /// Whether numeric editing is read-only.
    readonly: bool,
  },
  /// The numeric input received keyboard focus.
  Focus,
  /// The numeric input lost keyboard focus.
  Blur,
}
