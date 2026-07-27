//! Main document area for opened workspace files.
//!
//! This module owns desktop-specific document presentation. It intentionally
//! starts with a placeholder view so file-tree activation can be wired before a
//! real editor, molecule viewer, or structure viewer is introduced.

use std::{
  path::{Path, PathBuf},
  rc::Rc,
};

use chitin_core::WorkspaceSummary;
use chitin_ui::{
  components::panel::{
    PanelContainerConfig, PanelId, PanelLeaf, PanelResizeConfig, PanelSplitAxis, PanelSplitPath,
    PanelSplitPlacement, PanelTab, PanelTabActivateHandler, PanelTabCloseHandler,
    PanelTabCloseIconRenderer, PanelTabDrag, PanelTabDragConfig, PanelTabDragStartHandler,
    PanelTabDragState, PanelTabDragTargetHandler, PanelTabDropHandler, PanelTabDropTarget,
    PanelTabId, PanelTree, render_panel_container,
  },
  themes::UIThemes,
};
use gpui::{
  AnyElement, App, Context, FocusHandle, FontWeight, InteractiveElement, IntoElement, MouseButton,
  ParentElement, Pixels, Styled, WeakEntity, Window, div, px, svg,
};

use crate::{
  app::ChitinApp,
  commands::{
    PanelTabCommand,
    tab::{CloseTab, FocusNextPanelTab, FocusPreviousPanelTab, PANEL_CONTAINER_KEY_CONTEXT},
  },
  components::activity_bar::ActiveActivity,
};

/// File document currently opened from the project workspace tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenedProjectDocument {
  /// Original filesystem path of the opened file.
  pub path: PathBuf,
  /// Display name shown in the document placeholder.
  pub title: String,
}

/// Stable panel id used by the initial single document panel.
const DEFAULT_DOCUMENT_PANEL_ID: PanelId = PanelId::new(1);
/// Stable tab id used while the desktop owns a single active document.
const DEFAULT_DOCUMENT_TAB_ID: PanelTabId = PanelTabId::new(1);
/// Next panel id allocated after the default document panel.
const FIRST_DYNAMIC_DOCUMENT_PANEL_ID: PanelId = PanelId::new(2);
/// Next tab id allocated after the default document tab.
const FIRST_DYNAMIC_DOCUMENT_TAB_ID: PanelTabId = PanelTabId::new(2);
/// Asset path for the horizontal split panel action.
const SPLIT_HORIZONTAL_ICON_PATH: &str = "icons/panel/codicon-split-horizontal.svg";
/// Asset path for the vertical split panel action.
const SPLIT_VERTICAL_ICON_PATH: &str = "icons/panel/codicon-split-vertical.svg";
/// Asset path for document tab close buttons.
const TAB_CLOSE_ICON_PATH: &str = "icons/panel/lucide-x.svg";
/// Size used by tab strip action icons.
const PANEL_ACTION_ICON_SIZE: Pixels = px(16.0);
/// Size used by close tab button icons.
const TAB_CLOSE_ICON_SIZE: Pixels = px(12.0);

/// Active resize gesture for a document panel split.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocumentPanelResizeDrag {
  /// Path to the split node being resized.
  path: PanelSplitPath,
  /// Axis of the split node being resized.
  axis: PanelSplitAxis,
  /// Cursor position on the resize axis when dragging started.
  start_position: Pixels,
  /// Split ratio when dragging started.
  start_ratio: f32,
}

/// State for document panels rendered in the main workbench area.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DocumentPanelState {
  /// Binary panel tree containing document tab stacks.
  pub(crate) tree: PanelTree<OpenedProjectDocument>,
  /// Current focused panel id.
  pub(crate) focused_panel_id: PanelId,
  /// Next panel identifier to allocate for a split leaf.
  next_panel_id: PanelId,
  /// Next tab identifier to allocate for newly opened document tabs.
  next_tab_id: PanelTabId,
  /// Active split resize drag, if the user is dragging a split handle.
  resize_drag: Option<DocumentPanelResizeDrag>,
  /// Temporary tab drag session kept separate from the persistent panel tree.
  tab_drag: Option<PanelTabDragState>,
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

impl DocumentPanelState {
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
    Self {
      tree: PanelTree::single_leaf(PanelLeaf::new(DEFAULT_DOCUMENT_PANEL_ID).tab(PanelTab::new(
        DEFAULT_DOCUMENT_TAB_ID,
        document.title.clone(),
        document,
      ))),
      focused_panel_id: DEFAULT_DOCUMENT_PANEL_ID,
      next_panel_id: FIRST_DYNAMIC_DOCUMENT_PANEL_ID,
      next_tab_id: FIRST_DYNAMIC_DOCUMENT_TAB_ID,
      resize_drag: None,
      tab_drag: None,
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
      .find(|tab| tab.payload.path == document.path)
      .map(|tab| tab.id)
    {
      return self.activate_tab(panel_id, tab_id);
    }

    let tab_id = self.allocate_tab_id();
    let tab = PanelTab::new(tab_id, document.title.clone(), document);
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
    active_document_in_node(&self.tree.root)
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
  pub(crate) fn split_panel(&mut self, panel_id: PanelId, axis: PanelSplitAxis) -> Option<PanelId> {
    let mut active_tab = self.tree.leaf(panel_id)?.active_tab().cloned();
    let new_panel_id = self.allocate_panel_id();
    let mut new_leaf = PanelLeaf::new(new_panel_id);

    if let Some(active_tab) = active_tab.as_mut() {
      active_tab.id = self.allocate_tab_id();
    }
    if let Some(active_tab) = active_tab {
      new_leaf.add_tab(active_tab);
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
      .leaf(tab_drag.drag.source_panel_id)
      .is_some_and(|leaf| leaf.tabs.iter().any(|tab| tab.id == tab_drag.drag.tab_id));
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
    if !self
      .tree
      .move_tab(drag.source_panel_id, drag.tab_id, target)
    {
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
  pub(crate) fn start_resize(
    &mut self,
    path: PanelSplitPath,
    axis: PanelSplitAxis,
    start_position: Pixels,
  ) -> bool {
    let Some(start_ratio) = self.tree.split_ratio(&path) else {
      return false;
    };

    self.resize_drag = Some(DocumentPanelResizeDrag {
      path,
      axis,
      start_position,
      start_ratio,
    });
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
  pub(crate) fn drag_resize(
    &mut self,
    current_position: Pixels,
    root_width: Pixels,
    root_height: Pixels,
  ) -> bool {
    let Some(resize_drag) = &self.resize_drag else {
      return false;
    };
    let Some(available_size) =
      self
        .tree
        .split_axis_size(&resize_drag.path, root_width, root_height)
    else {
      return false;
    };
    let available_size = f32::from(available_size);

    if available_size <= 0.0 {
      return false;
    }

    let delta = f32::from(current_position) - f32::from(resize_drag.start_position);
    let ratio = resize_drag.start_ratio + delta / available_size;
    self.tree.resize_split(&resize_drag.path, ratio)
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
    self
      .resize_drag
      .as_ref()
      .map(|resize_drag| resize_drag.axis)
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
    let tabs = container_tab_order(&self.tree.root);
    let tab_count = tabs.len();

    if tab_count == 0 {
      return None;
    }

    let current_tab = self
      .tree
      .leaf(self.focused_panel_id)
      .and_then(|leaf| leaf.active_tab)
      .or_else(|| first_active_container_tab(&self.tree.root).map(|(_, tab_id)| tab_id))?;
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

/// Returns every tab in panel-tree visual order.
///
/// # Parameters
///
/// `node` is the panel tree node to traverse.
///
/// # Returns
///
/// A vector of `(PanelId, PanelTabId)` pairs in render order.
fn container_tab_order(
  node: &chitin_ui::components::panel::PanelNode<OpenedProjectDocument>,
) -> Vec<(PanelId, PanelTabId)> {
  let mut tabs = Vec::new();
  collect_container_tab_order(node, &mut tabs);
  tabs
}

/// Appends every tab below one node to an existing order list.
///
/// # Parameters
///
/// `node` is the panel tree node to traverse.
///
/// `tabs` receives panel and tab identifiers in visual order.
///
/// # Returns
///
/// This function returns `()` after appending identifiers.
fn collect_container_tab_order(
  node: &chitin_ui::components::panel::PanelNode<OpenedProjectDocument>,
  tabs: &mut Vec<(PanelId, PanelTabId)>,
) {
  match node {
    chitin_ui::components::panel::PanelNode::Leaf(leaf) => {
      tabs.extend(leaf.tabs.iter().map(|tab| (leaf.id, tab.id)));
    }
    chitin_ui::components::panel::PanelNode::Split(split) => {
      collect_container_tab_order(&split.first, tabs);
      collect_container_tab_order(&split.second, tabs);
    }
  }
}

/// Finds the first active tab in panel-tree visual order.
///
/// # Parameters
///
/// `node` is the panel tree node to traverse.
///
/// # Returns
///
/// `Some((PanelId, PanelTabId))` for the first active tab, or `None` when no
/// panel has an active tab.
fn first_active_container_tab(
  node: &chitin_ui::components::panel::PanelNode<OpenedProjectDocument>,
) -> Option<(PanelId, PanelTabId)> {
  match node {
    chitin_ui::components::panel::PanelNode::Leaf(leaf) => {
      leaf.active_tab.map(|tab_id| (leaf.id, tab_id))
    }
    chitin_ui::components::panel::PanelNode::Split(split) => {
      first_active_container_tab(&split.first).or_else(|| first_active_container_tab(&split.second))
    }
  }
}

/// Finds the first active document payload below a panel node.
///
/// # Parameters
///
/// `node` is the panel node to search.
///
/// # Returns
///
/// `Some(&OpenedProjectDocument)` for the first active document in tree order,
/// or `None` when no active document exists.
#[cfg(test)]
fn active_document_in_node(
  node: &chitin_ui::components::panel::PanelNode<OpenedProjectDocument>,
) -> Option<&OpenedProjectDocument> {
  match node {
    chitin_ui::components::panel::PanelNode::Leaf(leaf) => {
      leaf.active_tab().map(|tab| &tab.payload)
    }
    chitin_ui::components::panel::PanelNode::Split(split) => {
      active_document_in_node(&split.first).or_else(|| active_document_in_node(&split.second))
    }
  }
}

impl ChitinApp {
  /// Opens a workspace document in the main document panel area.
  ///
  /// The first opened file creates the document panel tree. Later opened files
  /// are appended as new tabs in the focused document panel instead of
  /// replacing the existing panel layout.
  ///
  /// # Parameters
  ///
  /// `document` is the file descriptor created from a project tree entry.
  ///
  /// # Returns
  ///
  /// This function returns `()` and mutates document panel state.
  pub(crate) fn open_project_document(&mut self, document: OpenedProjectDocument) {
    match &mut self.document_panels {
      Some(document_panels) => {
        if !document_panels.open_document_as_tab(document.clone()) {
          *document_panels = DocumentPanelState::new(document);
        }
      }
      None => self.document_panels = Some(DocumentPanelState::new(document)),
    }
  }

  /// Splits an existing document panel.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the document panel to split.
  ///
  /// `axis` controls the orientation of the new split.
  ///
  /// # Returns
  ///
  /// `true` when the panel exists and was split; otherwise `false`.
  pub(crate) fn split_document_panel(&mut self, panel_id: PanelId, axis: PanelSplitAxis) -> bool {
    self
      .document_panels
      .as_mut()
      .and_then(|document_panels| document_panels.split_panel(panel_id, axis))
      .is_some()
  }

  /// Activates a tab inside a document panel.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the document panel that owns the tab.
  ///
  /// `tab_id` identifies the tab to activate.
  ///
  /// # Returns
  ///
  /// `true` when the panel and tab exist; otherwise `false`.
  pub(crate) fn activate_document_panel_tab(
    &mut self,
    panel_id: PanelId,
    tab_id: PanelTabId,
  ) -> bool {
    self
      .document_panels
      .as_mut()
      .map(|document_panels| document_panels.activate_tab(panel_id, tab_id))
      .unwrap_or(false)
  }

  /// Closes a tab inside a document panel.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the document panel that owns the tab.
  ///
  /// `tab_id` identifies the tab to close.
  ///
  /// # Returns
  ///
  /// `true` when the panel and tab exist and the tab was removed; otherwise
  /// `false`.
  pub(crate) fn close_document_panel_tab(&mut self, panel_id: PanelId, tab_id: PanelTabId) -> bool {
    self
      .document_panels
      .as_mut()
      .map(|document_panels| document_panels.close_tab(panel_id, tab_id))
      .unwrap_or(false)
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
    self
      .document_panels
      .as_mut()
      .map(DocumentPanelState::focus_previous_tab)
      .unwrap_or(false)
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
    self
      .document_panels
      .as_mut()
      .map(DocumentPanelState::focus_next_tab)
      .unwrap_or(false)
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
    self
      .document_panels
      .as_mut()
      .map(DocumentPanelState::close_focused_tab)
      .unwrap_or(false)
  }

  /// Starts a document tab drag after GPUI crosses its movement threshold.
  ///
  /// # Parameters
  ///
  /// `drag` identifies the source panel, tab, index, and preview title.
  ///
  /// # Returns
  ///
  /// `true` when document panel state accepts the drag; otherwise `false`.
  pub(crate) fn start_document_panel_tab_drag(&mut self, drag: PanelTabDrag) -> bool {
    self
      .document_panels
      .as_mut()
      .map(|document_panels| document_panels.start_tab_drag(drag))
      .unwrap_or(false)
  }

  /// Updates the current document tab insertion target.
  ///
  /// # Parameters
  ///
  /// `target` identifies a valid panel tab strip and raw insertion index.
  ///
  /// # Returns
  ///
  /// `true` when temporary target state changed; otherwise `false`.
  pub(crate) fn update_document_panel_tab_drag_target(
    &mut self,
    target: PanelTabDropTarget,
  ) -> bool {
    self
      .document_panels
      .as_mut()
      .map(|document_panels| document_panels.update_tab_drag_target(target))
      .unwrap_or(false)
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
    self
      .document_panels
      .as_mut()
      .map(DocumentPanelState::clear_tab_drag_target)
      .unwrap_or(false)
  }

  /// Commits a dragged document tab released over a valid target strip.
  ///
  /// # Parameters
  ///
  /// `drag` is the stable GPUI drag payload.
  ///
  /// `target_panel_id` identifies the strip accepting the drop.
  ///
  /// # Returns
  ///
  /// `true` when the tab move succeeds; otherwise `false`.
  pub(crate) fn drop_document_panel_tab(
    &mut self,
    drag: PanelTabDrag,
    target_panel_id: PanelId,
  ) -> bool {
    self
      .document_panels
      .as_mut()
      .map(|document_panels| document_panels.drop_tab(drag, target_panel_id))
      .unwrap_or(false)
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
    self
      .document_panels
      .as_mut()
      .map(DocumentPanelState::cancel_tab_drag)
      .unwrap_or(false)
  }

  /// Starts resizing one document panel split.
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
  /// `true` when document panel state exists and accepted the resize start;
  /// otherwise `false`.
  pub(crate) fn start_document_panel_resize(
    &mut self,
    path: PanelSplitPath,
    axis: PanelSplitAxis,
    start_position: Pixels,
  ) -> bool {
    self
      .document_panels
      .as_mut()
      .map(|document_panels| document_panels.start_resize(path, axis, start_position))
      .unwrap_or(false)
  }

  /// Updates the active document panel split resize.
  ///
  /// # Parameters
  ///
  /// `current_position` is the latest cursor position on the active resize
  /// axis.
  ///
  /// `root_width` is the rendered width available to the document panel root.
  ///
  /// `root_height` is the rendered height available to the document panel root.
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
      .as_mut()
      .map(|document_panels| document_panels.drag_resize(current_position, root_width, root_height))
      .unwrap_or(false)
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
    self
      .document_panels
      .as_mut()
      .map(DocumentPanelState::stop_resize)
      .unwrap_or(false)
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
    self
      .document_panels
      .as_ref()
      .and_then(DocumentPanelState::resize_axis)
  }
}

/// Renders the main workbench document area.
///
/// When a file is opened from the workspace tree, this renders a minimal
/// placeholder document. Otherwise it keeps the existing product placeholder so
/// the main area is still useful before any file is selected.
///
/// # Parameters
///
/// `document_panels` contains the currently opened document panel layout, if
/// any.
///
/// `summary` provides product placeholder text for empty states.
///
/// `active_activity` identifies the selected workbench activity for empty
/// placeholder copy.
///
/// `theme` supplies colors for the document area.
///
/// `app` is the weak app entity used by document panel callbacks.
///
/// # Returns
///
/// A GPUI element for the main document region.
pub fn render_document_area(
  document_panels: Option<&DocumentPanelState>,
  summary: &WorkspaceSummary,
  active_activity: ActiveActivity,
  theme: UIThemes,
  focus_handle: &FocusHandle,
  app: WeakEntity<ChitinApp>,
  cx: &mut Context<ChitinApp>,
) -> impl IntoElement {
  match document_panels {
    Some(document_panels) => {
      render_opened_document_panels(document_panels, theme, focus_handle, app, cx)
    }
    None => render_empty_document_area(summary, active_activity, theme),
  }
}

/// Renders document panels for files opened from the workspace tree.
///
/// # Parameters
///
/// `document_panels` contains the panel tree and tab state to render.
///
/// `theme` supplies colors for the placeholder document surface.
///
/// `app` is the weak app entity used by tab and split button callbacks.
///
/// # Returns
///
/// A GPUI `Div` containing the placeholder opened-document view.
fn render_opened_document_panels(
  document_panels: &DocumentPanelState,
  theme: UIThemes,
  focus_handle: &FocusHandle,
  app: WeakEntity<ChitinApp>,
  cx: &mut Context<ChitinApp>,
) -> gpui::Div {
  let activate_app = app.clone();
  let on_activate_tab: Rc<PanelTabActivateHandler> = Rc::new(
    move |panel_id: PanelId, tab_id: PanelTabId, _: &mut Window, cx: &mut App| {
      let _ = activate_app.update(cx, |app, cx| {
        if app.activate_document_panel_tab(panel_id, tab_id) {
          cx.notify();
        }
      });
    },
  );
  let close_app = app.clone();
  let on_close_tab: Rc<PanelTabCloseHandler> = Rc::new(
    move |panel_id: PanelId, tab_id: PanelTabId, _: &mut Window, cx: &mut App| {
      let _ = close_app.update(cx, |app, cx| {
        if app.close_document_panel_tab(panel_id, tab_id) {
          cx.notify();
        }
      });
    },
  );
  let render_tab_close_icon: Rc<PanelTabCloseIconRenderer> = Rc::new(move |theme| {
    svg()
      .path(TAB_CLOSE_ICON_PATH)
      .size(TAB_CLOSE_ICON_SIZE)
      .text_color(theme.text.primary)
      .into_any_element()
  });
  let drag_start_app = app.clone();
  let on_tab_drag_start: Rc<PanelTabDragStartHandler> =
    Rc::new(move |drag, _: &mut Window, cx: &mut App| {
      let _ = drag_start_app.update(cx, |app, cx| {
        if app.start_document_panel_tab_drag(drag) {
          cx.notify();
        }
      });
    });
  let drag_target_app = app.clone();
  let on_tab_drag_target: Rc<PanelTabDragTargetHandler> =
    Rc::new(move |target, _: &mut Window, cx: &mut App| {
      let _ = drag_target_app.update(cx, |app, cx| {
        if app.update_document_panel_tab_drag_target(target) {
          cx.notify();
        }
      });
    });
  let drop_app = app.clone();
  let on_tab_drop: Rc<PanelTabDropHandler> =
    Rc::new(move |drag, panel_id, _: &mut Window, cx: &mut App| {
      let _ = drop_app.update(cx, |app, cx| {
        app.drop_document_panel_tab(drag, panel_id);
        cx.notify();
      });
    });
  let tab_drag = PanelTabDragConfig::new(on_tab_drag_start, on_tab_drag_target, on_tab_drop)
    .state(document_panels.tab_drag.clone());

  let actions_app = app.clone();
  let render_tab_strip_actions = Rc::new(move |panel_id| {
    render_panel_tab_strip_actions(panel_id, theme, actions_app.clone()).into_any_element()
  });
  let resize_app = app.clone();
  let resize = PanelResizeConfig::new(move |path, axis, start_position, _, cx| {
    let _ = resize_app.update(cx, |app, cx| {
      if app.start_document_panel_resize(path, axis, start_position) {
        cx.notify();
      }
    });
  });
  let panel_config = PanelContainerConfig::new()
    .resize(resize)
    .focused_panel_id(document_panels.focused_panel_id)
    .on_activate_tab(on_activate_tab)
    .on_close_tab(on_close_tab)
    .render_tab_close_icon(render_tab_close_icon)
    .render_tab_strip_actions(render_tab_strip_actions)
    .tab_drag(tab_drag);

  let mouse_focus_handle = focus_handle.clone();

  div()
    .flex()
    .flex_1()
    .min_w_0()
    .min_h_0()
    .track_focus(focus_handle)
    .key_context(PANEL_CONTAINER_KEY_CONTEXT)
    .on_action(cx.listener(|this, _: &FocusPreviousPanelTab, _, cx| {
      this.dispatch_command(PanelTabCommand::FocusPrevious.into(), cx);
    }))
    .on_action(cx.listener(|this, _: &FocusNextPanelTab, _, cx| {
      this.dispatch_command(PanelTabCommand::FocusNext.into(), cx);
    }))
    .on_action(cx.listener(|this, _: &CloseTab, _, cx| {
      this.dispatch_command(PanelTabCommand::Close.into(), cx);
    }))
    .on_mouse_down(MouseButton::Left, move |_, window, _| {
      window.focus(&mouse_focus_handle);
    })
    .child(render_panel_container(
      &document_panels.tree,
      theme,
      panel_config,
      &|tab| render_opened_document_body(&tab.payload, theme).into_any_element(),
    ))
}

/// Renders split controls at the right end of one document panel tab strip.
///
/// # Parameters
///
/// `panel_id` identifies the panel controlled by the split buttons.
///
/// `theme` supplies colors for the action buttons.
///
/// `app` is the weak app entity updated by button clicks.
///
/// # Returns
///
/// A GPUI element containing horizontal and vertical split buttons.
fn render_panel_tab_strip_actions(
  panel_id: PanelId,
  theme: UIThemes,
  app: WeakEntity<ChitinApp>,
) -> gpui::Div {
  div()
    .flex()
    .items_center()
    .h_full()
    .flex_none()
    .border_l_1()
    .border_color(theme.border.primary)
    .bg(theme.background.primary)
    .child(render_panel_split_button(
      panel_id,
      PanelSplitAxis::Horizontal,
      SPLIT_HORIZONTAL_ICON_PATH,
      theme,
      app.clone(),
    ))
    .child(render_panel_split_button(
      panel_id,
      PanelSplitAxis::Vertical,
      SPLIT_VERTICAL_ICON_PATH,
      theme,
      app,
    ))
}

/// Renders one split button for a document panel.
///
/// # Parameters
///
/// `panel_id` identifies the panel that should be split when clicked.
///
/// `axis` controls the orientation of the created split.
///
/// `icon_path` is the desktop asset path for the button icon.
///
/// `theme` supplies colors for the button.
///
/// `app` is the weak app entity updated by button clicks.
///
/// # Returns
///
/// A GPUI element for one icon-only split button.
fn render_panel_split_button(
  panel_id: PanelId,
  axis: PanelSplitAxis,
  icon_path: &'static str,
  theme: UIThemes,
  app: WeakEntity<ChitinApp>,
) -> gpui::Div {
  div()
    .flex()
    .items_center()
    .justify_center()
    .size(px(30.0))
    .cursor_pointer()
    .text_color(theme.text.secondary)
    .hover(move |style| {
      style
        .bg(theme.background.hover)
        .text_color(theme.text.primary)
    })
    .on_mouse_up(MouseButton::Left, move |_, _, cx| {
      let _ = app.update(cx, |app, cx| {
        if app.split_document_panel(panel_id, axis) {
          cx.notify();
        }
      });
    })
    .child(
      svg()
        .path(icon_path)
        .size(PANEL_ACTION_ICON_SIZE)
        .text_color(theme.text.secondary),
    )
}

/// Renders placeholder content for an opened document tab.
///
/// # Parameters
///
/// `document` is the opened file descriptor to display.
///
/// `theme` supplies colors for the document body.
///
/// # Returns
///
/// A GPUI element containing placeholder document content.
fn render_opened_document_body(document: &OpenedProjectDocument, theme: UIThemes) -> AnyElement {
  div()
    .flex()
    .flex_col()
    .flex_1()
    .min_h_0()
    .p_8()
    .gap_3()
    .child(
      div()
        .text_lg()
        .font_weight(FontWeight::SEMIBOLD)
        .child("Placeholder document"),
    )
    .child(
      div()
        .text_sm()
        .text_color(theme.text.secondary)
        .child(document.path.display().to_string()),
    )
    .into_any_element()
}

/// Renders the main area before a workspace file is opened.
///
/// # Parameters
///
/// `summary` provides product name and focus text.
///
/// `active_activity` selects the placeholder heading and description.
///
/// `theme` supplies colors for the empty document area.
///
/// # Returns
///
/// A GPUI `Div` containing the empty workbench placeholder.
fn render_empty_document_area(
  summary: &WorkspaceSummary,
  active_activity: ActiveActivity,
  theme: UIThemes,
) -> gpui::Div {
  div()
    .flex()
    .flex_col()
    .flex_1()
    .h_full()
    .p_8()
    .gap_4()
    .child(
      div()
        .text_3xl()
        .font_weight(FontWeight::SEMIBOLD)
        .child(summary.product_name),
    )
    .child(
      div()
        .text_lg()
        .text_color(theme.text.secondary)
        .child(summary.focus),
    )
    .child(
      div()
        .mt_6()
        .p_4()
        .rounded_md()
        .border_1()
        .border_color(theme.border.primary)
        .bg(theme.background.secondary)
        .child(
          div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
              div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(active_activity.title()),
            )
            .child(
              div()
                .text_sm()
                .text_color(theme.text.secondary)
                .child(active_activity.description()),
            ),
        ),
    )
}

#[cfg(test)]
mod tests {
  use super::*;
  use gpui::SharedString;

  /// Creates an opened test document.
  ///
  /// # Parameters
  ///
  /// `name` is the file name used for the document path and title.
  ///
  /// # Returns
  ///
  /// An [`OpenedProjectDocument`] rooted under a stable test directory.
  fn test_document(name: &str) -> OpenedProjectDocument {
    OpenedProjectDocument::new(
      Path::new("/tmp/chitin-document-panel-test")
        .join(name)
        .as_path(),
    )
  }

  /// Creates document panel state containing two tabs in the root panel.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A [`DocumentPanelState`] whose root panel can exercise independent active
  /// tab state after splitting.
  fn two_tab_document_panel_state() -> DocumentPanelState {
    DocumentPanelState {
      tree: PanelTree::single_leaf(
        PanelLeaf::new(DEFAULT_DOCUMENT_PANEL_ID)
          .tab(PanelTab::new(
            DEFAULT_DOCUMENT_TAB_ID,
            SharedString::from("alpha.rs"),
            test_document("alpha.rs"),
          ))
          .tab(PanelTab::new(
            PanelTabId::new(2),
            SharedString::from("beta.rs"),
            test_document("beta.rs"),
          )),
      ),
      focused_panel_id: DEFAULT_DOCUMENT_PANEL_ID,
      next_panel_id: FIRST_DYNAMIC_DOCUMENT_PANEL_ID,
      next_tab_id: PanelTabId::new(3),
      resize_drag: None,
      tab_drag: None,
    }
  }

  /// Creates a stable drag payload for one tab in a test panel.
  ///
  /// # Parameters
  ///
  /// `panel_id` identifies the source panel.
  ///
  /// `tab_id` identifies the source tab.
  ///
  /// `source_index` is the tab's original position.
  ///
  /// # Returns
  ///
  /// A [`PanelTabDrag`] with a deterministic preview title.
  fn test_tab_drag(panel_id: PanelId, tab_id: PanelTabId, source_index: usize) -> PanelTabDrag {
    PanelTabDrag {
      source_panel_id: panel_id,
      tab_id,
      source_index,
      title: SharedString::from("dragged"),
    }
  }

  /// Verifies that splitting a document panel copies only its active tab.
  #[test]
  fn split_panel_should_copy_only_active_tab() {
    let mut state = two_tab_document_panel_state();

    let Some(new_panel_id) =
      state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal)
    else {
      panic!("document panel should split");
    };
    let Some(source_leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
      panic!("source panel should still exist");
    };
    let Some(new_leaf) = state.tree.leaf(new_panel_id) else {
      panic!("new panel should exist");
    };

    assert_eq!(source_leaf.tabs.len(), 2);
    assert_eq!(source_leaf.active_tab, Some(DEFAULT_DOCUMENT_TAB_ID));
    assert_eq!(new_leaf.tabs.len(), 1);
    assert_eq!(new_leaf.tabs[0].payload.title, "alpha.rs");
    assert_eq!(new_leaf.active_tab, Some(PanelTabId::new(3)));
  }

  /// Verifies that splitting copies the active tab at split time.
  #[test]
  fn split_panel_should_copy_changed_active_tab() {
    let mut state = two_tab_document_panel_state();
    assert!(state.activate_tab(DEFAULT_DOCUMENT_PANEL_ID, PanelTabId::new(2)));

    let Some(new_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Vertical)
    else {
      panic!("document panel should split");
    };
    let Some(new_leaf) = state.tree.leaf(new_panel_id) else {
      panic!("new panel should exist");
    };

    assert_eq!(new_leaf.tabs.len(), 1);
    assert_eq!(new_leaf.tabs[0].payload.title, "beta.rs");
    assert_eq!(new_leaf.active_tab, Some(PanelTabId::new(3)));
  }

  /// Verifies that repeated splits do not copy inactive source tabs.
  #[test]
  fn split_panel_should_not_accumulate_all_source_tabs_on_repeated_splits() {
    let mut state = two_tab_document_panel_state();

    let Some(first_new_panel_id) =
      state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal)
    else {
      panic!("first split should succeed");
    };
    let Some(second_new_panel_id) =
      state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Vertical)
    else {
      panic!("second split should succeed");
    };

    let Some(first_new_leaf) = state.tree.leaf(first_new_panel_id) else {
      panic!("first new panel should exist");
    };
    let Some(second_new_leaf) = state.tree.leaf(second_new_panel_id) else {
      panic!("second new panel should exist");
    };

    assert_eq!(first_new_leaf.tabs.len(), 1);
    assert_eq!(second_new_leaf.tabs.len(), 1);
  }

  /// Verifies that closing a split panel's last tab removes that panel.
  #[test]
  fn close_tab_should_remove_empty_split_panel_and_keep_source_panel() {
    let mut state = two_tab_document_panel_state();

    let Some(new_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Vertical)
    else {
      panic!("document panel should split");
    };

    assert!(state.activate_tab(DEFAULT_DOCUMENT_PANEL_ID, PanelTabId::new(2)));
    assert!(state.close_tab(new_panel_id, PanelTabId::new(3)));

    let Some(source_leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
      panic!("source panel should still exist");
    };

    assert_eq!(state.tree.leaf_count(), 1);
    assert!(state.tree.leaf(new_panel_id).is_none());
    assert_eq!(source_leaf.tabs.len(), 2);
    assert_eq!(source_leaf.active_tab, Some(PanelTabId::new(2)));
    assert_eq!(state.focused_panel_id, DEFAULT_DOCUMENT_PANEL_ID);
  }

  /// Verifies next-tab focus wraps from the final tab to the first tab.
  #[test]
  fn focus_next_tab_should_wrap_to_first_tab() {
    let mut state = two_tab_document_panel_state();
    assert!(state.activate_tab(DEFAULT_DOCUMENT_PANEL_ID, PanelTabId::new(2)));

    assert!(state.focus_next_tab());

    let Some(leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
      panic!("focused panel should exist");
    };
    assert_eq!(leaf.active_tab, Some(DEFAULT_DOCUMENT_TAB_ID));
  }

  /// Verifies previous-tab focus wraps from the first tab to the final tab.
  #[test]
  fn focus_previous_tab_should_wrap_to_last_tab() {
    let mut state = two_tab_document_panel_state();

    assert!(state.focus_previous_tab());

    let Some(leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
      panic!("focused panel should exist");
    };
    assert_eq!(leaf.active_tab, Some(PanelTabId::new(2)));
  }

  /// Verifies closing the focused active tab selects its right neighbor.
  #[test]
  fn close_focused_tab_should_focus_right_neighbor() {
    let mut state = two_tab_document_panel_state();

    assert!(state.close_focused_tab());

    let Some(leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
      panic!("focused panel should exist");
    };
    assert_eq!(leaf.active_tab, Some(PanelTabId::new(2)));
    assert_eq!(state.focused_panel_id, DEFAULT_DOCUMENT_PANEL_ID);
  }

  /// Verifies closing the final focused tab selects its left neighbor.
  #[test]
  fn close_focused_tab_should_fall_back_to_left_neighbor() {
    let mut state = two_tab_document_panel_state();
    assert!(state.activate_tab(DEFAULT_DOCUMENT_PANEL_ID, PanelTabId::new(2)));

    assert!(state.close_focused_tab());

    let Some(leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
      panic!("focused panel should exist");
    };
    assert_eq!(leaf.active_tab, Some(DEFAULT_DOCUMENT_TAB_ID));
  }

  /// Verifies next-tab focus advances from one panel to the next panel.
  #[test]
  fn focus_next_tab_should_cross_panel_boundary() {
    let mut state = two_tab_document_panel_state();
    let Some(next_panel_id) =
      state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal)
    else {
      panic!("document panel should split");
    };
    assert!(state.activate_tab(DEFAULT_DOCUMENT_PANEL_ID, PanelTabId::new(2)));

    assert!(state.focus_next_tab());

    let Some(next_leaf) = state.tree.leaf(next_panel_id) else {
      panic!("next panel should exist");
    };
    assert_eq!(state.focused_panel_id, next_panel_id);
    assert_eq!(next_leaf.active_tab, Some(PanelTabId::new(3)));
  }

  /// Verifies previous-tab focus moves from one panel to the previous panel.
  #[test]
  fn focus_previous_tab_should_cross_panel_boundary() {
    let mut state = two_tab_document_panel_state();
    let Some(next_panel_id) =
      state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal)
    else {
      panic!("document panel should split");
    };
    assert!(state.activate_tab(next_panel_id, PanelTabId::new(3)));

    assert!(state.focus_previous_tab());

    let Some(previous_leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
      panic!("previous panel should exist");
    };
    assert_eq!(state.focused_panel_id, DEFAULT_DOCUMENT_PANEL_ID);
    assert_eq!(previous_leaf.active_tab, Some(PanelTabId::new(2)));
  }

  /// Verifies container navigation wraps from the final panel to the first tab.
  #[test]
  fn focus_next_tab_should_wrap_across_panels_to_first_tab() {
    let mut state = two_tab_document_panel_state();
    let Some(next_panel_id) =
      state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal)
    else {
      panic!("document panel should split");
    };
    assert!(state.activate_tab(next_panel_id, PanelTabId::new(3)));

    assert!(state.focus_next_tab());

    let Some(first_leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
      panic!("first panel should exist");
    };
    assert_eq!(state.focused_panel_id, DEFAULT_DOCUMENT_PANEL_ID);
    assert_eq!(first_leaf.active_tab, Some(DEFAULT_DOCUMENT_TAB_ID));
  }

  /// Verifies container navigation wraps from the first tab to the final panel.
  #[test]
  fn focus_previous_tab_should_wrap_across_panels_to_last_tab() {
    let mut state = two_tab_document_panel_state();
    let Some(next_panel_id) =
      state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal)
    else {
      panic!("document panel should split");
    };
    assert!(state.activate_tab(DEFAULT_DOCUMENT_PANEL_ID, DEFAULT_DOCUMENT_TAB_ID));

    assert!(state.focus_previous_tab());

    let Some(next_leaf) = state.tree.leaf(next_panel_id) else {
      panic!("next panel should exist");
    };
    assert_eq!(state.focused_panel_id, next_panel_id);
    assert_eq!(next_leaf.active_tab, Some(PanelTabId::new(3)));
  }

  /// Verifies that split views receive container-unique tab identifiers.
  #[test]
  fn split_panel_should_allocate_unique_tab_id_for_copied_active_tab() {
    let mut state = two_tab_document_panel_state();

    let Some(new_panel_id) =
      state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal)
    else {
      panic!("document panel should split");
    };
    let Some(new_leaf) = state.tree.leaf(new_panel_id) else {
      panic!("new panel should exist");
    };

    assert_eq!(new_leaf.tabs[0].id, PanelTabId::new(3));
    assert_eq!(new_leaf.active_tab, Some(PanelTabId::new(3)));
  }

  /// Verifies that splitting an empty panel creates an empty new panel.
  #[test]
  fn split_panel_should_create_empty_leaf_when_source_has_no_active_tab() {
    let mut state = DocumentPanelState {
      tree: PanelTree::single_leaf(PanelLeaf::new(DEFAULT_DOCUMENT_PANEL_ID)),
      focused_panel_id: DEFAULT_DOCUMENT_PANEL_ID,
      next_panel_id: FIRST_DYNAMIC_DOCUMENT_PANEL_ID,
      next_tab_id: FIRST_DYNAMIC_DOCUMENT_TAB_ID,
      resize_drag: None,
      tab_drag: None,
    };

    let Some(new_panel_id) =
      state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal)
    else {
      panic!("empty document panel should split");
    };
    let Some(new_leaf) = state.tree.leaf(new_panel_id) else {
      panic!("new panel should exist");
    };

    assert!(new_leaf.tabs.is_empty());
    assert_eq!(new_leaf.active_tab, None);
  }

  /// Verifies a drag reorder preserves the moved document and focuses its panel.
  #[test]
  fn tab_drag_should_reorder_inside_source_panel() {
    let mut state = two_tab_document_panel_state();
    let drag = test_tab_drag(DEFAULT_DOCUMENT_PANEL_ID, DEFAULT_DOCUMENT_TAB_ID, 0);
    assert!(state.start_tab_drag(drag.clone()));
    assert!(state.update_tab_drag_target(PanelTabDropTarget {
      panel_id: DEFAULT_DOCUMENT_PANEL_ID,
      insertion_index: 2,
    }));

    assert!(state.drop_tab(drag, DEFAULT_DOCUMENT_PANEL_ID));

    let Some(leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
      panic!("source panel should exist");
    };
    assert_eq!(
      leaf
        .tabs
        .iter()
        .map(|tab| tab.payload.title.as_str())
        .collect::<Vec<_>>(),
      vec!["beta.rs", "alpha.rs"]
    );
    assert_eq!(leaf.active_tab, Some(DEFAULT_DOCUMENT_TAB_ID));
    assert_eq!(state.focused_panel_id, DEFAULT_DOCUMENT_PANEL_ID);
    assert_eq!(state.tab_drag, None);
  }

  /// Verifies a cross-panel drop transfers focus and preserves tab identity.
  #[test]
  fn tab_drag_should_move_existing_tab_to_target_panel() {
    let mut state = two_tab_document_panel_state();
    let Some(target_panel_id) =
      state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal)
    else {
      panic!("target panel should be created");
    };
    let drag = test_tab_drag(DEFAULT_DOCUMENT_PANEL_ID, PanelTabId::new(2), 1);
    assert!(state.start_tab_drag(drag.clone()));
    assert!(state.update_tab_drag_target(PanelTabDropTarget {
      panel_id: target_panel_id,
      insertion_index: 0,
    }));

    assert!(state.drop_tab(drag, target_panel_id));

    let Some(target) = state.tree.leaf(target_panel_id) else {
      panic!("target panel should survive");
    };
    assert_eq!(target.tabs[0].id, PanelTabId::new(2));
    assert_eq!(target.tabs[0].payload.title, "beta.rs");
    assert_eq!(target.active_tab, Some(PanelTabId::new(2)));
    assert_eq!(state.focused_panel_id, target_panel_id);
  }

  /// Verifies moving a source panel's final tab collapses around the target.
  #[test]
  fn tab_drag_should_focus_target_after_empty_source_panel_is_removed() {
    let mut state = DocumentPanelState::new(test_document("alpha.rs"));
    let Some(target_panel_id) =
      state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal)
    else {
      panic!("target panel should be created");
    };
    let drag = test_tab_drag(DEFAULT_DOCUMENT_PANEL_ID, DEFAULT_DOCUMENT_TAB_ID, 0);
    assert!(state.start_tab_drag(drag.clone()));
    assert!(state.update_tab_drag_target(PanelTabDropTarget {
      panel_id: target_panel_id,
      insertion_index: 1,
    }));

    assert!(state.drop_tab(drag, target_panel_id));

    assert_eq!(state.tree.leaf_count(), 1);
    assert!(state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID).is_none());
    assert!(state.tree.leaf(target_panel_id).is_some());
    assert_eq!(state.focused_panel_id, target_panel_id);
  }

  /// Verifies cancelling a drag leaves the persistent panel tree unchanged.
  #[test]
  fn tab_drag_should_not_mutate_tree_when_cancelled() {
    let mut state = two_tab_document_panel_state();
    let before = state.tree.clone();
    let drag = test_tab_drag(DEFAULT_DOCUMENT_PANEL_ID, DEFAULT_DOCUMENT_TAB_ID, 0);
    assert!(state.start_tab_drag(drag));
    assert!(state.update_tab_drag_target(PanelTabDropTarget {
      panel_id: DEFAULT_DOCUMENT_PANEL_ID,
      insertion_index: 2,
    }));

    assert!(state.cancel_tab_drag());

    assert_eq!(state.tree, before);
    assert_eq!(state.tab_drag, None);
  }

  /// Verifies leaving every strip clears only temporary target feedback.
  #[test]
  fn tab_drag_should_clear_target_without_cancelling_session() {
    let mut state = two_tab_document_panel_state();
    let drag = test_tab_drag(DEFAULT_DOCUMENT_PANEL_ID, DEFAULT_DOCUMENT_TAB_ID, 0);
    assert!(state.start_tab_drag(drag.clone()));
    assert!(state.update_tab_drag_target(PanelTabDropTarget {
      panel_id: DEFAULT_DOCUMENT_PANEL_ID,
      insertion_index: 2,
    }));

    assert!(state.clear_tab_drag_target());

    assert_eq!(
      state.tab_drag,
      Some(PanelTabDragState {
        drag,
        drop_target: None,
      })
    );
  }

  /// Verifies a stale source index cannot begin a tab drag.
  #[test]
  fn tab_drag_should_reject_stale_source_without_temporary_state() {
    let mut state = two_tab_document_panel_state();

    assert!(!state.start_tab_drag(test_tab_drag(
      DEFAULT_DOCUMENT_PANEL_ID,
      DEFAULT_DOCUMENT_TAB_ID,
      1,
    )));

    assert_eq!(state.tab_drag, None);
  }

  /// Verifies that closing a non-final tab keeps its panel in the layout.
  #[test]
  fn close_tab_should_keep_panel_when_tabs_remain() {
    let mut state = two_tab_document_panel_state();
    assert!(
      state
        .split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal)
        .is_some()
    );

    assert!(state.close_tab(DEFAULT_DOCUMENT_PANEL_ID, PanelTabId::new(2)));

    let Some(source_leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
      panic!("source panel should still exist");
    };
    assert_eq!(state.tree.leaf_count(), 2);
    assert_eq!(source_leaf.tabs.len(), 1);
    assert_eq!(source_leaf.active_tab, Some(DEFAULT_DOCUMENT_TAB_ID));
    assert_eq!(state.focused_panel_id, DEFAULT_DOCUMENT_PANEL_ID);
  }

  /// Verifies that closing the final tab in the only panel keeps an empty panel.
  #[test]
  fn close_tab_should_keep_empty_state_when_only_panel_remains() {
    let mut state = DocumentPanelState::new(test_document("alpha.rs"));

    assert!(state.close_tab(DEFAULT_DOCUMENT_PANEL_ID, DEFAULT_DOCUMENT_TAB_ID));

    let Some(leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
      panic!("final panel should remain");
    };
    assert_eq!(state.tree.leaf_count(), 1);
    assert!(leaf.tabs.is_empty());
    assert_eq!(leaf.active_tab, None);
    assert_eq!(state.focused_panel_id, DEFAULT_DOCUMENT_PANEL_ID);
  }

  /// Verifies that repeated split closes keep the panel tree valid.
  #[test]
  fn close_tab_should_keep_panel_tree_valid_after_repeated_splits_and_closes() {
    let mut state = two_tab_document_panel_state();

    let Some(first_new_panel_id) =
      state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal)
    else {
      panic!("first split should succeed");
    };
    let Some(second_new_panel_id) =
      state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Vertical)
    else {
      panic!("second split should succeed");
    };

    assert!(state.close_tab(first_new_panel_id, PanelTabId::new(3)));
    assert!(state.close_tab(second_new_panel_id, PanelTabId::new(4)));

    assert_eq!(state.tree.leaf_count(), 1);
    assert!(state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID).is_some());
    assert_eq!(state.focused_panel_id, DEFAULT_DOCUMENT_PANEL_ID);
  }

  /// Verifies that opening another document appends a tab to the focused panel.
  #[test]
  fn open_document_as_tab_should_append_to_focused_panel() {
    let mut state = DocumentPanelState::new(test_document("alpha.rs"));

    assert!(state.open_document_as_tab(test_document("beta.rs")));

    let Some(leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
      panic!("default panel should exist");
    };

    assert_eq!(leaf.tabs.len(), 2);
    assert_eq!(leaf.active_tab, Some(PanelTabId::new(2)));
    assert_eq!(leaf.tabs[0].payload.title, "alpha.rs");
    assert_eq!(leaf.tabs[1].payload.title, "beta.rs");
  }

  /// Verifies that opening an already-open document focuses the existing tab.
  #[test]
  fn open_document_as_tab_should_focus_existing_tab_in_focused_panel() {
    let mut state = DocumentPanelState::new(test_document("alpha.rs"));
    assert!(state.open_document_as_tab(test_document("beta.rs")));
    assert!(state.activate_tab(DEFAULT_DOCUMENT_PANEL_ID, DEFAULT_DOCUMENT_TAB_ID));

    assert!(state.open_document_as_tab(test_document("beta.rs")));

    let Some(leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
      panic!("default panel should exist");
    };

    assert_eq!(leaf.tabs.len(), 2);
    assert_eq!(leaf.active_tab, Some(PanelTabId::new(2)));
    assert_eq!(leaf.tabs[1].payload.title, "beta.rs");
  }

  /// Verifies that new documents open in the currently focused split panel.
  #[test]
  fn open_document_as_tab_should_target_focused_panel() {
    let mut state = two_tab_document_panel_state();
    let Some(new_panel_id) =
      state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal)
    else {
      panic!("document panel should split");
    };

    assert!(state.activate_tab(new_panel_id, PanelTabId::new(3)));
    assert!(state.open_document_as_tab(test_document("gamma.rs")));

    let Some(source_leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
      panic!("source panel should exist");
    };
    let Some(focused_leaf) = state.tree.leaf(new_panel_id) else {
      panic!("focused panel should exist");
    };

    assert_eq!(source_leaf.tabs.len(), 2);
    assert_eq!(focused_leaf.tabs.len(), 2);
    assert_eq!(focused_leaf.active_tab, Some(PanelTabId::new(4)));
    assert_eq!(focused_leaf.tabs[1].payload.title, "gamma.rs");
  }

  /// Verifies that document panel resize updates the target split ratio.
  #[test]
  fn drag_resize_should_update_split_ratio() {
    let mut state = two_tab_document_panel_state();
    assert!(
      state
        .split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal)
        .is_some()
    );

    assert!(state.start_resize(
      PanelSplitPath::root(),
      PanelSplitAxis::Horizontal,
      px(100.0)
    ));
    assert!(state.drag_resize(px(150.0), px(500.0), px(300.0)));

    assert_eq!(state.tree.split_ratio(&PanelSplitPath::root()), Some(0.6));
  }

  /// Verifies that stopping document panel resize clears active drag state.
  #[test]
  fn stop_resize_should_clear_active_drag_state() {
    let mut state = two_tab_document_panel_state();
    assert!(
      state
        .split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Vertical)
        .is_some()
    );
    assert!(state.start_resize(PanelSplitPath::root(), PanelSplitAxis::Vertical, px(100.0)));

    assert!(state.stop_resize());

    assert_eq!(state.resize_axis(), None);
  }
}
