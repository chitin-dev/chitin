//! Semantic events for the reusable numeric input primitive.

use gpui::SharedString;

use super::NumberDraftState;

/// Semantic events emitted by [`super::NumberInputState`].
#[derive(Clone, Debug, PartialEq)]
pub enum NumberInputEvent {
  /// The editable numeric text changed, including temporarily invalid drafts.
  DraftChange {
    /// Current editing text.
    draft: SharedString,
  },
  /// The semantic numeric state of the current draft changed.
  DraftStateChange {
    /// Updated semantic state for the current raw draft.
    state: NumberDraftState,
  },
  /// The parsed value changed after a draft update.
  ValueChange {
    /// Parsed finite numeric value, or `None` for empty, incomplete, and invalid drafts.
    value: Option<f64>,
  },
  /// The user completed an edit by submitting or leaving the control.
  Commit {
    /// Final visible draft text after an accepted value is formatted.
    draft: SharedString,
    /// Semantic state of the final draft.
    state: NumberDraftState,
    /// Whether the draft was valid and updated the committed value.
    accepted: bool,
  },
  /// The user cancelled editing and the committed value was restored.
  Cancel,
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
