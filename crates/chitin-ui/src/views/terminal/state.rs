//! Persistent terminal viewport scroll state.

use gpui::{Context, FocusHandle, ScrollHandle};

/// Persistent scrolling behavior for a terminal viewport.
pub struct TerminalViewportState {
  scroll_handle: ScrollHandle,
  focus_handle: FocusHandle,
  follow_tail: bool,
}

impl TerminalViewportState {
  /// Creates a viewport that follows new output.
  pub fn new(cx: &mut Context<Self>) -> Self {
    Self {
      scroll_handle: ScrollHandle::new(),
      focus_handle: cx.focus_handle(),
      follow_tail: true,
    }
  }

  /// Returns the GPUI scroll handle used by the viewport.
  pub fn scroll_handle(&self) -> &ScrollHandle {
    &self.scroll_handle
  }

  /// Returns the focus handle used while a command is running.
  pub fn focus_handle(&self) -> &FocusHandle {
    &self.focus_handle
  }

  /// Returns whether new output should remain visible.
  pub const fn follows_tail(&self) -> bool {
    self.follow_tail
  }

  /// Enables or disables automatic tail following.
  pub fn set_follow_tail(&mut self, follow_tail: bool) {
    self.follow_tail = follow_tail;
  }

  /// Scrolls to the newest line when tail following is enabled.
  pub fn reveal_tail(&self) {
    if self.follow_tail {
      self.scroll_handle.scroll_to_bottom();
    }
  }
}
