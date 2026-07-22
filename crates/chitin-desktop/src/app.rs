//! Root GPUI application state and rendering.
//!
//! `ChitinApp` owns desktop-level state such as the active activity, currently
//! opened project workspace, and project sidebar state.

use std::path::PathBuf;

use chitin_core::{WorkspaceSummary, workspace::ProjectWorkspace};
use chitin_ui::themes::builtins;
use gpui::{Context, Render, Window, div, prelude::*};

use crate::components::{
  activity_bar::{ActiveActivity, render_activity_bar},
  document_area::{OpenedProjectDocument, render_document_area},
  project_sidebar::{ProjectSidebarState, render_project_sidebar},
  window_bar::render_window_bar,
};

/// Root state object rendered into the main GPUI window.
pub struct ChitinApp {
  /// Static product summary used by the placeholder main panel.
  summary: WorkspaceSummary,
  /// Currently opened project workspace, if a path was accepted.
  pub(crate) workspace: Option<ProjectWorkspace>,
  /// Project workspace sidebar state owned by the app.
  pub(crate) project_sidebar_state: ProjectSidebarState,
  /// File currently opened in the main document area.
  pub(crate) active_document: Option<OpenedProjectDocument>,
  /// Currently selected top-level workbench activity.
  pub(crate) active_activity: ActiveActivity,
}

impl ChitinApp {
  /// Creates app state from an optional project path.
  ///
  /// If no path is provided, the current working directory is used. Workspace
  /// loading is shallow; child directories are loaded later when expanded.
  ///
  /// Workspace loading failures are logged so they can be distinguished from
  /// the "no workspace" state.
  pub fn new(project_path: Option<PathBuf>) -> Self {
    let (_, workspace) = match project_path {
      Some(path) => match ProjectWorkspace::open(path.clone()) {
        Ok(workspace) => (Some(path), Some(workspace)),
        Err(err) => {
          eprintln!(
            "Failed to open workspace for project path '{}': {}",
            path.display(),
            err
          );
          (Some(path), None)
        }
      },
      None => {
        // when no path is provided, current working directory is used
        match std::env::current_dir() {
          Ok(path) => match ProjectWorkspace::open(path.clone()) {
            Ok(workspace) => (Some(path), Some(workspace)),
            Err(err) => {
              eprintln!(
                "Failed to open workspace for current directory '{}': {}",
                path.display(),
                err
              );
              (Some(path), None)
            }
          },
          Err(err) => {
            eprintln!(
              "Failed to determine current directory for workspace: {}",
              err
            );
            (None, None)
          }
        }
      }
    };

    // The workspace root starts expanded so first-level entries are visible
    // immediately after opening the desktop.
    let project_sidebar_state = ProjectSidebarState::with_workspace_root(
      workspace
        .as_ref()
        .map(|workspace| workspace.tree.root.path.as_path()),
    );

    Self {
      summary: WorkspaceSummary::default(),
      workspace,
      project_sidebar_state,
      active_document: None,
      active_activity: ActiveActivity::Files,
    }
  }
}

impl Render for ChitinApp {
  /// Renders the root desktop workbench layout.
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
    let theme = builtins::dark();

    div()
      .flex()
      .flex_col()
      .size_full()
      .bg(theme.background.primary)
      .text_color(theme.text.primary)
      .child(render_window_bar(theme, cx))
      .child(
        div()
          .flex()
          .flex_1()
          .min_h_0()
          .child(render_activity_bar(self.active_activity, theme, cx))
          .when(self.active_activity == ActiveActivity::Files, |layout| {
            layout.child(render_project_sidebar(
              self.workspace.as_ref(),
              &self.project_sidebar_state,
              theme,
              cx,
            ))
          })
          .child(render_document_area(
            self.active_document.as_ref(),
            &self.summary,
            self.active_activity,
            theme,
          )),
      )
  }
}
