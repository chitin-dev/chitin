//! Root GPUI application state and rendering.
//!
//! `ChitinApp` owns desktop-level state such as the active activity, currently
//! opened project workspace, and project sidebar state.

use std::path::PathBuf;

use chitin_ui::{
  composite::{
    activity_bar::DEFAULT_ACTIVITY_BAR_WIDTH,
    panel::{PanelSplitAxis, PanelTabDrag},
    window_bar::DEFAULT_WINDOW_BAR_HEIGHT,
  },
  themes::builtins,
};
use chitin_utils::workspace::ProjectWorkspace;
use gpui::{
  AnyView, Context, CursorStyle, FocusHandle, InteractiveElement, MouseButton, Render, Window, div, prelude::*,
};

use crate::{
  components::{
    activity_bar::{ActiveActivity, ActivityBarControls, render_activity_bar},
    command_panel::{CommandPanelController, render_command_panel},
    document_area::{DocumentPanelState, render_document_area, state::DocumentPanelContent},
    project_sidebar::{ProjectSidebarState, render_project_sidebar},
    window_bar::{WindowBarControls, render_window_bar},
  },
  keybindings::{ToggleCommandPanel, ToggleWorkspace},
};

pub use crate::components::document_area::state::WgpuDocumentViewFactory;

/// Root state object rendered into the main GPUI window.
pub struct ChitinApp {
  /// Currently opened project workspace, if a path was accepted.
  pub(crate) workspace: Option<ProjectWorkspace>,
  /// Project workspace sidebar state owned by the app.
  pub(crate) project_sidebar_state: ProjectSidebarState,
  /// Focus handle used by project tree keyboard navigation.
  pub(crate) project_sidebar_focus: Option<FocusHandle>,
  /// Focus handle used by global workbench keyboard shortcuts.
  pub(crate) workbench_focus: Option<FocusHandle>,
  /// Focus handle used by document panel container shortcuts.
  pub(crate) document_panel_focus: Option<FocusHandle>,
  /// Document panel tree currently shown in the main document area.
  pub(crate) document_panels: DocumentPanelState,
  /// Currently selected top-level workbench activity.
  pub(crate) active_activity: ActiveActivity,
  /// Whether the project workspace sidebar is visible when Workspace is active.
  pub(crate) project_sidebar_visible: bool,
  /// Searchable command panel state, metadata, and focus ownership.
  pub(crate) command_panel: CommandPanelController,
  /// Primitive button state for platform window actions.
  pub(crate) window_bar_controls: Option<WindowBarControls>,
  /// Primitive button state for activity-bar navigation controls.
  pub(crate) activity_bar_controls: Option<ActivityBarControls>,
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
  /// * `project_path` is the filesystem path to open as the initial workspace.
  ///   When it is `None`, the process current directory is used.
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
            eprintln!("Failed to determine current directory for workspace: {}", err);
            (None, None)
          }
        }
      }
    };

    // The workspace root starts expanded so first-level entries are visible
    // immediately after opening the desktop.
    let project_sidebar_state =
      ProjectSidebarState::with_workspace_root(workspace.as_ref().map(|workspace| workspace.tree.root.path.as_path()));

    Self {
      workspace,
      project_sidebar_state,
      project_sidebar_focus: None,
      workbench_focus: None,
      document_panel_focus: None,
      document_panels: DocumentPanelState::empty(),
      active_activity: ActiveActivity::Workspace,
      project_sidebar_visible: true,
      command_panel: CommandPanelController::new(),
      window_bar_controls: None,
      activity_bar_controls: None,
    }
  }

  /// Creates app state with a preallocated project-sidebar focus handle.
  ///
  /// This constructor is used by the GPUI window setup so the project sidebar
  /// can receive keyboard navigation focus as soon as the desktop opens.
  ///
  /// # Parameters
  ///
  /// * `project_path` is forwarded to [`ChitinApp::new`] as the initial
  ///   workspace path.
  /// * `project_sidebar_focus` is the GPUI focus handle tracked by the project
  ///   sidebar key context.
  ///
  /// # Returns
  ///
  /// A [`ChitinApp`] initialized like [`ChitinApp::new`], but with
  /// `project_sidebar_focus` stored for subsequent renders.
  pub fn new_with_project_sidebar_focus(project_path: Option<PathBuf>, project_sidebar_focus: FocusHandle) -> Self {
    let mut app = Self::new(project_path);
    app.project_sidebar_focus = Some(project_sidebar_focus);
    app
  }

  /// Creates app state with an experimental WGPU document tab.
  ///
  /// # Parameters
  ///
  /// * `project_path` is forwarded to [`ChitinApp::new`] as the initial
  ///   workspace path.
  /// * `project_sidebar_focus` is the GPUI focus handle tracked by the project
  ///   sidebar key context.
  /// * `title` is the document tab title used for the WGPU viewport.
  /// * `wgpu_panel` is the GPUI view that renders WGPU content.
  /// * `clone_wgpu_panel` creates independent WGPU views for split panels.
  ///
  /// # Returns
  ///
  /// A [`ChitinApp`] initialized with the WGPU panel in the document area.
  pub fn new_with_wgpu_document_panel(
    project_path: Option<PathBuf>,
    project_sidebar_focus: FocusHandle,
    title: impl Into<String>,
    wgpu_panel: impl Into<AnyView>,
    clone_wgpu_panel: WgpuDocumentViewFactory,
  ) -> Self {
    let mut app = Self::new_with_project_sidebar_focus(project_path, project_sidebar_focus);
    app.document_panels = DocumentPanelState::with_content(DocumentPanelContent::wgpu_interactive(
      title,
      wgpu_panel.into(),
      clone_wgpu_panel,
    ));
    app
  }

  /// Returns the stable focus handle used by project-sidebar key dispatch.
  ///
  /// If the handle has not been preallocated by window setup, this method
  /// lazily creates one from the GPUI context and stores it for future renders.
  ///
  /// # Parameters
  ///
  /// * `cx` is the GPUI context used to allocate a focus handle when none exists.
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

  /// Returns the stable focus handle used by global workbench shortcuts.
  ///
  /// This focus handle is tracked on the always-rendered root layout. It gives
  /// global keybindings a dispatch path even when optional panels such as the
  /// project sidebar are hidden.
  ///
  /// # Parameters
  ///
  /// * `cx` is the GPUI context used to allocate a focus handle when none exists.
  ///
  /// # Returns
  ///
  /// A cloned [`FocusHandle`] for the root workbench layout.
  pub(crate) fn workbench_focus(&mut self, cx: &mut Context<Self>) -> FocusHandle {
    self.workbench_focus.get_or_insert_with(|| cx.focus_handle()).clone()
  }

  /// Returns the stable focus handle used by document panel tab shortcuts.
  ///
  /// This focus handle is tracked by the rendered panel container wrapper, so
  /// tab keybindings are active only after the document panel has keyboard
  /// focus.
  ///
  /// # Parameters
  ///
  /// * `cx` is the GPUI context used to allocate a focus handle when none exists.
  ///
  /// # Returns
  ///
  /// A cloned [`FocusHandle`] for the document panel container.
  pub(crate) fn document_panel_focus(&mut self, cx: &mut Context<Self>) -> FocusHandle {
    self
      .document_panel_focus
      .get_or_insert_with(|| cx.focus_handle())
      .clone()
  }

  /// Shows or hides the project workspace sidebar.
  ///
  /// When another workbench activity is active, toggling the workspace first
  /// switches back to [`ActiveActivity::Workspace`] and shows the sidebar. When
  /// Workspace is already active, toggling flips only sidebar visibility.
  ///
  /// # Parameters
  ///
  /// * `cx` is the GPUI context notified after the workbench state changes.
  ///
  /// # Returns
  ///
  /// This function returns `()` and mutates workbench activity/sidebar state.
  pub(crate) fn toggle_workspace(&mut self, cx: &mut Context<Self>) {
    self.toggle_workspace_state();
    cx.notify();
  }

  /// Toggles command-panel visibility for command dispatch without a window.
  ///
  /// # Parameters
  ///
  /// * `cx` is notified after panel visibility changes.
  ///
  /// # Returns
  ///
  /// This function returns `()` after updating controller state. Window-aware UI
  /// entry points should call [`CommandPanelController::toggle`] directly.
  pub(crate) fn toggle_command_panel(&mut self, cx: &mut Context<Self>) {
    if self.command_panel.toggle_without_focus() {
      cx.notify();
    }
  }

  /// Toggles the workspace sidebar and moves focus to a rendered target.
  ///
  /// Keyboard shortcuts need this variant because hiding the sidebar removes
  /// the project tree focus target from the dispatch tree. After the state
  /// transition, focus moves to the project tree when it is visible, otherwise
  /// it moves to the document panel or always-rendered workbench root.
  ///
  /// # Parameters
  ///
  /// * `window` is the GPUI window whose keyboard focus should be updated.
  /// * `cx` is the GPUI context notified after the workbench state changes.
  ///
  /// # Returns
  ///
  /// This function returns `()` and mutates sidebar visibility plus keyboard
  /// focus.
  pub(crate) fn toggle_workspace_with_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.toggle_workspace_state();
    let focus = self.workspace_toggle_focus_target(cx);

    window.focus(&focus, cx);
    cx.notify();
  }

  /// Returns the focus target that should receive focus after Workspace toggle.
  ///
  /// Visible project sidebars receive project-tree focus so keyboard
  /// navigation works immediately. Hidden sidebars return focus to the
  /// always-rendered document panel so panel-container keybindings remain
  /// active.
  ///
  /// # Parameters
  ///
  /// * `cx` is the GPUI context used to lazily allocate focus handles.
  ///
  /// # Returns
  ///
  /// A [`FocusHandle`] for either the project sidebar or document panel.
  pub(crate) fn workspace_toggle_focus_target(&mut self, cx: &mut Context<Self>) -> FocusHandle {
    if self.active_activity == ActiveActivity::Workspace && self.project_sidebar_visible {
      self.project_sidebar_focus(cx)
    } else {
      self.document_panel_focus(cx)
    }
  }

  /// Applies the workspace-sidebar toggle state transition.
  ///
  /// This helper contains only synchronous state mutation so it can be tested
  /// without constructing a GPUI window. Use [`ChitinApp::toggle_workspace`]
  /// from UI actions so the app is notified after the transition.
  ///
  /// # Parameters
  ///
  /// This method mutably borrows `self`.
  ///
  /// # Returns
  ///
  /// This function returns `()` after mutating activity/sidebar visibility.
  fn toggle_workspace_state(&mut self) {
    if self.active_activity == ActiveActivity::Workspace {
      self.project_sidebar_visible = !self.project_sidebar_visible;
    } else {
      self.active_activity = ActiveActivity::Workspace;
      self.project_sidebar_visible = true;
    }
  }

  /// Calculates the current document panel root width.
  ///
  /// # Parameters
  ///
  /// * `window_width` is the full GPUI window width.
  /// * `visible_sidebar_width` is the current width occupied by an open sidebar,
  ///   or zero when no sidebar is visible.
  ///
  /// # Returns
  ///
  /// The approximate width available to the document panel root after removing
  /// the activity bar and visible project sidebar.
  fn document_panel_root_width(window_width: gpui::Pixels, visible_sidebar_width: gpui::Pixels) -> gpui::Pixels {
    gpui::px(
      (f32::from(window_width) - f32::from(DEFAULT_ACTIVITY_BAR_WIDTH) - f32::from(visible_sidebar_width)).max(0.0),
    )
  }

  /// Calculates the current document panel root height.
  ///
  /// # Parameters
  ///
  /// * `window_height` is the full GPUI window height.
  ///
  /// # Returns
  ///
  /// The approximate height available to the document panel root after removing
  /// the window bar.
  fn document_panel_root_height(window_height: gpui::Pixels) -> gpui::Pixels {
    gpui::px((f32::from(window_height) - f32::from(DEFAULT_WINDOW_BAR_HEIGHT)).max(0.0))
  }

  /// Returns persistent primitive button state for platform window controls.
  fn window_bar_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) -> WindowBarControls {
    if let Some(controls) = self.window_bar_controls.as_ref() {
      return controls.clone();
    }

    let controls = WindowBarControls::new(cx);
    controls.subscribe(window, cx);
    self.window_bar_controls = Some(controls.clone());
    controls
  }

  /// Returns persistent primitive button state for activity-bar controls.
  fn activity_bar_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) -> ActivityBarControls {
    if let Some(controls) = self.activity_bar_controls.as_ref() {
      return controls.clone();
    }

    let controls = ActivityBarControls::new(cx);
    controls.subscribe(window, cx);
    self.activity_bar_controls = Some(controls.clone());
    controls
  }
}

impl Render for ChitinApp {
  /// Renders the root desktop workbench layout.
  ///
  /// The rendered layout contains the window bar, activity bar, optional
  /// project sidebar, and main document area. It also owns top-level pointer
  /// handling for resize and tab drags because pointer movement can occur
  /// outside the originating component after dragging starts.
  ///
  /// # Parameters
  ///
  /// * `window` is the GPUI window used to register platform window controls.
  /// * `cx` is the GPUI render context used to access the app entity and focus
  ///   handles.
  ///
  /// # Returns
  ///
  /// A GPUI element tree for the current Chitin desktop frame.
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
    if !cx.has_active_drag() {
      self.cancel_document_panel_tab_drag();
    }

    let theme = builtins::dark();
    let window_bar_controls = self.window_bar_controls(window, cx);
    let activity_bar_controls = self.activity_bar_controls(window, cx);
    let command_panel_search_input = self.command_panel_search_input(window, cx);
    self.command_panel_rcsb_form(window, cx);
    let app = cx.weak_entity();
    let workbench_focus = self.workbench_focus(cx);
    let project_sidebar_focus = self.project_sidebar_focus(cx);
    let document_panel_focus = self.document_panel_focus(cx);
    let project_sidebar_is_resizing = self.project_sidebar_state.is_resizing();
    let document_panel_resize_axis = self.document_panel_resize_axis();
    let visible_sidebar_width = if self.active_activity == ActiveActivity::Workspace && self.project_sidebar_visible {
      self.project_sidebar_state.resize.width()
    } else {
      gpui::px(0.0)
    };

    div()
      .flex()
      .flex_col()
      .size_full()
      .relative()
      .track_focus(&workbench_focus)
      .bg(theme.background.primary)
      .text_color(theme.text.primary)
      .on_action(cx.listener(|this, _: &ToggleWorkspace, window, cx| {
        this.toggle_workspace_with_focus(window, cx);
      }))
      .on_action(cx.listener(|this, _: &ToggleCommandPanel, window, cx| {
        if this.command_panel.toggle(window, cx) {
          cx.notify();
        }
      }))
      .capture_key_down({
        let app = app.clone();
        move |event, window, cx| {
          let _ = app.update(cx, |this, cx| {
            this.handle_command_panel_key(event, window, cx);
          });
        }
      })
      .when(project_sidebar_is_resizing, |layout| {
        layout.cursor(CursorStyle::ResizeLeftRight)
      })
      .when(
        document_panel_resize_axis == Some(PanelSplitAxis::Horizontal),
        |layout| layout.cursor(CursorStyle::ResizeLeftRight),
      )
      .when(document_panel_resize_axis == Some(PanelSplitAxis::Vertical), |layout| {
        layout.cursor(CursorStyle::ResizeUpDown)
      })
      .on_mouse_move({
        let app = app.clone();
        move |event, window, cx| {
          if !project_sidebar_is_resizing && document_panel_resize_axis.is_none() {
            return;
          }

          if project_sidebar_is_resizing {
            let _ = app.update(cx, |this, cx| {
              if this.project_sidebar_state.drag_resize(event.position.x) {
                cx.notify();
              }
            });
          }

          if let Some(axis) = document_panel_resize_axis {
            let bounds = window.bounds();
            let document_root_width = Self::document_panel_root_width(bounds.size.width, visible_sidebar_width);
            let document_root_height = Self::document_panel_root_height(bounds.size.height);
            let current_position = match axis {
              PanelSplitAxis::Horizontal => event.position.x,
              PanelSplitAxis::Vertical => event.position.y,
            };

            let _ = app.update(cx, |this, cx| {
              if this.drag_document_panel_resize(current_position, document_root_width, document_root_height) {
                cx.notify();
              }
            });
          }
        }
      })
      .on_drag_move::<PanelTabDrag>({
        let app = app.clone();
        move |_, _, cx| {
          let _ = app.update(cx, |this, cx| {
            if this.clear_document_panel_tab_drag_target() {
              cx.notify();
            }
          });
        }
      })
      .on_mouse_up(MouseButton::Left, {
        let app = app.clone();
        move |_, _, cx| {
          let _ = app.update(cx, |this, cx| {
            let sidebar_stopped = this.project_sidebar_state.stop_resize();
            let document_panel_stopped = this.stop_document_panel_resize();
            let document_tab_drag_cancelled = this.cancel_document_panel_tab_drag();

            if sidebar_stopped || document_panel_stopped || document_tab_drag_cancelled {
              cx.notify();
            }
          });
        }
      })
      .child(render_window_bar(theme, window_bar_controls))
      .child(
        div()
          .flex()
          .flex_1()
          .min_h_0()
          .child(render_activity_bar(self.active_activity, theme, activity_bar_controls))
          .when(
            self.active_activity == ActiveActivity::Workspace && self.project_sidebar_visible,
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
            &self.document_panels,
            theme,
            &document_panel_focus,
            app.clone(),
            cx,
          )),
      )
      .when(self.command_panel.is_open(), |layout| {
        layout.child(render_command_panel(
          &mut self.command_panel,
          theme,
          app,
          cx,
          command_panel_search_input,
        ))
      })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  /// Verifies that toggling Workspace hides its sidebar when it is active.
  #[test]
  fn toggle_workspace_state_should_hide_active_workspace_sidebar() {
    let mut app = ChitinApp::new(Some(PathBuf::from("/chitin-test-missing-workspace")));

    app.toggle_workspace_state();

    assert_eq!(app.active_activity, ActiveActivity::Workspace);
    assert!(!app.project_sidebar_visible);
  }

  /// Verifies that toggling Workspace reopens it from another activity.
  #[test]
  fn toggle_workspace_state_should_show_workspace_from_other_activity() {
    let mut app = ChitinApp::new(Some(PathBuf::from("/chitin-test-missing-workspace")));
    app.active_activity = ActiveActivity::Search;
    app.project_sidebar_visible = false;

    app.toggle_workspace_state();

    assert_eq!(app.active_activity, ActiveActivity::Workspace);
    assert!(app.project_sidebar_visible);
  }

  /// Verifies that new app state starts with an empty document panel.
  #[test]
  fn new_should_start_with_empty_document_panel() {
    let app = ChitinApp::new(Some(PathBuf::from("/chitin-test-missing-workspace")));

    assert!(app.document_panels.active_document().is_none());
  }

  /// Verifies that document panel width excludes fixed workbench chrome.
  #[test]
  fn document_panel_root_width_should_exclude_activity_bar_and_sidebar() {
    assert_eq!(
      ChitinApp::document_panel_root_width(gpui::px(1000.0), gpui::px(260.0)),
      gpui::px(1000.0 - 48.0 - 260.0)
    );
  }

  /// Verifies that document panel height excludes the window bar.
  #[test]
  fn document_panel_root_height_should_exclude_window_bar() {
    assert_eq!(ChitinApp::document_panel_root_height(gpui::px(760.0)), gpui::px(730.0));
  }
}
