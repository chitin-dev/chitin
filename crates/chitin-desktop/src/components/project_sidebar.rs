//! Project sidebar composition.
//!
//! This module combines generic `chitin-ui` sidebar primitives with the
//! desktop-specific workspace tree renderer.

use std::{
  collections::HashSet,
  path::{Path, PathBuf},
};

use chitin_core::workspace::ProjectWorkspace;
use chitin_ui::{
  components::sidebar::{
    Sidebar, SidebarBody, SidebarHeader, SidebarResizeConfig, SidebarResizeState, SidebarSection,
    SidebarTitle,
  },
  themes::UIThemes,
};
use gpui::{Context, IntoElement, Pixels, div, prelude::*};

use crate::{app::ChitinApp, components::workspace_tree::render_workspace_tree};

/// Default title shown at the top of the project workspace sidebar.
pub const DEFAULT_PROJECT_WORKSPACE_TITLE: &str = "EXPLORER";

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
  /// Generic resize state for the project sidebar shell.
  pub resize: SidebarResizeState,
}

impl ProjectSidebarState {
  /// Creates sidebar state with the workspace root expanded when present.
  pub fn with_workspace_root(root: Option<&Path>) -> Self {
    Self {
      expanded_paths: root
        .map(|root| HashSet::from([root.to_path_buf()]))
        .unwrap_or_default(),
      loading_paths: HashSet::new(),
      selected_path: None,
      focused_path: None,
      resize: SidebarResizeState::default(),
    }
  }

  /// Selects the workspace tree entry that backs the active opened document.
  pub fn select_entry(&mut self, path: &Path) {
    self.selected_path = Some(path.to_path_buf());
  }

  /// Focuses a workspace tree entry for keyboard navigation.
  pub fn focus_entry(&mut self, path: &Path) {
    self.focused_path = Some(path.to_path_buf());
  }

  /// Starts a sidebar resize drag at the current cursor position.
  pub fn start_resize(&mut self, start_x: Pixels) {
    self.resize.start_resize(start_x);
  }

  /// Updates sidebar width from the current resize cursor position.
  pub fn drag_resize(&mut self, current_x: Pixels) -> bool {
    self.resize.drag_resize(current_x)
  }

  /// Stops the current sidebar resize drag.
  pub fn stop_resize(&mut self) -> bool {
    self.resize.stop_resize()
  }

  /// Returns whether the sidebar is currently being resized.
  pub fn is_resizing(&self) -> bool {
    self.resize.is_resizing()
  }
}

impl Default for ProjectSidebarState {
  /// Creates project sidebar state with no workspace root.
  fn default() -> Self {
    Self::with_workspace_root(None)
  }
}

/// Renders the project workspace sidebar.
///
/// The sidebar itself is generic composition, while the file tree inside it is
/// desktop-specific because it uses Chitin's workspace SVG icon assets and
/// dispatches expansion events to [`ChitinApp`].
pub fn render_project_sidebar(
  workspace: Option<&ProjectWorkspace>,
  state: &ProjectSidebarState,
  theme: UIThemes,
  cx: &mut Context<ChitinApp>,
) -> impl IntoElement {
  let app = cx.weak_entity();

  Sidebar::new()
    .width(state.resize.width())
    .resizable(SidebarResizeConfig::new(move |start_x, _, cx| {
      let _ = app.update(cx, |this, cx| {
        this.project_sidebar_state.start_resize(start_x);
        cx.notify();
      });
    }))
    .theme(theme)
    .child(
      SidebarHeader::new()
        .theme(theme)
        .child(SidebarTitle::new(DEFAULT_PROJECT_WORKSPACE_TITLE).theme(theme)),
    )
    .child(
      SidebarBody::new().theme(theme).child(match workspace {
        Some(workspace) => {
          SidebarSection::new()
            .theme(theme)
            .fill(true)
            .child(render_workspace_tree(
              &workspace.tree.root,
              state,
              theme,
              cx,
            ))
        }
        None => SidebarSection::new().theme(theme).child(
          div()
            .p_3()
            .text_xs()
            .text_color(theme.text.secondary)
            .child("Open a project path to show files."),
        ),
      }),
    )
}

#[cfg(test)]
mod tests {
  use chitin_ui::components::sidebar::DEFAULT_SIDEBAR_WIDTH;

  use super::*;

  /// Verifies that drag resize applies cursor delta to the starting width.
  #[test]
  fn drag_resize_should_apply_delta_from_drag_start() {
    let mut state = ProjectSidebarState::default();

    state.start_resize(gpui::px(100.0));
    assert!(state.drag_resize(gpui::px(140.0)));

    assert_eq!(
      state.resize.width(),
      gpui::px(f32::from(DEFAULT_SIDEBAR_WIDTH) + 40.0)
    );
  }

  /// Verifies that stopping resize clears active resize state.
  #[test]
  fn stop_resize_should_clear_active_resize_state() {
    let mut state = ProjectSidebarState::default();

    state.start_resize(gpui::px(100.0));
    assert!(state.is_resizing());
    assert!(state.stop_resize());
    assert!(!state.is_resizing());
  }
}
