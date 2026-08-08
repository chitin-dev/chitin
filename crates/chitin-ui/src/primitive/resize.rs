//! Generic resize gesture state.

use gpui::Pixels;

/// Active resize gesture metadata shared by resizable UI components.
///
/// `TAnchor` identifies the resized target, such as a split path or a sidebar
/// edge. `TValue` stores the target's value when the drag started, such as a
/// width or split ratio. Components keep their own resize policy and use this
/// type only for common drag bookkeeping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResizeGesture<TAnchor, TValue> {
  /// Component-specific target being resized.
  anchor: TAnchor,
  /// Cursor position on the resize axis when dragging started.
  start_position: Pixels,
  /// Component-specific value when dragging started.
  start_value: TValue,
}

impl<TAnchor, TValue> ResizeGesture<TAnchor, TValue> {
  /// Creates resize gesture metadata.
  ///
  /// # Parameters
  ///
  /// `anchor` identifies the resized target.
  ///
  /// `start_position` is the cursor position on the resize axis where dragging
  /// began.
  ///
  /// `start_value` is the target value when dragging began.
  ///
  /// # Returns
  ///
  /// A [`ResizeGesture`] containing the initial drag metadata.
  pub fn new(anchor: TAnchor, start_position: Pixels, start_value: TValue) -> Self {
    Self {
      anchor,
      start_position,
      start_value,
    }
  }

  /// Returns the resized target anchor.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// A shared reference to the component-specific anchor.
  pub fn anchor(&self) -> &TAnchor {
    &self.anchor
  }

  /// Returns the cursor position where the drag started.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// The start cursor position on the resize axis.
  pub fn start_position(&self) -> Pixels {
    self.start_position
  }
}

impl<TAnchor, TValue: Copy> ResizeGesture<TAnchor, TValue> {
  /// Returns the target value when the drag started.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// The copied start value.
  pub fn start_value(&self) -> TValue {
    self.start_value
  }

  /// Returns the pointer delta from the start position.
  ///
  /// # Parameters
  ///
  /// `current_position` is the latest cursor position on the resize axis.
  ///
  /// # Returns
  ///
  /// The signed logical-pixel delta between current and start positions.
  pub fn delta(&self, current_position: Pixels) -> f32 {
    f32::from(current_position) - f32::from(self.start_position)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gpui::px;

  /// Verifies that resize gestures preserve typed anchors and start values.
  #[test]
  fn resize_gesture_should_store_anchor_and_start_value() {
    let gesture = ResizeGesture::new("split", px(100.0), 0.5_f32);

    assert_eq!(gesture.anchor(), &"split");
    assert_eq!(gesture.start_position(), px(100.0));
    assert_eq!(gesture.start_value(), 0.5);
  }

  /// Verifies that resize gestures calculate pointer deltas.
  #[test]
  fn resize_gesture_should_calculate_delta_from_start_position() {
    let gesture = ResizeGesture::new((), px(100.0), px(260.0));

    assert_eq!(gesture.delta(px(140.0)), 40.0);
  }
}
