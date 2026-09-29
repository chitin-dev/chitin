//! Semantic events published by the scrollbar primitive.

use gpui::Pixels;

/// A viewport motion requested by a scrollbar interaction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollbarEvent {
  /// The user moved the viewport to this offset in the caller's pixel space.
  ///
  /// The offset follows GPUI's scroll convention: zero shows the start of the
  /// content and it becomes more negative as the viewport moves toward the end.
  Scrolled { offset: Pixels },
}
