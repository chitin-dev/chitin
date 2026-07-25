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
    PanelId, PanelLeaf, PanelResizeConfig, PanelSplitAxis, PanelSplitPath, PanelSplitPlacement,
    PanelTab, PanelTabActivateHandler, PanelTabId, PanelTree, render_panel_container,
  },
  themes::UIThemes,
};
use gpui::{
  AnyElement, App, FontWeight, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
  Styled, WeakEntity, Window, div, px, svg,
};

use crate::{app::ChitinApp, components::activity_bar::ActiveActivity};

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
const FIRST_DYNAMIC_DOCUMENT_PANEL_ID: u64 = 2;
/// Asset path for the horizontal split panel action.
const SPLIT_HORIZONTAL_ICON_PATH: &str = "icons/panel/codicon-split-horizontal.svg";
/// Asset path for the vertical split panel action.
const SPLIT_VERTICAL_ICON_PATH: &str = "icons/panel/codicon-split-vertical.svg";
/// Size used by tab strip action icons.
const PANEL_ACTION_ICON_SIZE: gpui::Pixels = px(16.0);

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
  /// Next panel identifier to allocate for a split leaf.
  next_panel_id: u64,
  /// Active split resize drag, if the user is dragging a split handle.
  resize_drag: Option<DocumentPanelResizeDrag>,
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
      next_panel_id: FIRST_DYNAMIC_DOCUMENT_PANEL_ID,
      resize_drag: None,
    }
  }

  /// Replaces the document layout with one panel for a newly opened file.
  ///
  /// # Parameters
  ///
  /// `document` is the workspace file opened from the project tree.
  ///
  /// # Returns
  ///
  /// This function returns `()` and resets the document panel tree.
  pub(crate) fn replace_with_document(&mut self, document: OpenedProjectDocument) {
    *self = Self::new(document);
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
    self.tree.activate_tab(panel_id, tab_id)
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

  /// Splits one document panel and copies its current tabs to the new panel.
  ///
  /// The new panel receives a fresh panel id, but its tabs and active tab value
  /// are cloned from the source panel. Because active tab state is stored per
  /// leaf, each panel can later switch tabs independently.
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
    let mut copied_leaf = self.tree.leaf(panel_id)?.clone();
    let new_panel_id = self.allocate_panel_id();
    copied_leaf.id = new_panel_id;

    self
      .tree
      .split_leaf(panel_id, axis, copied_leaf, PanelSplitPlacement::After, 0.5)
      .then_some(new_panel_id)
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
    let panel_id = PanelId::new(self.next_panel_id);
    self.next_panel_id += 1;
    panel_id
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
  /// # Parameters
  ///
  /// `document` is the file descriptor created from a project tree entry.
  ///
  /// # Returns
  ///
  /// This function returns `()` and creates or replaces the document panel
  /// state.
  pub(crate) fn open_project_document(&mut self, document: OpenedProjectDocument) {
    match &mut self.document_panels {
      Some(document_panels) => document_panels.replace_with_document(document),
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
  app: WeakEntity<ChitinApp>,
) -> impl IntoElement {
  match document_panels {
    Some(document_panels) => render_opened_document_panels(document_panels, theme, app),
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
  app: WeakEntity<ChitinApp>,
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

  render_panel_container(
    &document_panels.tree,
    theme,
    Some(resize),
    Some(on_activate_tab),
    Some(render_tab_strip_actions),
    &|tab| render_opened_document_body(&tab.payload, theme).into_any_element(),
  )
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
      next_panel_id: FIRST_DYNAMIC_DOCUMENT_PANEL_ID,
      resize_drag: None,
    }
  }

  /// Verifies that splitting a document panel copies its tab stack.
  #[test]
  /// # Parameters
  ///
  /// This test takes no parameters.
  ///
  /// # Returns
  ///
  /// This test returns `()` and panics if the new panel does not receive the
  /// same tabs and active tab as the source panel.
  fn split_panel_should_copy_tabs_and_active_tab() {
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

    assert_eq!(new_leaf.tabs, source_leaf.tabs);
    assert_eq!(new_leaf.active_tab, source_leaf.active_tab);
  }

  /// Verifies that tab activation stays scoped to one split panel.
  #[test]
  /// # Parameters
  ///
  /// This test takes no parameters.
  ///
  /// # Returns
  ///
  /// This test returns `()` and panics if activating a tab in one panel changes
  /// another panel's active tab.
  fn split_panel_should_keep_active_tab_state_independent() {
    let mut state = two_tab_document_panel_state();

    let Some(new_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Vertical)
    else {
      panic!("document panel should split");
    };

    assert!(state.activate_tab(new_panel_id, PanelTabId::new(2)));

    let Some(source_leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
      panic!("source panel should still exist");
    };
    let Some(new_leaf) = state.tree.leaf(new_panel_id) else {
      panic!("new panel should exist");
    };

    assert_eq!(source_leaf.active_tab, Some(DEFAULT_DOCUMENT_TAB_ID));
    assert_eq!(new_leaf.active_tab, Some(PanelTabId::new(2)));
  }

  /// Verifies that document panel resize updates the target split ratio.
  #[test]
  /// # Parameters
  ///
  /// This test takes no parameters.
  ///
  /// # Returns
  ///
  /// This test returns `()` and panics if dragging a split handle does not
  /// update the stored split ratio.
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
  /// # Parameters
  ///
  /// This test takes no parameters.
  ///
  /// # Returns
  ///
  /// This test returns `()` and panics if resize state remains active after
  /// stopping the drag.
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
