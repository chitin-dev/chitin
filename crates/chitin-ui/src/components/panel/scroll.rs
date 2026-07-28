//! Scroll state for panel tab bars.

use std::{cell::RefCell, fmt, rc::Rc};

use gpui::ScrollHandle;

use super::model::PanelId;

/// Persistent scroll handles for panel tab bars.
///
/// Panel tab scrolling is presentation state: it should survive GPUI render
/// passes, but it should not be stored in the logical [`super::PanelTree`].
#[derive(Clone, Default)]
pub struct PanelTabScrollState {
  handles: Rc<RefCell<Vec<(PanelId, ScrollHandle)>>>,
}

impl PanelTabScrollState {
  /// Creates empty tab scroll state.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A [`PanelTabScrollState`] with no tracked panel handles.
  pub fn new() -> Self {
    Self::default()
  }

  /// Returns the scroll handle associated with one panel tab bar.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the panel whose tab bar should be scrolled.
  ///
  /// # Returns
  ///
  /// A persistent [`ScrollHandle`] reused across render passes for `panel_id`.
  pub fn scroll_handle(&self, panel_id: PanelId) -> ScrollHandle {
    if let Some((_, handle)) = self
      .handles
      .borrow()
      .iter()
      .find(|(existing_panel_id, _)| *existing_panel_id == panel_id)
    {
      return handle.clone();
    }

    let handle = ScrollHandle::new();
    self.handles.borrow_mut().push((panel_id, handle.clone()));
    handle
  }

  /// Queues a scroll request that reveals one tab in its panel tab bar.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the panel whose tab bar owns the tab.
  ///
  /// `tab_index` is the tab's zero-based position in that panel's tab list.
  ///
  /// # Returns
  ///
  /// This function has no return value.
  pub fn reveal_tab(&self, panel_id: PanelId, tab_index: usize) {
    self.scroll_handle(panel_id).scroll_to_item(tab_index);
  }

  #[cfg(test)]
  pub(crate) fn tracked_panel_count(&self) -> usize {
    self.handles.borrow().len()
  }
}

impl fmt::Debug for PanelTabScrollState {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("PanelTabScrollState")
      .field(
        "panel_ids",
        &self
          .handles
          .borrow()
          .iter()
          .map(|(panel_id, _)| *panel_id)
          .collect::<Vec<_>>(),
      )
      .finish()
  }
}

impl PartialEq for PanelTabScrollState {
  fn eq(&self, other: &Self) -> bool {
    self
      .handles
      .borrow()
      .iter()
      .map(|(panel_id, _)| *panel_id)
      .eq(other.handles.borrow().iter().map(|(panel_id, _)| *panel_id))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn scroll_handle_should_reuse_existing_panel_handle() {
    let scroll_state = PanelTabScrollState::new();

    scroll_state.scroll_handle(PanelId::new(1));
    scroll_state.scroll_handle(PanelId::new(1));

    assert_eq!(scroll_state.tracked_panel_count(), 1);
  }

  #[test]
  fn scroll_handle_should_track_distinct_panel_handles() {
    let scroll_state = PanelTabScrollState::new();

    scroll_state.scroll_handle(PanelId::new(1));
    scroll_state.scroll_handle(PanelId::new(2));

    assert_eq!(scroll_state.tracked_panel_count(), 2);
  }
}
