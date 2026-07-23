//! Root GPUI application state and rendering.
//!
//! `ChitinApp` owns desktop-level state such as the active activity, currently
//! opened project workspace, and project sidebar state.

use std::path::PathBuf;

use chitin_core::{WorkspaceSummary, workspace::ProjectWorkspace};
use chitin_ui::themes::builtins;
use gpui::{
  Context, CursorStyle, FocusHandle, InteractiveElement, MouseButton, Render, Window, div,
  prelude::*,
};

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
  /// Focus handle used by project tree keyboard navigation.
  pub(crate) project_sidebar_focus: Option<FocusHandle>,
  /// File currently opened in the main document area.
  pub(crate) active_document: Option<OpenedProjectDocument>,
  /// Currently selected top-level workbench activity.
  pub(crate) active_activity: ActiveActivity,
}

impl ChitinApp {
  /// Creates root app state from an optional project path.
  ///
  /// If no path is provided, the current working directory is used. Workspace
  /// loading is shallow; child directories are loaded later when expanded.
  /// Workspace loading failures are logged so they can be distinguished from
  /// the "no workspace" state.
  ///
  /// # Parameters
  ///
  /// `project_path` is the filesystem path to open as the initial workspace.
  /// When it is `None`, the process current directory is used.
  ///
  /// # Returns
  ///
  /// A [`ChitinApp`] with workspace state initialized, the Files activity
  /// selected, and the project root expanded when workspace loading succeeds.
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
      project_sidebar_focus: None,
      active_document: None,
      active_activity: ActiveActivity::Workspace,
    }
  }

  /// Creates app state with a preallocated project-sidebar focus handle.
  ///
  /// This constructor is used by the GPUI window setup so the project sidebar
  /// can receive keyboard navigation focus as soon as the desktop opens.
  ///
  /// # Parameters
  ///
  /// `project_path` is forwarded to [`ChitinApp::new`] as the initial
  /// workspace path.
  ///
  /// `project_sidebar_focus` is the GPUI focus handle tracked by the project
  /// sidebar key context.
  ///
  /// # Returns
  ///
  /// A [`ChitinApp`] initialized like [`ChitinApp::new`], but with
  /// `project_sidebar_focus` stored for subsequent renders.
  pub(crate) fn new_with_project_sidebar_focus(
    project_path: Option<PathBuf>,
    project_sidebar_focus: FocusHandle,
  ) -> Self {
    let mut app = Self::new(project_path);
    app.project_sidebar_focus = Some(project_sidebar_focus);
    app
  }

  /// Returns the stable focus handle used by project-sidebar key dispatch.
  ///
  /// If the handle has not been preallocated by window setup, this method
  /// lazily creates one from the GPUI context and stores it for future renders.
  ///
  /// # Parameters
  ///
  /// `cx` is the GPUI context used to allocate a focus handle when none exists.
  ///
  /// # Returns
  ///
  /// A cloned [`FocusHandle`] for the project sidebar.
  pub(crate) fn project_sidebar_focus(&mut self, cx: &mut Context<Self>) -> FocusHandle {
    self
      .project_sidebar_focus
      .get_or_insert_with(|| cx.focus_handle())
      .clone()
  }
}

impl Render for ChitinApp {
  /// Renders the root desktop workbench layout.
  ///
  /// The rendered layout contains the window bar, activity bar, optional
  /// project sidebar, and main document area. It also owns top-level pointer
  /// handling for sidebar resize drags because resize movement can occur
  /// outside the sidebar bounds after dragging starts.
  ///
  /// # Parameters
  ///
  /// `_window` is the GPUI window being rendered. The current implementation
  /// does not need it directly.
  ///
  /// `cx` is the GPUI render context used to access the app entity and focus
  /// handles.
  ///
  /// # Returns
  ///
  /// A GPUI element tree for the current Chitin desktop frame.
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
    let theme = builtins::dark();
    let app = cx.weak_entity();
    let project_sidebar_focus = self.project_sidebar_focus(cx);

    div()
      .flex()
      .flex_col()
      .size_full()
      .bg(theme.background.primary)
      .text_color(theme.text.primary)
      .when(self.project_sidebar_state.is_resizing(), |layout| {
        layout.cursor(CursorStyle::ResizeLeftRight)
      })
      .on_mouse_move({
        let app = app.clone();
        move |event, _, cx| {
          let _ = app.update(cx, |this, cx| {
            if this.project_sidebar_state.drag_resize(event.position.x) {
              cx.notify();
            }
          });
        }
      })
      .on_mouse_up(MouseButton::Left, move |_, _, cx| {
        let _ = app.update(cx, |this, cx| {
          if this.project_sidebar_state.stop_resize() {
            cx.notify();
          }
        });
      })
      .child(render_window_bar(theme, cx))
      .child(
        div()
          .flex()
          .flex_1()
          .min_h_0()
          .child(render_activity_bar(self.active_activity, theme, cx))
          .when(
            self.active_activity == ActiveActivity::Workspace,
            |layout| {
              layout.child(render_project_sidebar(
                self.workspace.as_ref(),
                &self.project_sidebar_state,
                &project_sidebar_focus,
                theme,
                cx,
              ))
            },
          )
          .child(render_document_area(
            self.active_document.as_ref(),
            &self.summary,
            self.active_activity,
            theme,
          )),
      )
  }
}
