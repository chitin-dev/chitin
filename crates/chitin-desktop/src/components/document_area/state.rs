//! Document panel state and transition logic.

use std::{
  fmt,
  path::{Path, PathBuf},
  rc::Rc,
};

use chitin_ui::{
  composite::panel::{
    PanelId, PanelLeaf, PanelSplitAxis, PanelSplitPath, PanelSplitPlacement, PanelTab, PanelTabDrag, PanelTabDragState,
    PanelTabDropTarget, PanelTabId, PanelTabScrollState, PanelTree,
  },
  primitive::resize::ResizeGesture,
};
use gpui::{AnyView, App, Pixels, Window};

pub type FreshWgpuSurfaceCallback = Rc<dyn Fn(&mut Window, &mut App) -> AnyView>;

/// Factory used to create independent WGPU document views for split panels.
#[derive(Clone)]
pub struct WgpuDocumentViewFactory {
  /// Shared callback that creates a fresh WGPU surface-backed view.
  build: FreshWgpuSurfaceCallback,
}

/// Stable panel id used by the initial single document panel.
pub(super) const DEFAULT_DOCUMENT_PANEL_ID: PanelId = PanelId::new(1);
/// Stable tab id used while the desktop owns a single active document.
pub(super) const DEFAULT_DOCUMENT_TAB_ID: PanelTabId = PanelTabId::new(1);
/// Next panel id allocated after the default document panel.
pub(super) const FIRST_DYNAMIC_DOCUMENT_PANEL_ID: PanelId = PanelId::new(2);
/// Next tab id allocated after the default document tab.
pub(super) const FIRST_DYNAMIC_DOCUMENT_TAB_ID: PanelTabId = PanelTabId::new(2);

/// File document currently opened from the project workspace tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenedProjectDocument {
  /// Original filesystem path of the opened file.
  pub path: PathBuf,
  /// Display name shown in the document placeholder.
  pub title: String,
}

/// Content shown by one tab in the desktop document panel tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DocumentPanelContent {
  /// File document opened from the project workspace tree.
  ProjectDocument(OpenedProjectDocument),
  /// Experimental WGPU viewport hosted as a document-area panel.
  #[allow(dead_code)]
  WgpuInteractive {
    /// Display title shown in the document tab strip.
    title: String,
    /// GPUI entity that owns the WGPU surface and interaction state.
    view: AnyView,
    /// Callback that creates an independent view for split-panel clones.
    clone_view: WgpuDocumentViewFactory,
  },
}

/// Resized document panel split target.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocumentPanelResizeAnchor {
  /// Path to the split node being resized.
  path: PanelSplitPath,
  /// Axis of the split node being resized.
  axis: PanelSplitAxis,
}

/// State for document panels rendered in the main workbench area.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocumentPanelState {
  /// Binary panel tree containing document tab stacks.
  pub(crate) tree: PanelTree<DocumentPanelContent>,
  /// Current focused panel id.
  pub(crate) focused_panel_id: PanelId,
  /// Next panel identifier to allocate for a split leaf.
  pub(super) next_panel_id: PanelId,
  /// Next tab identifier to allocate for newly opened document tabs.
  pub(super) next_tab_id: PanelTabId,
  /// Active split resize drag, if the user is dragging a split handle.
  pub(super) resize_drag: Option<ResizeGesture<DocumentPanelResizeAnchor, f32>>,
  /// Temporary tab drag session kept separate from the persistent panel tree.
  pub(super) tab_drag: Option<PanelTabDragState>,
  /// Presentation-only scroll state for panel tab bars.
  pub(super) tab_scroll: PanelTabScrollState,
}

impl OpenedProjectDocument {
  /// Creates an opened document descriptor from a filesystem path.
  ///
  /// # Parameters
  ///
  /// `path` is the workspace file path opened from the project tree.
  ///
  /// # Returns
  ///
  /// An [`OpenedProjectDocument`] with the original path and a display title
  /// derived from the final path component.
  pub fn new(path: &Path) -> Self {
    Self {
      path: path.to_path_buf(),
      title: path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string()),
    }
  }
}

impl WgpuDocumentViewFactory {
  /// Creates a factory for independent WGPU document views.
  ///
  /// # Parameters
  ///
  /// `build` creates a fresh GPUI view from the current window and app context.
  ///
  /// # Returns
  ///
  /// A cloneable factory handle suitable for storing in tab payloads.
  pub fn new(build: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self {
    Self { build: Rc::new(build) }
  }

  /// Builds a fresh WGPU document view.
  ///
  /// # Parameters
  ///
  /// `window` owns the GPUI surface allocation API.
  ///
  /// `cx` creates the GPUI entity that renders the surface.
  ///
  /// # Returns
  ///
  /// A new [`AnyView`] with independent surface and entity state.
  pub fn build(&self, window: &mut Window, cx: &mut App) -> AnyView {
    (self.build)(window, cx)
  }
}

impl fmt::Debug for WgpuDocumentViewFactory {
  /// Formats the opaque factory for debug output.
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str("WgpuDocumentViewFactory")
  }
}

impl PartialEq for WgpuDocumentViewFactory {
  /// Compares factory identity by shared callback allocation.
  fn eq(&self, other: &Self) -> bool {
    Rc::ptr_eq(&self.build, &other.build)
  }
}

impl Eq for WgpuDocumentViewFactory {}

impl DocumentPanelContent {
  /// Creates document-panel content for a workspace file.
  ///
  /// # Parameters
  ///
  /// `document` is the opened project file descriptor.
  ///
  /// # Returns
  ///
  /// A [`DocumentPanelContent`] value that renders with the project document
  /// placeholder until a real editor is available.
  pub(crate) fn project(document: OpenedProjectDocument) -> Self {
    Self::ProjectDocument(document)
  }

  /// Creates document-panel content for the experimental WGPU viewport.
  ///
  /// # Parameters
  ///
  /// `title` is the tab-strip label for the WGPU panel.
  ///
  /// `view` is the GPUI entity that renders and owns WGPU interaction state.
  ///
  /// # Returns
  ///
  /// A [`DocumentPanelContent`] value that can be inserted into the panel tree.
  #[allow(dead_code)]
  pub(crate) fn wgpu_interactive(title: impl Into<String>, view: AnyView, clone_view: WgpuDocumentViewFactory) -> Self {
    Self::WgpuInteractive {
      title: title.into(),
      view,
      clone_view,
    }
  }

  /// Returns the tab title for this document-panel content.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// A string slice suitable for the document panel tab strip.
  pub(crate) fn title(&self) -> &str {
    match self {
      Self::ProjectDocument(document) => document.title.as_str(),
      Self::WgpuInteractive { title, .. } => title.as_str(),
    }
  }

  /// Returns the project document payload when this tab holds one.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// `Some(&OpenedProjectDocument)` for project-file tabs; otherwise `None`.
  #[cfg(test)]
  fn project_document(&self) -> Option<&OpenedProjectDocument> {
    match self {
      Self::ProjectDocument(document) => Some(document),
      Self::WgpuInteractive { .. } => None,
    }
  }

  /// Reports whether this content is a project document with the given path.
  ///
  /// # Parameters
  ///
  /// `path` is the project path being opened or focused.
  ///
  /// # Returns
  ///
  /// `true` when this tab already represents `path`; otherwise `false`.
  fn matches_project_path(&self, path: &Path) -> bool {
    match self {
      Self::ProjectDocument(document) => document.path == path,
      Self::WgpuInteractive { .. } => false,
    }
  }

  /// Creates an independent payload for a split panel.
  ///
  /// # Parameters
  ///
  /// `window` owns any surface allocation required by WGPU payloads.
  ///
  /// `cx` creates any GPUI entities required by cloned payloads.
  ///
  /// # Returns
  ///
  /// A payload that can be inserted into the newly split panel.
  pub(crate) fn clone_for_split(&self, window: &mut Window, cx: &mut App) -> Self {
    match self {
      Self::ProjectDocument(document) => Self::ProjectDocument(document.clone()),
      Self::WgpuInteractive { title, clone_view, .. } => Self::WgpuInteractive {
        title: title.clone(),
        view: clone_view.build(window, cx),
        clone_view: clone_view.clone(),
      },
    }
  }
}

impl DocumentPanelState {
  /// Creates empty document panel state with one root panel.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A [`DocumentPanelState`] with one empty leaf panel that can render the
  /// main workbench empty state before any project file is opened.
  pub(crate) fn empty() -> Self {
    Self {
      tree: PanelTree::single_leaf(PanelLeaf::new(DEFAULT_DOCUMENT_PANEL_ID)),
      focused_panel_id: DEFAULT_DOCUMENT_PANEL_ID,
      next_panel_id: FIRST_DYNAMIC_DOCUMENT_PANEL_ID,
      next_tab_id: FIRST_DYNAMIC_DOCUMENT_TAB_ID,
      resize_drag: None,
      tab_drag: None,
      tab_scroll: PanelTabScrollState::new(),
    }
  }

  /// Creates document panel state with one opened document.
  ///
  /// # Parameters
  ///
  /// `document` is the first document to show in the default root panel.
  ///
  /// # Returns
  ///
  /// A [`DocumentPanelState`] with a single leaf panel and one active tab.
  pub(crate) fn new(document: OpenedProjectDocument) -> Self {
    Self::with_content(DocumentPanelContent::project(document))
  }

  /// Creates document panel state with one arbitrary content tab.
  ///
  /// # Parameters
  ///
  /// `content` is the first document-area payload to show in the root panel.
  ///
  /// # Returns
  ///
  /// A [`DocumentPanelState`] with a single leaf panel and one active tab.
  pub(crate) fn with_content(content: DocumentPanelContent) -> Self {
    let title = content.title().to_string();
    Self {
      tree: PanelTree::single_leaf(PanelLeaf::new(DEFAULT_DOCUMENT_PANEL_ID).tab(PanelTab::new(
        DEFAULT_DOCUMENT_TAB_ID,
        title,
        content,
      ))),
      focused_panel_id: DEFAULT_DOCUMENT_PANEL_ID,
      next_panel_id: FIRST_DYNAMIC_DOCUMENT_PANEL_ID,
      next_tab_id: FIRST_DYNAMIC_DOCUMENT_TAB_ID,
      resize_drag: None,
      tab_drag: None,
      tab_scroll: PanelTabScrollState::new(),
    }
  }

  /// Opens a document in the focused panel.
  ///
  /// When the focused panel already contains a tab for `document.path`, this
  /// function activates that existing tab instead of appending a duplicate.
  /// Otherwise it appends a new tab and activates it.
  ///
  /// # Parameters
  ///
  /// `document` is the workspace file descriptor to focus or append.
  ///
  /// # Returns
  ///
  /// `true` when the document was focused or appended; otherwise `false`.
  pub(crate) fn open_document_as_tab(&mut self, document: OpenedProjectDocument) -> bool {
    let panel_id = self.focused_panel_id;
    let Some(leaf) = self.tree.leaf(panel_id) else {
      return false;
    };

    if let Some(tab_id) = leaf
      .tabs
      .iter()
      .find(|tab| tab.payload.matches_project_path(&document.path))
      .map(|tab| tab.id)
    {
      return self.activate_tab(panel_id, tab_id);
    }

    let tab_id = self.allocate_tab_id();
    let content = DocumentPanelContent::project(document);
    let title = content.title().to_string();
    let tab = PanelTab::new(tab_id, title, content);
    let Some(leaf) = self.tree.leaf_mut(panel_id) else {
      return false;
    };

    leaf.add_tab(tab);
    leaf.activate_tab(tab_id);
    self.focused_panel_id = panel_id;
    true
  }

  /// Activates one tab inside one document panel.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the document panel whose active tab should change.
  ///
  /// `tab_id` identifies the tab to activate.
  ///
  /// # Returns
  ///
  /// `true` when the panel and tab exist; otherwise `false`.
  pub(crate) fn activate_tab(&mut self, panel_id: PanelId, tab_id: PanelTabId) -> bool {
    if self.tree.activate_tab(panel_id, tab_id) {
      self.focused_panel_id = panel_id;
      return true;
    }
    false
  }

  /// Closes one tab inside one document panel.
  ///
  /// If closing the tab leaves a non-final panel empty, the empty panel is
  /// removed and its parent split is collapsed. The final remaining panel is
  /// kept so the document area can render its empty state.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the document panel whose tab should close.
  ///
  /// `tab_id` identifies the tab to close.
  ///
  /// # Returns
  ///
  /// `true` when the panel and tab exist and the tab was removed; otherwise
  /// `false`.
  pub(crate) fn close_tab(&mut self, panel_id: PanelId, tab_id: PanelTabId) -> bool {
    if !self.tree.close_tab(panel_id, tab_id) {
      return false;
    }

    let panel_is_empty = self
      .tree
      .leaf(panel_id)
      .map(|leaf| leaf.tabs.is_empty())
      .unwrap_or(false);
    if panel_is_empty && self.tree.leaf_count() > 1 {
      if let Some(focused_panel_id) = self.tree.remove_leaf(panel_id) {
        self.focused_panel_id = focused_panel_id;
      }
    } else {
      self.focused_panel_id = panel_id;
    }

    true
  }

  /// Focuses the previous tab in the focused document panel.
  ///
  /// Navigation is circular across every tab in the panel container's visual
  /// tree order.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows `self`.
  ///
  /// # Returns
  ///
  /// `true` when another tab was activated; otherwise `false`.
  pub(crate) fn focus_previous_tab(&mut self) -> bool {
    let Some((panel_id, tab_id)) = self.relative_container_tab(-1) else {
      return false;
    };

    self.activate_tab(panel_id, tab_id)
  }

  /// Focuses the next tab in the focused document panel.
  ///
  /// Navigation is circular across every tab in the panel container's visual
  /// tree order.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows `self`.
  ///
  /// # Returns
  ///
  /// `true` when another tab was activated; otherwise `false`.
  pub(crate) fn focus_next_tab(&mut self) -> bool {
    let Some((panel_id, tab_id)) = self.relative_container_tab(1) else {
      return false;
    };

    self.activate_tab(panel_id, tab_id)
  }

  /// Closes the active tab in the focused document panel.
  ///
  /// The underlying panel tree repairs active-tab selection by preferring the
  /// right neighbor, then the left neighbor when no right neighbor exists.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows `self`.
  ///
  /// # Returns
  ///
  /// `true` when an active tab existed and was closed; otherwise `false`.
  pub(crate) fn close_focused_tab(&mut self) -> bool {
    let panel_id = self.focused_panel_id;
    let Some(tab_id) = self.tree.leaf(panel_id).and_then(|leaf| leaf.active_tab) else {
      return false;
    };

    self.close_tab(panel_id, tab_id)
  }

  /// Returns the first active document in panel-tree order.
  ///
  /// This helper is intentionally read-only and primarily supports places that
  /// still need a simple "current document" view while the richer panel model
  /// is being introduced.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// `Some(&OpenedProjectDocument)` when any panel has an active tab;
  /// otherwise `None`.
  #[cfg(test)]
  pub(crate) fn active_document(&self) -> Option<&OpenedProjectDocument> {
    self
      .tree
      .first_active_tab()
      .and_then(|(_, tab)| tab.payload.project_document())
  }

  /// Returns the active tab payload for one document panel.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the panel whose active tab should be read.
  ///
  /// # Returns
  ///
  /// `Some(&DocumentPanelContent)` when the panel has an active tab.
  pub(crate) fn active_tab_payload(&self, panel_id: PanelId) -> Option<&DocumentPanelContent> {
    self.tree.leaf(panel_id)?.active_tab().map(|tab| &tab.payload)
  }

  /// Splits one document panel and copies its active tab to the new panel.
  ///
  /// The new panel receives a fresh panel id and starts with only the source
  /// panel's active tab. The source panel keeps its original tab stack and
  /// active-tab selection.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the source panel to split.
  ///
  /// `axis` controls whether the new split is horizontal or vertical.
  ///
  /// # Returns
  ///
  /// `Some(PanelId)` with the new panel id when splitting succeeds; otherwise
  /// `None`.
  #[cfg(test)]
  pub(crate) fn split_panel(&mut self, panel_id: PanelId, axis: PanelSplitAxis) -> Option<PanelId> {
    let active_content = self.tree.leaf(panel_id)?.active_tab().map(|tab| tab.payload.clone());
    self.split_panel_with_content(panel_id, axis, active_content)
  }

  /// Splits one document panel with a caller-provided active-tab payload copy.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the source panel to split.
  ///
  /// `axis` controls whether the new split is horizontal or vertical.
  ///
  /// `active_content` is the independent payload to install in the new panel.
  ///
  /// # Returns
  ///
  /// `Some(PanelId)` with the new panel id when splitting succeeds; otherwise
  /// `None`.
  pub(crate) fn split_panel_with_content(
    &mut self,
    panel_id: PanelId,
    axis: PanelSplitAxis,
    active_content: Option<DocumentPanelContent>,
  ) -> Option<PanelId> {
    self.tree.leaf(panel_id)?;
    // this equals to if self.tree.leaf(panel_id).is_none() { return None; }

    let new_panel_id = self.allocate_panel_id();
    let mut new_leaf = PanelLeaf::new(new_panel_id);

    if let Some(active_content) = active_content {
      let title = active_content.title().to_string();
      let tab = PanelTab::new(self.allocate_tab_id(), title, active_content);
      new_leaf.add_tab(tab);
    }

    let split = self
      .tree
      .split_leaf(panel_id, axis, new_leaf, PanelSplitPlacement::After, 0.5)
      .then_some(new_panel_id);

    if split.is_some() {
      self.focused_panel_id = new_panel_id;
    }

    split
  }

  /// Starts a native GPUI tab drag and activates its source tab.
  ///
  /// # Parameters
  ///
  /// `drag` contains stable source identifiers, the original index, and title.
  ///
  /// # Returns
  ///
  /// `true` when the source panel and tab still match the payload; otherwise
  /// `false`.
  pub(crate) fn start_tab_drag(&mut self, drag: PanelTabDrag) -> bool {
    let source_matches = self
      .tree
      .leaf(drag.source_panel_id)
      .and_then(|leaf| leaf.tabs.get(drag.source_index))
      .is_some_and(|tab| tab.id == drag.tab_id);
    if !source_matches || !self.activate_tab(drag.source_panel_id, drag.tab_id) {
      return false;
    }

    self.tab_drag = Some(PanelTabDragState {
      drag,
      drop_target: None,
    });
    true
  }

  /// Updates the proposed insertion target for an active tab drag.
  ///
  /// # Parameters
  ///
  /// `target` identifies the panel and raw visual insertion position under the
  /// pointer.
  ///
  /// # Returns
  ///
  /// `true` when a valid target changed; otherwise `false`.
  pub(crate) fn update_tab_drag_target(&mut self, target: PanelTabDropTarget) -> bool {
    if self.tree.leaf(target.panel_id).is_none() {
      return false;
    }
    let Some(tab_drag) = self.tab_drag.as_ref() else {
      return false;
    };
    let source_exists = self
      .tree
      .find_tab(tab_drag.drag.source_panel_id, tab_drag.drag.tab_id)
      .is_some();
    if !source_exists || tab_drag.drop_target == Some(target) {
      return false;
    }

    if let Some(tab_drag) = self.tab_drag.as_mut() {
      tab_drag.drop_target = Some(target);
    }
    true
  }

  /// Clears the current insertion target while preserving the active drag.
  ///
  /// The workbench root invokes this at the start of each native drag-move
  /// event. A tab strip later in event propagation replaces it with a valid
  /// target when the pointer is inside that strip.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows the temporary drag session.
  ///
  /// # Returns
  ///
  /// `true` when an existing target was cleared; otherwise `false`.
  pub(crate) fn clear_tab_drag_target(&mut self) -> bool {
    let Some(tab_drag) = self.tab_drag.as_mut() else {
      return false;
    };

    tab_drag.drop_target.take().is_some()
  }

  /// Commits an active tab drag through the panel model's atomic move API.
  ///
  /// The temporary drag state is always cleared before validation, so stale or
  /// invalid payloads cannot leak feedback into a later interaction.
  ///
  /// # Parameters
  ///
  /// `drag` is GPUI's stable payload for the released tab.
  ///
  /// `target_panel_id` identifies the tab strip that accepted the drop.
  ///
  /// # Returns
  ///
  /// `true` when the payload and current target match and the model accepts the
  /// move; otherwise `false`.
  pub(crate) fn drop_tab(&mut self, drag: PanelTabDrag, target_panel_id: PanelId) -> bool {
    let Some(tab_drag) = self.tab_drag.take() else {
      return false;
    };
    let Some(target) = tab_drag.drop_target else {
      return false;
    };
    if tab_drag.drag != drag || target.panel_id != target_panel_id {
      return false;
    }
    if !self.tree.move_tab(drag.source_panel_id, drag.tab_id, target) {
      return false;
    }

    self.focused_panel_id = target.panel_id;
    true
  }

  /// Cancels the current tab drag without mutating the panel tree.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows `self` to clear temporary interaction state.
  ///
  /// # Returns
  ///
  /// `true` when a drag session existed and was removed; otherwise `false`.
  pub(crate) fn cancel_tab_drag(&mut self) -> bool {
    self.tab_drag.take().is_some()
  }

  /// Starts resizing a document panel split.
  ///
  /// # Parameters
  ///
  /// `path` identifies the split node whose handle was pressed.
  ///
  /// `axis` controls whether horizontal or vertical pointer movement is used.
  ///
  /// `start_position` is the cursor position on the resize axis where the drag
  /// began.
  ///
  /// # Returns
  ///
  /// `true` when the split path exists and resize state was recorded;
  /// otherwise `false`.
  pub(crate) fn start_resize(&mut self, path: PanelSplitPath, axis: PanelSplitAxis, start_position: Pixels) -> bool {
    let Some(start_ratio) = self.tree.split_ratio(&path) else {
      return false;
    };

    self.resize_drag = Some(ResizeGesture::new(
      DocumentPanelResizeAnchor { path, axis },
      start_position,
      start_ratio,
    ));
    true
  }

  /// Updates a document panel split from the current pointer position.
  ///
  /// # Parameters
  ///
  /// `current_position` is the latest cursor position on the resize axis.
  ///
  /// `root_width` is the rendered width available to the document panel root.
  ///
  /// `root_height` is the rendered height available to the document panel root.
  ///
  /// # Returns
  ///
  /// `true` when an active drag changed a split ratio; otherwise `false`.
  pub(crate) fn drag_resize(&mut self, current_position: Pixels, root_width: Pixels, root_height: Pixels) -> bool {
    let Some(resize_drag) = &self.resize_drag else {
      return false;
    };
    let Some(available_size) = self
      .tree
      .split_axis_size(&resize_drag.anchor().path, root_width, root_height)
    else {
      return false;
    };
    let available_size = f32::from(available_size);

    if available_size <= 0.0 {
      return false;
    }

    let ratio = resize_drag.start_value() + resize_drag.delta(current_position) / available_size;
    self.tree.resize_split(&resize_drag.anchor().path, ratio)
  }

  /// Stops the active document panel split resize gesture.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows `self` to clear resize state.
  ///
  /// # Returns
  ///
  /// `true` when a resize drag was active and removed; otherwise `false`.
  pub(crate) fn stop_resize(&mut self) -> bool {
    self.resize_drag.take().is_some()
  }

  /// Returns the active document panel resize axis.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// `Some(PanelSplitAxis)` when a resize drag is active; otherwise `None`.
  pub(crate) fn resize_axis(&self) -> Option<PanelSplitAxis> {
    self.resize_drag.as_ref().map(|resize_drag| resize_drag.anchor().axis)
  }

  /// Allocates the next document panel identifier.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows `self`.
  ///
  /// # Returns
  ///
  /// A fresh [`PanelId`] for a document panel leaf.
  fn allocate_panel_id(&mut self) -> PanelId {
    let panel_id = self.next_panel_id;
    self.next_panel_id = PanelId::new(panel_id.value() + 1);
    panel_id
  }

  /// Allocates the next document tab identifier.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows `self`.
  ///
  /// # Returns
  ///
  /// A fresh [`PanelTabId`] for a document tab.
  fn allocate_tab_id(&mut self) -> PanelTabId {
    let tab_id = self.next_tab_id;
    self.next_tab_id = PanelTabId::new(tab_id.value() + 1);
    tab_id
  }

  /// Returns a tab adjacent to the focused active tab in container order.
  ///
  /// # Parameters
  ///
  /// `offset` is `-1` for previous or `1` for next.
  ///
  /// # Returns
  ///
  /// `Some((PanelId, PanelTabId))` for the circular adjacent tab across all
  /// panels, or `None` when the container has no tabs.
  fn relative_container_tab(&self, offset: isize) -> Option<(PanelId, PanelTabId)> {
    let tabs = self.tree.tabs_in_order();
    let tab_count = tabs.len();

    if tab_count == 0 {
      return None;
    }

    let current_tab = self
      .tree
      .leaf(self.focused_panel_id)
      .and_then(|leaf| leaf.active_tab)
      .or_else(|| self.tree.first_active_tab().map(|(_, tab)| tab.id))?;
    let current_index = tabs
      .iter()
      .position(|(panel_id, tab_id)| *panel_id == self.focused_panel_id && *tab_id == current_tab)
      .or_else(|| tabs.iter().position(|(_, tab_id)| *tab_id == current_tab))?;
    let next_index = if offset < 0 {
      current_index.checked_sub(1).unwrap_or(tab_count - 1)
    } else {
      (current_index + 1) % tab_count
    };

    Some(tabs[next_index])
  }
}
