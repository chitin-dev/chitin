//! Root GPUI application state and rendering.
//!
//! `ChitinApp` owns desktop-level state such as the primary sidebar view, currently
//! opened project workspace, and project sidebar state.

use std::{collections::BTreeSet, path::PathBuf};

use chitin_command::CommandExecutor;
use chitin_databases::ClientConfig;
use chitin_ui::{
  widgets::toast::{Toast, ToastHost, ToastId, ToastViewport},
  workbench::{WorkbenchStyle, bottom_dock::BottomDockState, resize_handle_appearance},
};
use chitin_utils::workspace::ProjectWorkspace;
use gpui::{Context, Entity, FocusHandle, InteractiveElement, Render, Window, div, prelude::*};
use gpui_kit::component::resizable::{h_resizable, resizable_panel};

use crate::{
  commands::{
    portable::DesktopPortableCommandRunner,
    shell_host::{DesktopShellHost, desktop_shell_context},
  },
  features::{
    command_palette::{CommandPanelController, render_command_panel},
    molecule::options::DocumentOptionsControls,
    terminal::{TERMINAL_DOCK_ITEM_ID, TerminalPanelControls, TerminalPanelState, render_terminal_bottom_dock},
  },
  keybindings::{
    RedoWorkbenchEdit, ToggleCommandPanel, ToggleTerminal, ToggleWorkspace, UndoWorkbenchEdit, WORKBENCH_KEY_CONTEXT,
  },
  services::tasks::BackgroundTaskCenter,
  workbench::{
    documents::{DocumentPanelState, dock::DocumentDock, render_document_area, state::DocumentPanelContent},
    explorer::{MAX_PROJECT_SIDEBAR_WIDTH, MIN_PROJECT_SIDEBAR_WIDTH, ProjectSidebarState},
    sidebar::{SidebarView, render_primary_sidebar},
    welcome::render_welcome,
    window_bar::render_window_bar,
  },
};

pub use crate::workbench::documents::state::{WgpuDocumentView, WgpuDocumentViewFactory};

/// Root state object rendered into the main GPUI window.
pub struct ChitinApp {
  pub(crate) edit_history: crate::commands::history::WorkbenchHistory,
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
  /// Kit docking mount, separate from document command state.
  pub(crate) document_dock: Option<DocumentDock>,
  /// Content selected by the primary sidebar's navigation.
  pub(crate) sidebar_view: SidebarView,
  /// Remembered expansion of the Functions sidebar's database category.
  pub(crate) database_functions_expanded: bool,
  /// Scientific entry caches, selection inputs, and view-scoped bookmarks.
  pub(crate) entries_sidebar: crate::workbench::entries::EntriesSidebarState,
  /// Whether the primary sidebar is visible, independently of its selected view.
  pub(crate) project_sidebar_visible: bool,
  /// Searchable command panel state, metadata, and focus ownership.
  pub(crate) command_panel: CommandPanelController,
  /// Application-wide executor and registry for background work.
  pub(crate) tasks: BackgroundTaskCenter,
  /// Shared adapter for frontend-independent commands submitted by desktop views.
  pub(crate) portable_commands: DesktopPortableCommandRunner,
  /// Application-level command bridge for agent and system invocations.
  pub(crate) builtin_shell: DesktopShellHost,
  /// Active tool and geometry of the workbench-level bottom dock.
  pub(crate) bottom_dock: BottomDockState,
  /// Scrollback presentation, shell mapping, and focus state for the terminal.
  pub(crate) terminal_panel: TerminalPanelState,
  /// Persistent primitive controls for the bottom terminal.
  pub(crate) terminal_panel_controls: Option<TerminalPanelControls>,
  /// Structure paths currently being read and parsed off the UI thread.
  pub(crate) pending_structure_loads: BTreeSet<PathBuf>,
  /// Persistent semantic controls for molecular document options.
  pub(crate) document_options_controls: Option<DocumentOptionsControls>,
  /// Window-level transient notification queue.
  pub(crate) toast_viewport: Option<Entity<ToastViewport>>,
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

    let shell_workspace_root = workspace.as_ref().map(|workspace| workspace.root.clone());
    let command_executor = CommandExecutor::new(ClientConfig::default());
    Self {
      workspace,
      project_sidebar_state,
      project_sidebar_focus: None,
      workbench_focus: None,
      document_panel_focus: None,
      document_panels: DocumentPanelState::empty(),
      edit_history: Default::default(),
      document_dock: None,
      sidebar_view: SidebarView::Project,
      database_functions_expanded: true,
      entries_sidebar: Default::default(),
      project_sidebar_visible: true,
      command_panel: CommandPanelController::new(),
      tasks: BackgroundTaskCenter::new(),
      portable_commands: DesktopPortableCommandRunner::new(command_executor),
      builtin_shell: DesktopShellHost::new(desktop_shell_context(shell_workspace_root)),
      bottom_dock: BottomDockState::new(),
      terminal_panel: TerminalPanelState::new(),
      terminal_panel_controls: None,
      pending_structure_loads: BTreeSet::new(),
      document_options_controls: None,
      toast_viewport: None,
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
    wgpu_panel: WgpuDocumentView,
    clone_wgpu_panel: WgpuDocumentViewFactory,
  ) -> Self {
    let title = title.into();
    let mut app = Self::new_with_project_sidebar_focus(project_path, project_sidebar_focus);
    app.document_panels = DocumentPanelState::with_content(DocumentPanelContent::wgpu_interactive(
      Some(PathBuf::from(&title)),
      title,
      wgpu_panel,
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
    if self.sidebar_view == SidebarView::Project && self.project_sidebar_visible {
      self.project_sidebar_focus(cx)
    } else {
      self.document_panel_focus(cx)
    }
  }

  /// Applies the workspace-sidebar toggle state transition.
  fn toggle_workspace_state(&mut self) {
    if self.sidebar_view == SidebarView::Project {
      self.project_sidebar_visible = !self.project_sidebar_visible;
    } else {
      self.sidebar_view = SidebarView::Project;
      self.project_sidebar_visible = true;
    }
  }

  /// Returns persistent semantic controls for molecular document options.
  fn document_options_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) -> DocumentOptionsControls {
    if let Some(controls) = self.document_options_controls.as_ref() {
      return controls.clone();
    }

    let controls = DocumentOptionsControls::new(window, cx);
    controls.subscribe(window, cx);
    self.document_options_controls = Some(controls.clone());
    controls
  }

  /// Adds a transient notification to the global bottom-right viewport.
  pub fn show_toast(&mut self, toast: Toast, cx: &mut Context<Self>) -> ToastId {
    let viewport = self.toast_viewport(cx);
    viewport.update(cx, |viewport, cx| viewport.push(toast, cx))
  }

  /// Returns the lazily created window-level notification viewport.
  pub(crate) fn toast_viewport(&mut self, cx: &mut Context<Self>) -> Entity<ToastViewport> {
    self
      .toast_viewport
      .get_or_insert_with(|| cx.new(|_| ToastViewport::new()))
      .clone()
  }
}

impl Render for ChitinApp {
  /// Renders the root desktop workbench layout.
  ///
  /// The rendered layout contains the window bar, optional primary
  /// sidebar, and main document area. Kit owns pointer tracking for
  /// explorer resizing, document docking, and the independent bottom overlay.
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
    let theme = gpui_kit::component::Theme::global(cx).colors;
    let workbench_style = WorkbenchStyle::new(theme);
    self.sync_shell_rendering_panels();
    let document_options_controls = self.document_options_controls(window, cx);
    let toast_viewport = self.toast_viewport(cx);
    let command_palette = self.command_panel.palette(window, cx);
    self.command_panel_rcsb_form(window, cx);
    let terminal_dock_controls = if self.bottom_dock.is_active(TERMINAL_DOCK_ITEM_ID) {
      self.terminal_panel_controls(window, cx)
    } else {
      None
    };
    if terminal_dock_controls.is_some() {
      self.sync_terminal_output();
    }
    if self.terminal_panel.take_focus_request()
      && let Some(controls) = terminal_dock_controls.as_ref()
      && let Some(focus) = controls.focus(cx)
    {
      window.focus(&focus, cx);
    }
    let app = cx.weak_entity();
    let workbench_focus = self.workbench_focus(cx);
    let document_panel_focus = self.document_panel_focus(cx);
    let mut document_dock = self
      .document_dock
      .take()
      .unwrap_or_else(|| DocumentDock::new(window, cx));
    document_dock.sync(&self.document_panels, document_options_controls, window, cx);
    self.edit_history.prune(&self.document_panels);
    self.entries_sidebar.prune(&self.document_panels);
    let document_area = document_dock.area.clone();
    self.document_dock = Some(document_dock);
    let bottom_dock_height = self.bottom_dock.height();

    let document_body = if self.document_panels.tree.tabs_in_order().is_empty() {
      render_welcome(self, theme, cx).into_any_element()
    } else {
      document_area.into_any_element()
    };
    let document_content = div()
      .relative()
      .flex()
      .flex_col()
      .size_full()
      .min_w_0()
      .min_h_0()
      .child(render_document_area(document_body, &document_panel_focus, cx))
      .when_some(terminal_dock_controls, |layout, controls| {
        layout.child(render_terminal_bottom_dock(
          controls,
          bottom_dock_height,
          theme,
          app.clone(),
        ))
      });
    let workbench_content = if self.project_sidebar_visible {
      let resize_app = app.clone();
      h_resizable("workspace-explorer-split")
        .with_handle_appearance(resize_handle_appearance())
        .child(
          resizable_panel()
            .size(self.project_sidebar_state.width())
            .size_range(MIN_PROJECT_SIDEBAR_WIDTH..MAX_PROJECT_SIDEBAR_WIDTH)
            .child(render_primary_sidebar(self, theme, window, cx)),
        )
        .child(resizable_panel().child(document_content))
        .on_resize(move |state, _, cx| {
          let Some(width) = state.read(cx).sizes().first().copied() else {
            return;
          };
          let _ = resize_app.update(cx, |app, cx| {
            if app.project_sidebar_state.set_width(width) {
              cx.notify();
            }
          });
        })
        .into_any_element()
    } else {
      document_content.into_any_element()
    };

    div()
      .flex()
      .flex_col()
      .size_full()
      .relative()
      .key_context(WORKBENCH_KEY_CONTEXT)
      .track_focus(&workbench_focus)
      .bg(workbench_style.background)
      .text_sm()
      .text_color(theme.foreground)
      .on_action(cx.listener(|this, _: &ToggleWorkspace, window, cx| {
        this.toggle_workspace_with_focus(window, cx);
      }))
      .on_action(cx.listener(|this, _: &ToggleCommandPanel, window, cx| {
        if this.command_panel.toggle(window, cx) {
          cx.notify();
        }
      }))
      .on_action(cx.listener(|this, _: &ToggleTerminal, _, cx| {
        this.toggle_terminal(cx);
      }))
      .on_action(cx.listener(|this, _: &UndoWorkbenchEdit, _, cx| {
        if let Err(error) = this.dispatch_history_command(false, cx) {
          log::error!("{error}");
        }
      }))
      .on_action(cx.listener(|this, _: &RedoWorkbenchEdit, _, cx| {
        if let Err(error) = this.dispatch_history_command(true, cx) {
          log::error!("{error}");
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
      .child(render_window_bar(theme, app.clone()))
      .child(
        div().flex().flex_1().min_h_0().child(
          div()
            .flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .p(workbench_style.panel_gap * 0.5)
            .child(workbench_content),
        ),
      )
      .when(self.command_panel.is_open(), |layout| {
        layout.child(render_command_panel(
          &mut self.command_panel,
          theme,
          app,
          cx,
          command_palette,
          window,
        ))
      })
      .child(ToastHost::new(toast_viewport))
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

    assert_eq!(app.sidebar_view, SidebarView::Project);
    assert!(!app.project_sidebar_visible);
  }

  /// Verifies that toggling Workspace reopens it from another activity.
  #[test]
  fn toggle_workspace_state_should_show_workspace_from_other_activity() {
    let mut app = ChitinApp::new(Some(PathBuf::from("/chitin-test-missing-workspace")));
    app.sidebar_view = SidebarView::Functions;
    app.project_sidebar_visible = false;

    app.toggle_workspace_state();

    assert_eq!(app.sidebar_view, SidebarView::Project);
    assert!(app.project_sidebar_visible);
  }

  /// Verifies that new app state starts with an empty document panel.
  #[test]
  fn new_should_start_with_empty_document_panel() {
    let app = ChitinApp::new(Some(PathBuf::from("/chitin-test-missing-workspace")));

    assert!(app.document_panels.active_document().is_none());
  }
}
