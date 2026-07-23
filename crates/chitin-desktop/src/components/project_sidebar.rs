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
use gpui::{Context, FocusHandle, IntoElement, Pixels, div, prelude::*};

use crate::{
  app::ChitinApp,
  commands::{
    WorkspaceCommand,
    workspace::{
      ActivateFocusedEntry, FocusFirstEntry, FocusLastEntry, FocusNextEntry, FocusPreviousEntry,
      PROJECT_TREE_KEY_CONTEXT,
    },
  },
  components::workspace_tree::render_workspace_tree,
};

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
  ///
  /// # Parameters
  ///
  /// `root` is the optional workspace root path to mark as expanded.
  ///
  /// # Returns
  ///
  /// A [`ProjectSidebarState`] with empty selection/focus state and default
  /// resize state.
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
  ///
  /// # Parameters
  ///
  /// `path` is the filesystem path to store as the selected project entry.
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
  /// `path` is the filesystem path to store as the focused project entry.
  ///
  /// # Returns
  ///
  /// This function returns `()` and mutates `focused_path`.
  pub fn focus_entry(&mut self, path: &Path) {
    self.focused_path = Some(path.to_path_buf());
  }

  /// Starts a sidebar resize drag at the current cursor position.
  ///
  /// # Parameters
  ///
  /// `start_x` is the horizontal cursor position where dragging began.
  ///
  /// # Returns
  ///
  /// This function returns `()` and records the active resize drag.
  pub fn start_resize(&mut self, start_x: Pixels) {
    log::debug!(
      "Project sidebar: start width resizing from width {:?}",
      start_x
    );
    self.resize.start_resize(start_x);
  }

  /// Updates sidebar width from the current resize cursor position.
  ///
  /// # Parameters
  ///
  /// `current_x` is the latest horizontal cursor position during the drag.
  ///
  /// # Returns
  ///
  /// `true` when an active drag was updated; `false` when no drag is active.
  pub fn drag_resize(&mut self, current_x: Pixels) -> bool {
    self.resize.drag_resize(current_x)
  }

  /// Stops the current sidebar resize drag.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows `self` to clear resize state.
  ///
  /// # Returns
  ///
  /// `true` when a drag was active and has been stopped; otherwise `false`.
  pub fn stop_resize(&mut self) -> bool {
    log::debug!("Project sidebar: stop width resizing from");
    self.resize.stop_resize()
  }

  /// Returns whether the sidebar is currently being resized.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// `true` when a resize drag is active; otherwise `false`.
  pub fn is_resizing(&self) -> bool {
    self.resize.is_resizing()
  }
}

impl Default for ProjectSidebarState {
  /// Creates project sidebar state with no workspace root.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A [`ProjectSidebarState`] with no expanded root.
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
/// `workspace` is the currently opened project workspace. When `None`, the
/// sidebar renders an empty-workspace message instead of a tree.
///
/// `state` contains expansion, loading, selection, focus, and resize state used
/// by the sidebar and tree.
///
/// `focus_handle` is the GPUI focus handle associated with the `"ProjectTree"`
/// key context.
///
/// `theme` supplies the UI colors and spacing used by the sidebar shell.
///
/// `cx` is the GPUI context used to create command action listeners and obtain
/// a weak app entity for resize callbacks.
///
/// # Returns
///
/// A GPUI element that renders the resizable project sidebar.
pub fn render_project_sidebar(
  workspace: Option<&ProjectWorkspace>,
  state: &ProjectSidebarState,
  focus_handle: &FocusHandle,
  theme: UIThemes,
  cx: &mut Context<ChitinApp>,
) -> impl IntoElement {
  let app = cx.weak_entity();

  div()
    .track_focus(focus_handle)
    .key_context(PROJECT_TREE_KEY_CONTEXT)
    .on_action(cx.listener(|this, _: &FocusPreviousEntry, _, cx| {
      this.dispatch_command(WorkspaceCommand::FocusPrevious.into(), cx);
    }))
    .on_action(cx.listener(|this, _: &FocusNextEntry, _, cx| {
      this.dispatch_command(WorkspaceCommand::FocusNext.into(), cx);
    }))
    .on_action(cx.listener(|this, _: &ActivateFocusedEntry, _, cx| {
      this.dispatch_command(WorkspaceCommand::ActivateFocused.into(), cx);
    }))
    .on_action(cx.listener(|this, _: &FocusFirstEntry, _, cx| {
      this.dispatch_command(WorkspaceCommand::FocusFirst.into(), cx);
    }))
    .on_action(cx.listener(|this, _: &FocusLastEntry, _, cx| {
      this.dispatch_command(WorkspaceCommand::FocusLast.into(), cx);
    }))
    .child(
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
        ),
    )
}

#[cfg(test)]
mod tests {
  use chitin_ui::components::sidebar::DEFAULT_SIDEBAR_WIDTH;

  use super::*;

  /// Verifies that drag resize applies cursor delta to the starting width.
  #[test]
  /// # Parameters
  ///
  /// This test takes no parameters.
  ///
  /// # Returns
  ///
  /// This test returns `()` and panics if drag width math regresses.
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
  /// # Parameters
  ///
  /// This test takes no parameters.
  ///
  /// # Returns
  ///
  /// This test returns `()` and panics if resize state remains active.
  fn stop_resize_should_clear_active_resize_state() {
    let mut state = ProjectSidebarState::default();

    state.start_resize(gpui::px(100.0));
    assert!(state.is_resizing());
    assert!(state.stop_resize());
    assert!(!state.is_resizing());
  }
}
