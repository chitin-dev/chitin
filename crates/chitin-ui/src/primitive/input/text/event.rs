//! Semantic events and selection data for the text input primitive.

use std::ops::Range;

use gpui::SharedString;

/// Semantic events emitted by [`super::TextInputState`].
///
/// Consumers react to these events instead of interpreting raw keyboard or
/// mouse input from the text control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextInputEvent {
  /// The visible text changed.
  Change {
    /// Current complete input value.
    value: SharedString,
  },
  /// The cursor or selection changed without changing text.
  SelectionChange {
    /// Current cursor and selection state.
    selection: TextSelection,
  },
  /// Editing availability changed.
  DisabledChange {
    /// Whether the input is disabled.
    disabled: bool,
  },
  /// Read-only mode changed.
  ReadOnlyChange {
    /// Whether the input is read-only.
    readonly: bool,
  },
  /// The user requested submission, normally by pressing Enter.
  Submit {
    /// Current complete input value.
    value: SharedString,
  },
  /// The user requested cancellation, normally by pressing Escape.
  Cancel,
  /// The input received keyboard focus.
  Focus,
  /// The input lost keyboard focus.
  Blur,
}

/// A half-open byte range inside the input text. All positions are UTF-8 byte
/// offsets and must remain on `char` boundaries. Grapheme-aware cursor movement
/// can be added without change the public selection representation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextSelection {
  /// The fixed end of the selection.
  anchor: usize,
  /// The active end of the selection.
  head: usize,
}

impl TextSelection {
  /// Creates a selection from an anchor and active head.
  pub fn new(anchor: usize, head: usize) -> Self {
    Self { anchor, head }
  }

  /// Returns the fixed end of the selection.
  pub fn anchor(&self) -> usize {
    self.anchor
  }

  /// Returns the active end of the selection.
  pub fn head(&self) -> usize {
    self.head
  }

  /// Creates a collapsed selection at `offset`.
  pub fn caret(offset: usize) -> Self {
    Self {
      anchor: offset,
      head: offset,
    }
  }

  /// Returns whether the selection contains no text.
  pub fn is_collapsed(&self) -> bool {
    self.anchor == self.head
  }

  /// Returns the normalized half-open selection range.
  pub fn range(&self) -> Range<usize> {
    self.anchor.min(self.head)..self.anchor.max(self.head)
  }
}
