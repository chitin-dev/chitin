use gpui::{Context, EventEmitter, FocusHandle, SharedString};

use super::TextEvent;

/// Persistent state for text that may be copied to the system clipboard.
pub struct TextState {
  text: SharedString,
  copyable: bool,
  focused: bool,
  focus_handle: FocusHandle,
}

impl TextState {
  /// Creates text state with copying disabled.
  pub fn new(text: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
    Self {
      text: text.into(),
      copyable: false,
      focused: false,
      focus_handle: cx.focus_handle(),
    }
  }

  /// Returns the displayed text.
  pub fn text(&self) -> &str {
    &self.text
  }

  /// Returns whether this text can be copied with Ctrl/Cmd+C.
  pub fn is_copyable(&self) -> bool {
    self.copyable
  }

  /// Returns whether this text currently owns keyboard focus.
  pub fn is_focused(&self) -> bool {
    self.focused
  }

  /// Returns the focus handle tracked by this text element.
  pub fn focus_handle(&self) -> &FocusHandle {
    &self.focus_handle
  }

  /// Replaces the displayed text and emits a change event when it differs.
  pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
    let text = text.into();
    if self.text == text {
      return;
    }

    self.text = text;
    cx.emit(TextEvent::Change {
      value: self.text.clone(),
    });
    cx.notify();
  }

  /// Enables or disables copying with Ctrl/Cmd+C.
  pub fn set_copyable(&mut self, copyable: bool, cx: &mut Context<Self>) {
    if self.copyable == copyable {
      return;
    }

    self.copyable = copyable;
    cx.emit(TextEvent::CopyableChange { copyable });
    cx.notify();
  }

  /// Synchronizes focus state from the rendered GPUI focus handle.
  pub(crate) fn sync_focus(&mut self, focused: bool, cx: &mut Context<Self>) {
    if self.focused == focused {
      return;
    }

    self.focused = focused;
    cx.emit(if focused { TextEvent::Focus } else { TextEvent::Blur });
    cx.notify();
  }

  /// Emits a copied event when copying is currently enabled.
  pub(crate) fn copy(&self, cx: &mut Context<Self>) -> Option<SharedString> {
    if !self.copyable {
      return None;
    }

    let value = self.text.clone();
    cx.emit(TextEvent::Copied { value: value.clone() });
    cx.notify();
    Some(value)
  }
}

impl EventEmitter<TextEvent> for TextState {}
