//! Panel tab drag-and-drop payloads, config, and hit testing.

use std::rc::Rc;

use gpui::{App, Bounds, Pixels, SharedString, Window};

use super::model::{PanelId, PanelTabId};

/// Callback invoked when GPUI starts a panel-tab drag.
pub type PanelTabDragStartHandler = dyn Fn(PanelTabDrag, &mut Window, &mut App);
/// Callback invoked when a panel-tab drag enters a new insertion position.
pub type PanelTabDragTargetHandler = dyn Fn(PanelTabDropTarget, &mut Window, &mut App);
/// Callback invoked when a panel tab is dropped on a valid tab strip.
pub type PanelTabDropHandler = dyn Fn(PanelTabDrag, PanelId, &mut Window, &mut App);

/// Stable payload carried by GPUI while one panel tab is being dragged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelTabDrag {
  /// Panel that owns the tab when the drag starts.
  pub source_panel_id: PanelId,
  /// Stable identifier of the dragged tab.
  pub tab_id: PanelTabId,
  /// Tab position when the drag starts.
  pub source_index: usize,
  /// Tab title rendered by the lightweight drag preview.
  pub title: SharedString,
}

/// Proposed insertion position in an existing panel tab strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelTabDropTarget {
  /// Panel that will receive the dragged tab.
  pub panel_id: PanelId,
  /// Raw insertion position before same-panel removal normalization.
  pub insertion_index: usize,
}

/// Temporary presentation state for one active panel-tab drag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelTabDragState {
  /// Stable drag payload created when GPUI crosses its drag threshold.
  pub drag: PanelTabDrag,
  /// Current valid tab-strip target, if the pointer is over one.
  pub drop_target: Option<PanelTabDropTarget>,
}

/// Callbacks and temporary presentation state for panel-tab dragging.
#[derive(Clone)]
pub struct PanelTabDragConfig {
  /// Current drag session used to render source and target feedback.
  pub(super) state: Option<PanelTabDragState>,
  /// Callback invoked after GPUI crosses its pointer movement threshold.
  pub(super) on_start: Rc<PanelTabDragStartHandler>,
  /// Callback invoked when actual tab bounds produce a drop target.
  pub(super) on_target: Rc<PanelTabDragTargetHandler>,
  /// Callback invoked when GPUI drops the payload on a valid tab strip.
  pub(super) on_drop: Rc<PanelTabDropHandler>,
}

impl PanelTabDragConfig {
  /// Creates tab drag configuration from application-owned callbacks.
  ///
  /// # Parameters
  ///
  /// * `on_start` begins the application's temporary drag session.
  /// * `on_target` records the current panel and raw insertion position.
  /// * `on_drop` commits a payload released over a valid tab strip.
  ///
  /// # Returns
  ///
  /// A [`PanelTabDragConfig`] without an active presentation state.
  pub fn new(
    on_start: Rc<PanelTabDragStartHandler>,
    on_target: Rc<PanelTabDragTargetHandler>,
    on_drop: Rc<PanelTabDropHandler>,
  ) -> Self {
    Self {
      state: None,
      on_start,
      on_target,
      on_drop,
    }
  }

  /// Sets the current temporary drag state used by panel rendering.
  ///
  /// # Parameters
  ///
  /// * `state` identifies the dragged tab and current valid insertion target.
  ///
  /// # Returns
  ///
  /// The updated [`PanelTabDragConfig`] for builder chaining.
  pub fn state(mut self, state: Option<PanelTabDragState>) -> Self {
    self.state = state;
    self
  }
}

/// Calculates an insertion position from actual rendered tab bounds.
///
/// Bounds must be supplied in visual order. The first tab whose midpoint lies
/// to the right of `pointer_x` determines the insertion position; a pointer
/// after every midpoint appends after the final tab.
///
/// # Parameters
///
/// * `tab_bounds` contains actual logical-pixel bounds in strip order.
/// * `pointer_x` is the pointer's horizontal logical-pixel coordinate.
///
/// # Returns
///
/// An insertion index from `0..=tab_bounds.len()`. Empty bounds return `0`.
pub fn panel_tab_insertion_index(tab_bounds: &[Bounds<Pixels>], pointer_x: Pixels) -> usize {
  tab_bounds
    .iter()
    .position(|bounds| pointer_x < bounds.origin.x + bounds.size.width / 2.0)
    .unwrap_or(tab_bounds.len())
}
