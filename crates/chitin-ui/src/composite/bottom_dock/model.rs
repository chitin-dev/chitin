//! Active-item and remembered geometry state for the bottom dock.

use gpui::{Pixels, SharedString, px};

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
}

impl BottomDockState {
  /// Creates a closed dock with the default height constraints.
  pub fn new() -> Self {
    Self {
      active_item: None,
      height: DEFAULT_BOTTOM_DOCK_HEIGHT,
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

  /// Closes the dock while retaining its last measured height.
  pub fn close(&mut self) {
    self.active_item = None;
  }

  /// Returns the current dock height.
  pub const fn height(&self) -> Pixels {
    self.height
  }

  /// Records Kit's measured height without owning its drag lifecycle.
  ///
  /// # Parameters
  ///
  /// * `height` is the bottom pane's extent reported by Kit. Non-finite values
  ///   are ignored; Kit enforces the center-area constraint during layout.
  ///
  /// # Returns
  ///
  /// Whether the remembered height changed, after enforcing the dock minimum.
  pub fn set_height(&mut self, height: Pixels) -> bool {
    let value = f32::from(height);
    if !value.is_finite() {
      return false;
    }
    let height = px(value.max(f32::from(DEFAULT_BOTTOM_DOCK_MIN_HEIGHT)));
    if self.height == height {
      return false;
    }
    self.height = height;
    true
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
  fn reported_height_should_preserve_minimum_dock_height() {
    let mut state = BottomDockState::new();
    state.set_height(px(50.0));
    assert_eq!(state.height(), DEFAULT_BOTTOM_DOCK_MIN_HEIGHT);
  }

  #[test]
  fn closing_should_retain_height_for_reopening() {
    let mut state = BottomDockState::new();
    state.set_height(px(350.0));
    state.show("terminal");
    state.close();
    state.show("terminal");
    assert_eq!(state.height(), px(350.0));
  }

  #[test]
  fn invalid_or_unchanged_height_should_not_request_a_render() {
    let mut state = BottomDockState::new();
    assert!(!state.set_height(px(f32::NAN)));
    assert!(!state.set_height(px(f32::INFINITY)));
    assert!(!state.set_height(DEFAULT_BOTTOM_DOCK_HEIGHT));
  }
}
