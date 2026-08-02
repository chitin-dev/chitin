use gpui::{Context, EventEmitter, FocusHandle};

use super::ButtonEvent;

/// Persistent interaction state for a reusable button.
pub struct ButtonState {
  focus_handle: FocusHandle,
  disabled: bool,
  pressed: bool,
  focused: bool,
}

impl ButtonState {
  /// Creates an enabled button state with a GPUI focus handle.
  pub fn new(cx: &mut Context<Self>) -> Self {
    Self {
      focus_handle: cx.focus_handle(),
      disabled: false,
      pressed: false,
      focused: false,
    }
  }

  /// Returns the focus handle tracked by this button.
  pub fn focus_handle(&self) -> &FocusHandle {
    &self.focus_handle
  }

  /// Returns whether activation is disabled.
  pub fn is_disabled(&self) -> bool {
    self.disabled
  }

  /// Returns whether the button is currently pressed.
  pub fn is_pressed(&self) -> bool {
    self.pressed
  }

  /// Returns whether this button currently owns keyboard focus.
  pub fn is_focused(&self) -> bool {
    self.focused
  }

  /// Enables or disables activation and emits an availability event when changed.
  pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
    if self.disabled == disabled {
      return;
    }

    self.disabled = disabled;
    if disabled {
      self.set_pressed(false, cx);
    }
    cx.emit(ButtonEvent::DisabledChange { disabled });
    cx.notify();
  }

  /// Synchronizes focus state from the rendered GPUI focus handle.
  pub(crate) fn sync_focus(&mut self, focused: bool, cx: &mut Context<Self>) {
    if self.focused == focused {
      return;
    }

    self.focused = focused;
    cx.emit(if focused { ButtonEvent::Focus } else { ButtonEvent::Blur });
    cx.notify();
  }

  /// Marks the button as pressed while an activation gesture is in progress.
  pub(crate) fn press(&mut self, cx: &mut Context<Self>) {
    if !self.disabled {
      self.set_pressed(true, cx);
    }
  }

  /// Completes a pointer activation and emits a click when it began pressed.
  pub(crate) fn release_and_click(&mut self, cx: &mut Context<Self>) -> bool {
    if self.disabled || !self.pressed {
      return false;
    }

    self.set_pressed(false, cx);
    self.emit_click(cx);
    true
  }

  /// Activates the button from a keyboard gesture.
  pub(crate) fn activate(&mut self, cx: &mut Context<Self>) {
    if !self.disabled {
      self.emit_click(cx);
    }
  }

  fn set_pressed(&mut self, pressed: bool, cx: &mut Context<Self>) {
    if self.pressed == pressed {
      return;
    }

    self.pressed = pressed;
    cx.emit(ButtonEvent::PressedChange { pressed });
    cx.notify();
  }

  fn emit_click(&self, cx: &mut Context<Self>) {
    cx.emit(ButtonEvent::Click);
    cx.notify();
  }
}

impl EventEmitter<ButtonEvent> for ButtonState {}
