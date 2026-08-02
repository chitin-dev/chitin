/// Semantic events emitted by [`super::ButtonState`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonEvent {
  /// The button was activated by a pointer or keyboard action.
  Click,
  /// The button availability changed.
  DisabledChange {
    /// Whether the button is disabled.
    disabled: bool,
  },
  /// The pressed visual state changed.
  PressedChange {
    /// Whether the button is currently pressed.
    pressed: bool,
  },
  /// The button received keyboard focus.
  Focus,
  /// The button lost keyboard focus.
  Blur,
}
