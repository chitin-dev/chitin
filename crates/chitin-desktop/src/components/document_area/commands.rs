//! Document panel command adapters for the desktop app.

use chitin_ui::composite::panel::{
  PanelId, PanelSplitAxis, PanelSplitPath, PanelTabDrag, PanelTabDropTarget, PanelTabId,
};
use gpui::{Context, Pixels, Window};

use crate::{app::ChitinApp, components::document_area::state::OpenedProjectDocument};

impl ChitinApp {
  /// Opens a workspace document in the main document panel area.
  ///
  /// The first opened file creates the document panel tree. Later opened files
  /// are appended as new tabs in the focused document panel instead of
  /// replacing the existing panel layout.
  ///
  /// # Parameters
  ///
  /// * `document` is the file descriptor created from a project tree entry.
  ///
  /// # Returns
  ///
  /// This function returns `()` and mutates document panel state.
  pub(crate) fn open_project_document(&mut self, document: OpenedProjectDocument) {
    if !self.document_panels.open_document_as_tab(document.clone()) {
      self.document_panels = super::DocumentPanelState::new(document);
    }
  }

  /// Splits an existing document panel.
  ///
  /// # Parameters
  ///
  /// * `panel_id` identifies the document panel to split.
  /// * `axis` controls the orientation of the new split.
  ///
  /// # Returns
  ///
  /// `true` when the panel exists and was split; otherwise `false`.
  pub(crate) fn split_document_panel(
    &mut self,
    panel_id: PanelId,
    axis: PanelSplitAxis,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> bool {
    let active_content = self
      .document_panels
      .active_tab_payload(panel_id)
      .map(|content| content.clone_for_split(window, cx));
    self
      .document_panels
      .split_panel_with_content(panel_id, axis, active_content)
      .is_some()
  }

  /// Activates a tab inside a document panel.
  ///
  /// # Parameters
  ///
  /// * `panel_id` identifies the document panel that owns the tab.
  /// * `tab_id` identifies the tab to activate.
  ///
  /// # Returns
  ///
  /// `true` when the panel and tab exist; otherwise `false`.
  pub(crate) fn activate_document_panel_tab(&mut self, panel_id: PanelId, tab_id: PanelTabId) -> bool {
    self.document_panels.activate_tab(panel_id, tab_id)
  }

  /// Closes a tab inside a document panel.
  ///
  /// # Parameters
  ///
  /// * `panel_id` identifies the document panel that owns the tab.
  /// * `tab_id` identifies the tab to close.
  ///
  /// # Returns
  ///
  /// `true` when the panel and tab exist and the tab was removed; otherwise
  /// `false`.
  pub(crate) fn close_document_panel_tab(&mut self, panel_id: PanelId, tab_id: PanelTabId) -> bool {
    self.document_panels.close_tab(panel_id, tab_id)
  }

  /// Focuses the previous tab in the focused document panel.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows `self`.
  ///
  /// # Returns
  ///
  /// `true` when the active tab changed; otherwise `false`.
  pub(crate) fn focus_previous_document_panel_tab(&mut self) -> bool {
    self.document_panels.focus_previous_tab()
  }

  /// Focuses the next tab in the focused document panel.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows `self`.
  ///
  /// # Returns
  ///
  /// `true` when the active tab changed; otherwise `false`.
  pub(crate) fn focus_next_document_panel_tab(&mut self) -> bool {
    self.document_panels.focus_next_tab()
  }

  /// Closes the active tab in the focused document panel.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows `self`.
  ///
  /// # Returns
  ///
  /// `true` when an active tab was closed; otherwise `false`.
  pub(crate) fn close_focused_document_panel_tab(&mut self) -> bool {
    self.document_panels.close_focused_tab()
  }

  /// Starts a document tab drag after GPUI crosses its movement threshold.
  ///
  /// # Parameters
  ///
  /// * `drag` identifies the source panel, tab, index, and preview title.
  ///
  /// # Returns
  ///
  /// `true` when document panel state accepts the drag; otherwise `false`.
  pub(crate) fn start_document_panel_tab_drag(&mut self, drag: PanelTabDrag) -> bool {
    self.document_panels.start_tab_drag(drag)
  }

  /// Updates the current document tab insertion target.
  ///
  /// # Parameters
  ///
  /// * `target` identifies a valid panel tab strip and raw insertion index.
  ///
  /// # Returns
  ///
  /// `true` when temporary target state changed; otherwise `false`.
  pub(crate) fn update_document_panel_tab_drag_target(&mut self, target: PanelTabDropTarget) -> bool {
    self.document_panels.update_tab_drag_target(target)
  }

  /// Clears stale document tab insertion feedback before target hit testing.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows the app's document panel state.
  ///
  /// # Returns
  ///
  /// `true` when a previous insertion target was removed; otherwise `false`.
  pub(crate) fn clear_document_panel_tab_drag_target(&mut self) -> bool {
    self.document_panels.clear_tab_drag_target()
  }

  /// Commits a dragged document tab released over a valid target strip.
  ///
  /// # Parameters
  ///
  /// * `drag` is the stable GPUI drag payload.
  /// * `target_panel_id` identifies the strip accepting the drop.
  ///
  /// # Returns
  ///
  /// `true` when the tab move succeeds; otherwise `false`.
  pub(crate) fn drop_document_panel_tab(&mut self, drag: PanelTabDrag, target_panel_id: PanelId) -> bool {
    self.document_panels.drop_tab(drag, target_panel_id)
  }

  /// Cancels any uncommitted document tab drag.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows the app's document panel state.
  ///
  /// # Returns
  ///
  /// `true` when temporary drag state existed and was cleared; otherwise
  /// `false`.
  pub(crate) fn cancel_document_panel_tab_drag(&mut self) -> bool {
    self.document_panels.cancel_tab_drag()
  }

  /// Starts resizing one document panel split.
  ///
  /// # Parameters
  ///
  /// * `path` identifies the split node whose handle was pressed.
  /// * `axis` controls whether horizontal or vertical pointer movement is used.
  /// * `start_position` is the cursor position on the resize axis where the drag
  /// began.
  ///
  /// # Returns
  ///
  /// `true` when document panel state exists and accepted the resize start;
  /// otherwise `false`.
  pub(crate) fn start_document_panel_resize(
    &mut self,
    path: PanelSplitPath,
    axis: PanelSplitAxis,
    start_position: Pixels,
  ) -> bool {
    self.document_panels.start_resize(path, axis, start_position)
  }

  /// Updates the active document panel split resize.
  ///
  /// # Parameters
  ///
  /// * `current_position` is the latest cursor position on the active resize
  /// axis.
  /// * `root_width` is the rendered width available to the document panel root.
  /// * `root_height` is the rendered height available to the document panel root.
  ///
  /// # Returns
  ///
  /// `true` when an active document panel drag changed a split ratio; otherwise
  /// `false`.
  pub(crate) fn drag_document_panel_resize(
    &mut self,
    current_position: Pixels,
    root_width: Pixels,
    root_height: Pixels,
  ) -> bool {
    self
      .document_panels
      .drag_resize(current_position, root_width, root_height)
  }

  /// Stops the active document panel split resize.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows `self` to clear document panel resize state.
  ///
  /// # Returns
  ///
  /// `true` when a document panel resize drag was active and removed;
  /// otherwise `false`.
  pub(crate) fn stop_document_panel_resize(&mut self) -> bool {
    self.document_panels.stop_resize()
  }

  /// Returns the active document panel resize axis.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// `Some(PanelSplitAxis)` when a document panel resize drag is active;
  /// otherwise `None`.
  pub(crate) fn document_panel_resize_axis(&self) -> Option<PanelSplitAxis> {
    self.document_panels.resize_axis()
  }
}
