//! Project sidebar composition.
//!
//! This module combines GPUI Kit sidebar chrome with the
//! desktop-specific workspace tree renderer.

pub(crate) mod tree;

use std::{
  collections::HashSet,
  path::{Path, PathBuf},
};

use chitin_command::WorkspaceCommand;
use chitin_ui::workbench::{WorkbenchStyle, panel_surface};
use chitin_utils::workspace::ProjectWorkspace;
use gpui_kit::component::{sidebar::SidebarHeader, theme::ThemeColor};

use gpui::{Context, FocusHandle, IntoElement, Pixels, ScrollStrategy, UniformListScrollHandle, div, prelude::*};

use crate::{
  app::ChitinApp,
  keybindings::{
    ActivateFocusedEntry, FocusFirstEntry, FocusLastEntry, FocusNextEntry, FocusPreviousEntry, PROJECT_TREE_KEY_CONTEXT,
  },
  workbench::explorer::tree::{WorkspaceTreeView, render_workspace_tree},
};

/// Default title shown at the top of the project workspace sidebar.
pub const DEFAULT_PROJECT_WORKSPACE_TITLE: &str = "File Explorer";
/// Initial width of the explorer pane.
pub const DEFAULT_PROJECT_SIDEBAR_WIDTH: Pixels = gpui::px(200.0);
/// Minimum explorer width supported by the workbench.
pub const MIN_PROJECT_SIDEBAR_WIDTH: Pixels = gpui::px(180.0);
/// Maximum explorer width supported by the workbench.
pub const MAX_PROJECT_SIDEBAR_WIDTH: Pixels = gpui::px(480.0);

/// App state managed by the project workspace sidebar.
///
/// `ChitinApp` owns this value as one grouped workbench state field. The
/// sidebar and workspace tree borrow it during rendering so new sidebar state
/// can be added without widening every component function signature.
#[derive(Clone, Debug)]
pub struct ProjectSidebarState {
  /// Directory paths whose children are visible in the workspace tree.
  pub expanded_paths: HashSet<PathBuf>,
  /// Directory paths currently loading their direct children.
  pub loading_paths: HashSet<PathBuf>,
  /// Workspace tree entry selected as the active project item.
  pub selected_path: Option<PathBuf>,
  /// Workspace tree entry focused for keyboard navigation.
  pub focused_path: Option<PathBuf>,
  /// Virtualized workspace tree scroll state.
  scroll_handle: UniformListScrollHandle,
  /// Kit tree mount and its last visible-row projection.
  pub(crate) tree_view: Option<WorkspaceTreeView>,
  /// Width projected from Kit's resizable group; no application drag state.
  width: Pixels,
}

impl ProjectSidebarState {
  /// Creates sidebar state with the workspace root expanded when present.
  ///
  /// # Parameters
  ///
  /// * `root` is the optional workspace root path to mark as expanded.
  ///
  /// # Returns
  ///
  /// A [`ProjectSidebarState`] with empty selection/focus state and default
  /// pane width.
  pub fn with_workspace_root(root: Option<&Path>) -> Self {
    Self {
      expanded_paths: root.map(|root| HashSet::from([root.to_path_buf()])).unwrap_or_default(),
      loading_paths: HashSet::new(),
      selected_path: None,
      focused_path: None,
      scroll_handle: UniformListScrollHandle::new(),
      tree_view: None,
      width: DEFAULT_PROJECT_SIDEBAR_WIDTH,
    }
  }

  /// Selects the workspace tree entry that backs the active opened document.
  ///
  /// # Parameters
  ///
  /// * `path` is the filesystem path to store as the selected project entry.
  ///
  /// # Returns
  ///
  /// This function returns `()` and mutates `selected_path`.
  pub fn select_entry(&mut self, path: &Path) {
    self.selected_path = Some(path.to_path_buf());
  }

  /// Focuses a workspace tree entry for keyboard navigation.
  ///
  /// # Parameters
  ///
  /// * `path` is the filesystem path to store as the focused project entry.
  ///
  /// # Returns
  ///
  /// This function returns `()` and mutates `focused_path`.
  pub fn focus_entry(&mut self, path: &Path) {
    self.focused_path = Some(path.to_path_buf());
  }

  /// Scrolls the workspace tree viewport until a rendered row is visible.
  ///
  /// # Parameters
  ///
  /// * `row_index` is the zero-based index in the rendered virtual tree row
  ///   list, including non-focusable message rows.
  /// * `strategy` controls which viewport edge should be used when the row is
  ///   outside the visible range.
  ///
  /// # Returns
  ///
  /// This function returns `()` and records a deferred GPUI scroll request.
  pub fn reveal_tree_row(&self, row_index: usize, strategy: ScrollStrategy) {
    self.scroll_handle.scroll_to_item(row_index, strategy);
  }

  /// Returns the sidebar width last reported by Kit.
  pub fn width(&self) -> Pixels {
    self.width
  }

  /// Shares Kit's scroll handle with workspace command navigation.
  pub(crate) fn set_tree_scroll_handle(&mut self, handle: UniformListScrollHandle) {
    self.scroll_handle = handle;
  }

  /// Stores the width reported by Kit within the product's sidebar bounds.
  ///
  /// # Parameters
  ///
  /// * `width` is Kit's measured panel width; non-finite values are ignored.
  ///
  /// # Returns
  ///
  /// Whether the stored width changed. Kit owns drag state and pointer handling.
  pub fn set_width(&mut self, width: Pixels) -> bool {
    let value = f32::from(width);
    if !value.is_finite() {
      return false;
    }
    let width = gpui::px(value.clamp(
      f32::from(MIN_PROJECT_SIDEBAR_WIDTH),
      f32::from(MAX_PROJECT_SIDEBAR_WIDTH),
    ));
    if self.width == width {
      return false;
    }
    self.width = width;
    true
  }
}

impl Default for ProjectSidebarState {
  /// Creates project sidebar state with no workspace root.
  fn default() -> Self {
    Self::with_workspace_root(None)
  }
}

/// Renders the project workspace sidebar and its command action boundary.
///
/// The sidebar itself is generic composition, while the file tree inside it is
/// desktop-specific because it uses Chitin's workspace SVG icon assets and
/// dispatches expansion events to [`ChitinApp`]. The outer wrapper tracks a
/// GPUI focus handle and registers workspace command actions so keybindings can
/// invoke the same command dispatcher future command palette entries will use.
///
/// # Parameters
///
/// * `workspace` is the currently opened project workspace. When `None`, the
///   sidebar renders an empty-workspace message instead of a tree.
/// * `state` contains expansion, loading, selection, focus, and saved width used
///   by the sidebar and tree.
/// * `focus_handle` is the GPUI focus handle associated with the `"ProjectTree"`
///   key context.
/// * `theme` supplies the UI colors and spacing used by the sidebar shell.
/// * `cx` creates the workspace command action listeners.
///
/// # Returns
///
/// A sidebar body for Kit's workbench resizable group.
pub fn render_project_sidebar(
  workspace: Option<&ProjectWorkspace>,
  state: &mut ProjectSidebarState,
  focus_handle: &FocusHandle,
  theme: ThemeColor,
  cx: &mut Context<ChitinApp>,
) -> impl IntoElement {
  panel_surface(WorkbenchStyle::new(theme))
    .track_focus(focus_handle)
    .key_context(PROJECT_TREE_KEY_CONTEXT)
    .on_action(cx.listener(|this, _: &FocusPreviousEntry, _, cx| {
      this.dispatch_command(WorkspaceCommand::FocusPrevious.into(), cx);
    }))
    .on_action(cx.listener(|this, _: &FocusNextEntry, _, cx| {
      this.dispatch_command(WorkspaceCommand::FocusNext.into(), cx);
    }))
    .on_action(cx.listener(|this, _: &ActivateFocusedEntry, window, cx| {
      this.activate_focused_project_tree_entry_with_window(window, cx);
      cx.notify();
    }))
    .on_action(cx.listener(|this, _: &FocusFirstEntry, _, cx| {
      this.dispatch_command(WorkspaceCommand::FocusFirst.into(), cx);
    }))
    .on_action(cx.listener(|this, _: &FocusLastEntry, _, cx| {
      this.dispatch_command(WorkspaceCommand::FocusLast.into(), cx);
    }))
    .child(
      SidebarHeader::new()
        .flex_none()
        .h(gpui::px(30.0))
        .rounded_none()
        .text_xs()
        .text_color(theme.sidebar_foreground)
        .child(DEFAULT_PROJECT_WORKSPACE_TITLE),
    )
    .child(
      div().flex().flex_1().min_h_0().w_full().child(match workspace {
        Some(workspace) => render_workspace_tree(&workspace.tree.root, state, theme, cx).into_any_element(),
        None => div()
          .p_3()
          .text_xs()
          .text_color(theme.muted_foreground)
          .child("Open a project path to show files.")
          .into_any_element(),
      }),
    )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reported_width_should_be_clamped_to_product_bounds() {
    let mut state = ProjectSidebarState::default();
    state.set_width(gpui::px(900.0));
    assert_eq!(state.width(), MAX_PROJECT_SIDEBAR_WIDTH);
    state.set_width(gpui::px(10.0));
    assert_eq!(state.width(), MIN_PROJECT_SIDEBAR_WIDTH);
  }

  #[test]
  fn unchanged_width_should_not_request_another_render() {
    let mut state = ProjectSidebarState::default();
    assert!(!state.set_width(DEFAULT_PROJECT_SIDEBAR_WIDTH));
  }

  #[test]
  fn non_finite_width_should_not_corrupt_layout() {
    let mut state = ProjectSidebarState::default();
    assert!(!state.set_width(gpui::px(f32::NAN)));
    assert_eq!(state.width(), DEFAULT_PROJECT_SIDEBAR_WIDTH);
  }
}
