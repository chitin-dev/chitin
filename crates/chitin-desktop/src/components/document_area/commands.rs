//! Document panel command adapters for the desktop app.

use std::sync::Arc;

use chitin_molecule_renderer::AtomRepresentation;
use chitin_ui::composite::panel::{
  PanelId, PanelSplitAxis, PanelSplitPath, PanelTabDrag, PanelTabDropTarget, PanelTabId,
};
use gpui::{App, AppContext, AsyncApp, Context, Pixels, WeakEntity, Window};

use crate::{
  app::ChitinApp,
  components::document_area::{
    state::{DocumentPanelContent, OpenedProjectDocument, WgpuDocumentViewFactory},
    structure::{build_structure_view_from_scene, load_structure_scene},
  },
};

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

  /// Opens a workspace document, creating a structure viewport for supported files.
  pub(crate) fn open_project_document_with_window(
    &mut self,
    document: OpenedProjectDocument,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let is_structure = document
      .path
      .extension()
      .and_then(|extension| extension.to_str())
      .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "pdb" | "ent" | "cif" | "mmcif"));
    if is_structure {
      if self.document_panels.activate_path_in_focused_panel(&document.path) {
        return;
      }
      self.open_structure_document(document, window, cx);
    } else if !self.document_panels.open_document_as_tab(document.clone()) {
      self.document_panels = super::DocumentPanelState::new(document);
    }
  }

  /// Starts background parsing for a structure document and installs completed views on the UI thread.
  fn open_structure_document(&mut self, document: OpenedProjectDocument, window: &mut Window, cx: &mut Context<Self>) {
    let path = document.path.clone();
    if !self.pending_structure_loads.insert(path.clone()) {
      // The original request will install the completed tab when parsing ends.
      return;
    }

    let panel_id = self.document_panels.focused_panel_id;
    let window_handle = window.window_handle();
    cx.spawn(async move |app: WeakEntity<ChitinApp>, async_cx: &mut AsyncApp| {
      let load_path = path.clone();
      let result = async_cx
        .background_executor()
        .spawn(async move { load_structure_scene(&load_path) })
        .await;
      let _ = async_cx.update_window(window_handle, |_, window, cx| {
        let _ = app.update(cx, |this, cx| {
          this.pending_structure_loads.remove(&path);
          match result {
            Ok(scene) => this.install_loaded_structure(panel_id, &document, scene, window, cx),
            Err(error) => {
              log::error!("failed to load structure '{}': {error}", path.display());
            }
          }
          cx.notify();
        });
      });
    })
    .detach();
  }

  /// Installs a parsed structure as an independent WGPU view in its target panel.
  fn install_loaded_structure(
    &mut self,
    panel_id: PanelId,
    document: &OpenedProjectDocument,
    scene: Arc<chitin_bio::structure::StructureScene>,
    window: &mut Window,
    cx: &mut App,
  ) {
    let content = structure_document_content(document, scene, window, cx);
    let focused_panel_id = self.document_panels.focused_panel_id;
    let opened = self
      .document_panels
      .open_content_in_panel(panel_id, &document.path, content);
    if opened {
      // Parsing completion must not steal focus from a panel the user selected
      // while the background task was running.
      self.document_panels.focused_panel_id = focused_panel_id;
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

  /// Toggles the active molecular document's options menu.
  pub(crate) fn toggle_document_options_menu(&mut self, panel_id: PanelId) -> bool {
    self.document_panels.toggle_options_menu(panel_id)
  }

  /// Dismisses the open molecular document options menu.
  pub(crate) fn dismiss_document_options_menu(&mut self) -> bool {
    self.document_panels.dismiss_options_menu()
  }

  /// Applies an atom representation to the active molecular document.
  pub(crate) fn select_document_atom_representation(
    &mut self,
    panel_id: PanelId,
    representation: AtomRepresentation,
    cx: &mut Context<Self>,
  ) -> bool {
    let Some(on_change) = self
      .document_panels
      .select_atom_representation(panel_id, representation)
    else {
      self.document_panels.dismiss_options_menu();
      return false;
    };
    self.document_panels.dismiss_options_menu();
    on_change(representation, cx);
    true
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
  pub(crate) fn focus_previous_document_panel_tab(&mut self) -> bool {
    self.document_panels.focus_previous_tab()
  }

  /// Focuses the next tab in the focused document panel.
  pub(crate) fn focus_next_document_panel_tab(&mut self) -> bool {
    self.document_panels.focus_next_tab()
  }

  /// Closes the active tab in the focused document panel.
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
  ///   began.
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
  ///   axis.
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
  pub(crate) fn stop_document_panel_resize(&mut self) -> bool {
    self.document_panels.stop_resize()
  }

  /// Returns the active document panel resize axis.
  pub(crate) fn document_panel_resize_axis(&self) -> Option<PanelSplitAxis> {
    self.document_panels.resize_axis()
  }
}

/// Creates one surface-backed panel payload while sharing parsed scene data with future splits.
fn structure_document_content(
  document: &OpenedProjectDocument,
  scene: Arc<chitin_bio::structure::StructureScene>,
  window: &mut Window,
  cx: &mut App,
) -> DocumentPanelContent {
  let document_view = build_structure_view_from_scene(Arc::clone(&scene), window, cx);
  let clone_view =
    WgpuDocumentViewFactory::new(move |window, cx| build_structure_view_from_scene(Arc::clone(&scene), window, cx));
  DocumentPanelContent::wgpu_interactive(
    Some(document.path.clone()),
    document.title.clone(),
    document_view,
    clone_view,
  )
}
