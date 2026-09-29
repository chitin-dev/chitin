//! Active-item and resize state for the bottom dock.

use gpui::{Pixels, SharedString, px};

use crate::primitive::resize::ResizeGesture;

/// Default bottom-dock height.
pub const DEFAULT_BOTTOM_DOCK_HEIGHT: Pixels = px(260.0);
/// Default minimum height retained by an open bottom dock.
pub const DEFAULT_BOTTOM_DOCK_MIN_HEIGHT: Pixels = px(120.0);
/// Default minimum height reserved for the workbench center area.
pub const DEFAULT_CENTER_AREA_MIN_HEIGHT: Pixels = px(160.0);

/// Stable semantic identity of one bottom-dock tool.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BottomDockItemId(SharedString);

impl BottomDockItemId {
  /// Creates a dock-item identity from an application-defined stable string.
  pub fn new(id: impl Into<SharedString>) -> Self {
    Self(id.into())
  }

  /// Returns the stable string representation.
  pub fn as_str(&self) -> &str {
    &self.0
  }
}

/// Active tool and geometry state for a reusable bottom workbench dock.
#[derive(Clone, Debug)]
pub struct BottomDockState {
  active_item: Option<BottomDockItemId>,
  height: Pixels,
  min_height: Pixels,
  min_center_height: Pixels,
  resize_drag: Option<ResizeGesture<(), Pixels>>,
}

impl BottomDockState {
  /// Creates a closed dock with the default height constraints.
  pub fn new() -> Self {
    Self {
      active_item: None,
      height: DEFAULT_BOTTOM_DOCK_HEIGHT,
      min_height: DEFAULT_BOTTOM_DOCK_MIN_HEIGHT,
      min_center_height: DEFAULT_CENTER_AREA_MIN_HEIGHT,
      resize_drag: None,
    }
  }

  /// Returns the active dock item, or `None` while the dock is closed.
  pub fn active_item(&self) -> Option<&BottomDockItemId> {
    self.active_item.as_ref()
  }

  /// Returns whether any dock item is visible.
  pub const fn is_open(&self) -> bool {
    self.active_item.is_some()
  }

  /// Returns whether the identified item is currently visible.
  pub fn is_active(&self, item_id: &str) -> bool {
    self
      .active_item
      .as_ref()
      .is_some_and(|active| active.as_str() == item_id)
  }

  /// Shows the identified item, replacing the previously active dock tool.
  pub fn show(&mut self, item_id: impl Into<SharedString>) {
    self.active_item = Some(BottomDockItemId::new(item_id));
  }

  /// Toggles one item and returns whether it is visible afterward.
  ///
  /// An inactive item replaces the current dock content instead of closing the
  /// dock. Toggling the already-active item closes the dock.
  ///
  /// # Parameters
  ///
  /// * `item_id` is the stable identity of the tool being toggled.
  ///
  /// # Returns
  ///
  /// `true` when the requested item is visible afterward; otherwise `false`.
  pub fn toggle(&mut self, item_id: impl Into<SharedString>) -> bool {
    let item_id = BottomDockItemId::new(item_id);
    if self.active_item.as_ref() == Some(&item_id) {
      self.close();
      false
    } else {
      self.active_item = Some(item_id);
      true
    }
  }

  /// Closes the dock and stops any active resize gesture.
  pub fn close(&mut self) {
    self.active_item = None;
    self.resize_drag = None;
  }

  /// Returns the current dock height.
  pub const fn height(&self) -> Pixels {
    self.height
  }

  /// Starts resizing from the dock's top edge.
  pub fn start_resize(&mut self, start_y: Pixels) {
    self.resize_drag = Some(ResizeGesture::new((), start_y, self.height));
  }

  /// Updates dock height while preserving minimum center-area space.
  ///
  /// # Parameters
  ///
  /// * `current_y` is the latest vertical pointer position.
  /// * `available_height` is the workbench height available to the center area
  ///   and bottom dock together.
  ///
  /// # Returns
  ///
  /// `true` when an active resize gesture updated the dock; otherwise `false`.
  pub fn drag_resize(&mut self, current_y: Pixels, available_height: Pixels) -> bool {
    let Some(resize_drag) = self.resize_drag else {
      return false;
    };
    let maximum_height =
      px((f32::from(available_height) - f32::from(self.min_center_height)).max(f32::from(self.min_height)));
    let desired_height = f32::from(resize_drag.start_value()) - resize_drag.delta(current_y);
    self.height = px(desired_height.clamp(f32::from(self.min_height), f32::from(maximum_height)));
    true
  }

  /// Stops the active dock resize gesture.
  pub fn stop_resize(&mut self) -> bool {
    self.resize_drag.take().is_some()
  }

  /// Returns whether the dock top edge is being dragged.
  pub const fn is_resizing(&self) -> bool {
    self.resize_drag.is_some()
  }
}

impl Default for BottomDockState {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn toggling_active_item_should_close_dock() {
    let mut state = BottomDockState::new();
    state.show("terminal");

    assert!(!state.toggle("terminal"));
    assert!(!state.is_open());
  }

  #[test]
  fn toggling_inactive_item_should_replace_content() {
    let mut state = BottomDockState::new();
    state.show("terminal");

    assert!(state.toggle("tasks"));
    assert!(state.is_active("tasks"));
  }

  #[test]
  fn drag_resize_should_increase_height_when_pointer_moves_up() {
    let mut state = BottomDockState::new();
    state.start_resize(px(400.0));

    assert!(state.drag_resize(px(350.0), px(800.0)));
    assert_eq!(state.height(), px(310.0));
  }

  #[test]
  fn drag_resize_should_preserve_minimum_center_height() {
    let mut state = BottomDockState::new();
    state.start_resize(px(400.0));

    assert!(state.drag_resize(px(0.0), px(600.0)));
    assert_eq!(state.height(), px(440.0));
  }

  #[test]
  fn drag_resize_should_preserve_minimum_dock_height() {
    let mut state = BottomDockState::new();
    state.start_resize(px(400.0));

    assert!(state.drag_resize(px(800.0), px(600.0)));
    assert_eq!(state.height(), DEFAULT_BOTTOM_DOCK_MIN_HEIGHT);
  }
}
