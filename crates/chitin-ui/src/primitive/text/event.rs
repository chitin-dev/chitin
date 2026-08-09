//! Semantic events for the reusable text display primitive.

use gpui::SharedString;

/// Semantic events emitted by [`super::TextState`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextEvent {
  /// The displayed text changed.
  Change {
    /// Current complete text value.
    value: SharedString,
  },
  /// Clipboard support changed.
  CopyableChange {
    /// Whether Ctrl/Cmd+C can copy this text.
    copyable: bool,
  },
  /// The complete text was written to the clipboard.
  Copied {
    /// Text written to the clipboard.
    value: SharedString,
  },
  /// The text element received keyboard focus.
  Focus,
  /// The text element lost keyboard focus.
  Blur,
}
