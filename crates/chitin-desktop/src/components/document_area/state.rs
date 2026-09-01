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
use chitin_wgpu::AtomRepresentation;
use gpui::{AnyView, App, Bounds, Pixels, Window};

/// Callback that applies an atom representation to one WGPU document view.
type AtomRepresentationChangeHandler = dyn Fn(AtomRepresentation, &mut App);

/// Result produced when a WGPU document factory creates a fresh view.
pub struct WgpuDocumentView {
  /// Type-erased GPUI view rendered inside the panel body.
  view: AnyView,
  /// Optional molecule-specific representation controller.
  atom_representation: Option<WgpuAtomRepresentationControl>,
}

/// Mutable atom-representation state and its view update callback.
#[derive(Clone)]
pub(crate) struct WgpuAtomRepresentationControl {
  /// Representation currently selected for this independent panel view.
  representation: AtomRepresentation,
  /// Callback that invalidates the scene renderer after a selection change.
  on_change: Rc<AtomRepresentationChangeHandler>,
}

type FreshWgpuSurfaceCallback = Rc<dyn Fn(&Path, &mut Window, &mut App) -> WgpuDocumentView>;

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
    /// Filesystem path represented by this structure viewport, when available.
    path: Option<PathBuf>,
    /// Display title shown in the document tab strip.
    title: String,
    /// GPUI entity that owns the WGPU surface and interaction state.
    view: AnyView,
    /// Molecule representation state when this view supports atom rendering.
    atom_representation: Option<WgpuAtomRepresentationControl>,
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
  /// Panel whose document options popover is currently visible.
  pub(super) options_menu_panel_id: Option<PanelId>,
  /// Last prepainted bounds of the options trigger in window coordinates.
  pub(super) options_menu_anchor: Option<Bounds<Pixels>>,
}

impl OpenedProjectDocument {
  /// Creates an opened document descriptor from a filesystem path.
  ///
  /// # Parameters
  ///
  /// * `path` is the workspace file path opened from the project tree.
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

impl WgpuDocumentView {
  /// Creates a WGPU document view without molecule-specific controls.
  pub fn new(view: impl Into<AnyView>) -> Self {
    Self {
      view: view.into(),
      atom_representation: None,
    }
  }

  /// Creates a molecular WGPU view with an independently mutable representation.
  pub fn with_atom_representation(
    view: impl Into<AnyView>,
    representation: AtomRepresentation,
    on_change: impl Fn(AtomRepresentation, &mut App) + 'static,
  ) -> Self {
    Self {
      view: view.into(),
      atom_representation: Some(WgpuAtomRepresentationControl {
        representation,
        on_change: Rc::new(on_change),
      }),
    }
  }

  /// Applies a copied representation to a newly created independent view.
  fn select_atom_representation(&mut self, representation: AtomRepresentation, cx: &mut App) {
    let Some(control) = self.atom_representation.as_mut() else {
      return;
    };
    if let Some(on_change) = control.select(representation) {
      on_change(representation, cx);
    }
  }
}

impl From<AnyView> for WgpuDocumentView {
  fn from(view: AnyView) -> Self {
    Self::new(view)
  }
}

impl fmt::Debug for WgpuAtomRepresentationControl {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("WgpuAtomRepresentationControl")
      .field("representation", &self.representation)
      .finish_non_exhaustive()
  }
}

impl PartialEq for WgpuAtomRepresentationControl {
  fn eq(&self, other: &Self) -> bool {
    self.representation == other.representation && Rc::ptr_eq(&self.on_change, &other.on_change)
  }
}

impl Eq for WgpuAtomRepresentationControl {}

impl WgpuAtomRepresentationControl {
  /// Selects a new representation and returns the callback that applies it.
  fn select(&mut self, representation: AtomRepresentation) -> Option<Rc<AtomRepresentationChangeHandler>> {
    if self.representation == representation {
      return None;
    }
    self.representation = representation;
    Some(Rc::clone(&self.on_change))
  }
}

impl WgpuDocumentViewFactory {
  /// Creates a factory for independent WGPU document views.
  ///
  /// # Parameters
  ///
  /// * `build` creates a fresh GPUI view from the current window and app context.
  ///
  /// # Returns
  ///
  /// A cloneable factory handle suitable for storing in tab payloads.
  pub fn new<View>(build: impl Fn(&mut Window, &mut App) -> View + 'static) -> Self
  where
    View: Into<WgpuDocumentView>,
  {
    Self {
      build: Rc::new(move |_, window, cx| build(window, cx).into()),
    }
  }

  /// Creates a factory that builds a WGPU view for each opened structure path.
  pub fn new_for_document<View>(build: impl Fn(&Path, &mut Window, &mut App) -> View + 'static) -> Self
  where
    View: Into<WgpuDocumentView>,
  {
    Self {
      build: Rc::new(move |path, window, cx| build(path, window, cx).into()),
    }
  }

  /// Builds a fresh WGPU document view.
  ///
  /// # Parameters
  ///
  /// * `window` owns the GPUI surface allocation API.
  /// * `cx` creates the GPUI entity that renders the surface.
  ///
  /// # Returns
  ///
  /// A new [`WgpuDocumentView`] with independent surface and control state.
  pub fn build(&self, window: &mut Window, cx: &mut App) -> WgpuDocumentView {
    (self.build)(Path::new(""), window, cx)
  }

  /// Builds a WGPU view for a specific opened document path.
  pub fn build_for_document(&self, path: &Path, window: &mut Window, cx: &mut App) -> WgpuDocumentView {
    (self.build)(path, window, cx)
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
  /// * `document` is the opened project file descriptor.
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
  /// * `title` is the tab-strip label for the WGPU panel.
  /// * `view` is the GPUI entity that renders and owns WGPU interaction state.
  ///
  /// # Returns
  ///
  /// A [`DocumentPanelContent`] value that can be inserted into the panel tree.
  #[allow(dead_code)]
  pub(crate) fn wgpu_interactive(
    path: Option<PathBuf>,
    title: impl Into<String>,
    document_view: WgpuDocumentView,
    clone_view: WgpuDocumentViewFactory,
  ) -> Self {
    Self::WgpuInteractive {
      path,
      title: title.into(),
      view: document_view.view,
      atom_representation: document_view.atom_representation,
      clone_view,
    }
  }

  /// Returns the tab title for this document-panel content.
  pub(crate) fn title(&self) -> &str {
    match self {
      Self::ProjectDocument(document) => document.title.as_str(),
      Self::WgpuInteractive { title, .. } => title.as_str(),
    }
  }

  /// Returns the project document payload when this tab holds one.
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
  /// * `path` is the project path being opened or focused.
  ///
  /// # Returns
  ///
  /// `true` when this tab already represents `path`; otherwise `false`.
  fn matches_project_path(&self, path: &Path) -> bool {
    match self {
      Self::ProjectDocument(document) => document.path == path,
      Self::WgpuInteractive {
        path: Some(document_path),
        ..
      } => document_path == path,
      Self::WgpuInteractive { path: None, .. } => false,
    }
  }

  /// Returns the selected atom representation for molecular content.
  fn atom_representation(&self) -> Option<AtomRepresentation> {
    match self {
      Self::WgpuInteractive {
        atom_representation: Some(control),
        ..
      } => Some(control.representation),
      Self::ProjectDocument(_) | Self::WgpuInteractive { .. } => None,
    }
  }

  /// Updates molecular selection state and returns its scene callback.
  fn select_atom_representation(
    &mut self,
    representation: AtomRepresentation,
  ) -> Option<Rc<AtomRepresentationChangeHandler>> {
    let Self::WgpuInteractive {
      atom_representation: Some(control),
      ..
    } = self
    else {
      return None;
    };
    control.select(representation)
  }

  /// Creates an independent payload for a split panel.
  ///
  /// # Parameters
  ///
  /// * `window` owns any surface allocation required by WGPU payloads.
  /// * `cx` creates any GPUI entities required by cloned payloads.
  ///
  /// # Returns
  ///
  /// A payload that can be inserted into the newly split panel.
  pub(crate) fn clone_for_split(&self, window: &mut Window, cx: &mut App) -> Self {
    match self {
      Self::ProjectDocument(document) => Self::ProjectDocument(document.clone()),
      Self::WgpuInteractive {
        path,
        title,
        atom_representation,
        clone_view,
        ..
      } => {
        let mut document_view = match path.as_deref() {
          Some(path) => clone_view.build_for_document(path, window, cx),
          None => clone_view.build(window, cx),
        };
        if let Some(control) = atom_representation {
          document_view.select_atom_representation(control.representation, cx);
        }
        Self::WgpuInteractive {
          path: path.clone(),
          title: title.clone(),
          view: document_view.view,
          atom_representation: document_view.atom_representation,
          clone_view: clone_view.clone(),
        }
      }
    }
  }
}

impl DocumentPanelState {
  /// Creates empty document panel state with one root panel.
  pub(crate) fn empty() -> Self {
    Self {
      tree: PanelTree::single_leaf(PanelLeaf::new(DEFAULT_DOCUMENT_PANEL_ID)),
      focused_panel_id: DEFAULT_DOCUMENT_PANEL_ID,
      next_panel_id: FIRST_DYNAMIC_DOCUMENT_PANEL_ID,
      next_tab_id: FIRST_DYNAMIC_DOCUMENT_TAB_ID,
      resize_drag: None,
      tab_drag: None,
      tab_scroll: PanelTabScrollState::new(),
      options_menu_panel_id: None,
      options_menu_anchor: None,
    }
  }

  /// Creates document panel state with one opened document.
  ///
  /// # Parameters
  ///
  /// * `document` is the first document to show in the default root panel.
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
  /// * `content` is the first document-area payload to show in the root panel.
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
      options_menu_panel_id: None,
      options_menu_anchor: None,
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
  /// * `document` is the workspace file descriptor to focus or append.
  ///
  /// # Returns
  ///
  /// `true` when the document was focused or appended; otherwise `false`.
  pub(crate) fn open_document_as_tab(&mut self, document: OpenedProjectDocument) -> bool {
    let path = document.path.clone();
    self.open_content_as_tab(path.as_path(), DocumentPanelContent::project(document))
  }

  /// Opens arbitrary document content in the focused panel.
  pub(crate) fn open_content_as_tab(&mut self, path: &Path, content: DocumentPanelContent) -> bool {
    self.open_content_in_panel(self.focused_panel_id, path, content)
  }

  /// Opens content in a specific panel without changing the caller's target.
  pub(crate) fn open_content_in_panel(
    &mut self,
    panel_id: PanelId,
    path: &Path,
    content: DocumentPanelContent,
  ) -> bool {
    let Some(leaf) = self.tree.leaf(panel_id) else {
      return false;
    };

    if let Some(tab_id) = leaf
      .tabs
      .iter()
      .find(|tab| tab.payload.matches_project_path(path))
      .map(|tab| tab.id)
    {
      return self.activate_tab(panel_id, tab_id);
    }

    let tab_id = self.allocate_tab_id();
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

  /// Activates an already-open path in the focused panel without creating content.
  pub(crate) fn activate_path_in_focused_panel(&mut self, path: &Path) -> bool {
    let panel_id = self.focused_panel_id;
    let Some(tab_id) = self
      .tree
      .leaf(panel_id)
      .and_then(|leaf| leaf.tabs.iter().find(|tab| tab.payload.matches_project_path(path)))
      .map(|tab| tab.id)
    else {
      return false;
    };
    self.activate_tab(panel_id, tab_id)
  }

  /// Activates one tab inside one document panel.
  ///
  /// # Parameters
  ///
  /// * `panel_id` identifies the document panel whose active tab should change.
  /// * `tab_id` identifies the tab to activate.
  ///
  /// # Returns
  ///
  /// `true` when the panel and tab exist; otherwise `false`.
  pub(crate) fn activate_tab(&mut self, panel_id: PanelId, tab_id: PanelTabId) -> bool {
    if self.tree.activate_tab(panel_id, tab_id) {
      self.focused_panel_id = panel_id;
      self.options_menu_panel_id = None;
      self.options_menu_anchor = None;
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
  /// * `panel_id` identifies the document panel whose tab should close.
  /// * `tab_id` identifies the tab to close.
  ///
  /// # Returns
  ///
  /// `true` when the panel and tab exist and the tab was removed; otherwise
  /// `false`.
  pub(crate) fn close_tab(&mut self, panel_id: PanelId, tab_id: PanelTabId) -> bool {
    if !self.tree.close_tab(panel_id, tab_id) {
      return false;
    }
    self.options_menu_panel_id = None;
    self.options_menu_anchor = None;

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
  pub(crate) fn focus_previous_tab(&mut self) -> bool {
    let Some((panel_id, tab_id)) = self.relative_container_tab(-1) else {
      return false;
    };

    self.activate_tab(panel_id, tab_id)
  }

  /// Focuses the next tab in the focused document panel.
  pub(crate) fn focus_next_tab(&mut self) -> bool {
    let Some((panel_id, tab_id)) = self.relative_container_tab(1) else {
      return false;
    };

    self.activate_tab(panel_id, tab_id)
  }

  /// Closes the active tab in the focused document panel.
  pub(crate) fn close_focused_tab(&mut self) -> bool {
    let panel_id = self.focused_panel_id;
    let Some(tab_id) = self.tree.leaf(panel_id).and_then(|leaf| leaf.active_tab) else {
      return false;
    };

    self.close_tab(panel_id, tab_id)
  }

  /// Returns the first active document in panel-tree order.
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
  /// * `panel_id` identifies the panel whose active tab should be read.
  ///
  /// # Returns
  ///
  /// `Some(&DocumentPanelContent)` when the panel has an active tab.
  pub(crate) fn active_tab_payload(&self, panel_id: PanelId) -> Option<&DocumentPanelContent> {
    self.tree.leaf(panel_id)?.active_tab().map(|tab| &tab.payload)
  }

  /// Returns the active molecular representation in one panel.
  pub(crate) fn active_atom_representation(&self, panel_id: PanelId) -> Option<AtomRepresentation> {
    self.active_tab_payload(panel_id)?.atom_representation()
  }

  /// Changes the active molecular representation and returns its view callback.
  pub(crate) fn select_atom_representation(
    &mut self,
    panel_id: PanelId,
    representation: AtomRepresentation,
  ) -> Option<Rc<AtomRepresentationChangeHandler>> {
    let leaf = self.tree.leaf_mut(panel_id)?;
    let active_tab_id = leaf.active_tab?;
    let content = leaf
      .tabs
      .iter_mut()
      .find(|tab| tab.id == active_tab_id)
      .map(|tab| &mut tab.payload)?;
    content.select_atom_representation(representation)
  }

  /// Toggles the options menu for a molecular document panel.
  pub(crate) fn toggle_options_menu(&mut self, panel_id: PanelId) -> bool {
    if self.active_atom_representation(panel_id).is_none() {
      return false;
    }
    let opening = self.options_menu_panel_id != Some(panel_id);
    self.options_menu_panel_id = opening.then_some(panel_id);
    if !opening {
      self.options_menu_anchor = None;
    }
    true
  }

  /// Stores the latest trigger bounds used to anchor a deferred options popup.
  pub(crate) fn set_options_menu_anchor(&mut self, bounds: Bounds<Pixels>) -> bool {
    if self.options_menu_anchor == Some(bounds) {
      return false;
    }
    self.options_menu_anchor = Some(bounds);
    true
  }

  /// Dismisses the currently visible document options menu.
  pub(crate) fn dismiss_options_menu(&mut self) -> bool {
    let was_open = self.options_menu_panel_id.take().is_some();
    self.options_menu_anchor = None;
    was_open
  }

  /// Splits one document panel and copies its active tab to the new panel.
  ///
  /// The new panel receives a fresh panel id and starts with only the source
  /// panel's active tab. The source panel keeps its original tab stack and
  /// active-tab selection.
  ///
  /// # Parameters
  ///
  /// * `panel_id` identifies the source panel to split.
  /// * `axis` controls whether the new split is horizontal or vertical.
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
  /// * `panel_id` identifies the source panel to split.
  /// * `axis` controls whether the new split is horizontal or vertical.
  /// * `active_content` is the independent payload to install in the new panel.
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
  /// * `drag` contains stable source identifiers, the original index, and title.
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
  /// * `target` identifies the panel and raw visual insertion position under the
  ///   pointer.
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
  /// * `drag` is GPUI's stable payload for the released tab.
  /// * `target_panel_id` identifies the tab strip that accepted the drop.
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
  pub(crate) fn cancel_tab_drag(&mut self) -> bool {
    self.tab_drag.take().is_some()
  }

  /// Starts resizing a document panel split.
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
  /// * `current_position` is the latest cursor position on the resize axis.
  /// * `root_width` is the rendered width available to the document panel root.
  /// * `root_height` is the rendered height available to the document panel root.
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
  pub(crate) fn stop_resize(&mut self) -> bool {
    self.resize_drag.take().is_some()
  }

  /// Returns the active document panel resize axis.
  pub(crate) fn resize_axis(&self) -> Option<PanelSplitAxis> {
    self.resize_drag.as_ref().map(|resize_drag| resize_drag.anchor().axis)
  }

  /// Allocates the next document panel identifier.
  fn allocate_panel_id(&mut self) -> PanelId {
    let panel_id = self.next_panel_id;
    self.next_panel_id = PanelId::new(panel_id.value() + 1);
    panel_id
  }

  /// Allocates the next document tab identifier.
  fn allocate_tab_id(&mut self) -> PanelTabId {
    let tab_id = self.next_tab_id;
    self.next_tab_id = PanelTabId::new(tab_id.value() + 1);
    tab_id
  }

  /// Returns a tab adjacent to the focused active tab in container order.
  ///
  /// # Parameters
  ///
  /// * `offset` is `-1` for previous or `1` for next.
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

#[cfg(test)]
mod representation_tests {
  use super::*;

  fn stick_control() -> WgpuAtomRepresentationControl {
    WgpuAtomRepresentationControl {
      representation: AtomRepresentation::Stick,
      on_change: Rc::new(|_, _| {}),
    }
  }

  #[test]
  fn selecting_new_representation_should_update_control_and_return_callback() {
    let mut control = stick_control();

    let callback = control.select(AtomRepresentation::Sphere);

    assert_eq!(
      (control.representation, callback.is_some()),
      (AtomRepresentation::Sphere, true)
    );
  }

  #[test]
  fn selecting_current_representation_should_not_return_callback() {
    let mut control = stick_control();

    let callback = control.select(AtomRepresentation::Stick);

    assert!(callback.is_none());
  }
}
