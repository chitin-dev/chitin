//! Desktop adapter for terminal sessions hosted by the workbench bottom dock.

mod completion;
mod controller;
mod presenter;
mod render;

use std::{
  path::{Path, PathBuf},
  sync::{Arc, Mutex},
};

use chitin_builtin_shell::{BuiltinTerminalProgram, ShellCommandId};
use chitin_terminal::{TerminalProfile, TerminalSession, TerminalSessionError, TerminalSize};
use chitin_ui::{
  composite::toast::{Toast, ToastVariant},
  primitive::{
    button::{ButtonEvent, ButtonState},
    input::select::{SelectInputEvent, SelectInputState, SelectOption},
    terminal::{TerminalEmulatorEvent, TerminalEmulatorState},
  },
};
use gpui::{AppContext, Context, Entity, ScrollHandle, Subscription, Window};

use crate::app::ChitinApp;

pub(crate) use render::render_terminal_bottom_dock;

pub(crate) const TERMINAL_DOCK_ITEM_ID: &str = "terminal";
const INITIAL_TERMINAL_SIZE: TerminalSize = TerminalSize::new(80, 24, 9, 21);

/// Returns the desktop asset used to represent one terminal profile.
pub(super) const fn terminal_profile_icon(profile: TerminalProfile) -> &'static str {
  match profile {
    TerminalProfile::BuiltinShell => "icons/terminal-builtin.svg",
    TerminalProfile::SystemShell => "icons/terminal-shell.svg",
    TerminalProfile::Bash => "icons/terminal-bash.svg",
    TerminalProfile::Fish => "icons/terminal-fish.svg",
  }
}

/// Stable identity of one terminal session in the bottom dock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TerminalSessionId(u64);

/// Terminal focus state that belongs to the bottom dock rather than a session.
pub(crate) struct TerminalPanelState {
  focus_requested: bool,
}

impl TerminalPanelState {
  /// Creates a closed terminal panel.
  pub(crate) fn new() -> Self {
    Self { focus_requested: false }
  }

  /// Updates whether the active session should receive focus on the next render.
  pub(crate) fn request_focus(&mut self, requested: bool) {
    self.focus_requested = requested;
  }

  /// Takes the pending input-focus request.
  pub(crate) fn take_focus_request(&mut self) -> bool {
    std::mem::take(&mut self.focus_requested)
  }
}

impl Default for TerminalPanelState {
  fn default() -> Self {
    Self::new()
  }
}

/// One independently rendered terminal surface and its optional in-process program.
#[derive(Clone)]
pub(super) struct ManagedTerminalSession {
  pub(super) id: TerminalSessionId,
  pub(super) profile: TerminalProfile,
  pub(super) terminal: Entity<TerminalEmulatorState>,
  pub(super) program: Option<Arc<Mutex<BuiltinTerminalProgram>>>,
  pub(super) tab: Entity<ButtonState>,
  pub(super) active_shell_command: Option<ShellCommandId>,
  pub(super) rendered_running_output: String,
}

impl ManagedTerminalSession {
  /// Tracks the portable command currently writing into this session.
  fn start_command(&mut self, command_id: ShellCommandId) {
    self.active_shell_command = Some(command_id);
    self.rendered_running_output.clear();
  }

  /// Clears this session's replaceable command-output projection.
  fn finish_command(&mut self) {
    self.active_shell_command = None;
    self.rendered_running_output.clear();
  }
}

/// Persistent session manager and controls for the bottom terminal panel.
#[derive(Clone)]
pub(crate) struct TerminalPanelControls {
  pub(super) sessions: Vec<ManagedTerminalSession>,
  pub(super) active_session: TerminalSessionId,
  next_session_id: u64,
  pub(super) close_session: Entity<ButtonState>,
  pub(super) profile_select: Entity<SelectInputState>,
  /// Scroll position of the session tab strip.
  ///
  /// This has to outlive a render: a handle created while rendering would start every
  /// frame at the top of the strip and the lane would never move.
  pub(super) tab_scroll: ScrollHandle,
}

impl TerminalPanelControls {
  /// Creates the session manager with an initial built-in shell.
  fn new(working_directory: &Path, cx: &mut Context<ChitinApp>) -> Result<Self, TerminalSessionError> {
    let close_session = cx.new(ButtonState::new);
    let profile_select = cx.new(|cx| {
      SelectInputState::new(
        TerminalProfile::ALL
          .into_iter()
          .map(|profile| SelectOption::new(profile.id(), profile.label()).icon(terminal_profile_icon(profile))),
        cx,
      )
    });
    let session = Self::build_session(
      TerminalSessionId(1),
      TerminalProfile::BuiltinShell,
      working_directory,
      cx,
    )?;
    Ok(Self {
      sessions: vec![session],
      active_session: TerminalSessionId(1),
      next_session_id: 2,
      close_session,
      profile_select,
      tab_scroll: ScrollHandle::new(),
    })
  }

  /// Builds a terminal surface for one launch profile.
  fn build_session(
    id: TerminalSessionId,
    profile: TerminalProfile,
    working_directory: &Path,
    cx: &mut Context<ChitinApp>,
  ) -> Result<ManagedTerminalSession, TerminalSessionError> {
    let (session, program) = if profile == TerminalProfile::BuiltinShell {
      let prompt = presenter::terminal_prompt_ansi(working_directory);
      let (session, program) = BuiltinTerminalProgram::connect(INITIAL_TERMINAL_SIZE, prompt)?;
      (session, Some(Arc::new(Mutex::new(program))))
    } else {
      (
        TerminalSession::spawn_system_shell(profile, working_directory, INITIAL_TERMINAL_SIZE)?,
        None,
      )
    };
    Ok(ManagedTerminalSession {
      id,
      profile,
      terminal: cx.new(|cx| TerminalEmulatorState::new(session, INITIAL_TERMINAL_SIZE, cx)),
      program,
      tab: cx.new(ButtonState::new),
      active_shell_command: None,
      rendered_running_output: String::new(),
    })
  }

  /// Connects session-manager and initial-session semantic controls.
  fn subscribe(&self, window: &mut Window, cx: &mut Context<ChitinApp>) {
    self.subscribe_session(&self.sessions[0], window, cx);

    let subscription: Subscription = cx.subscribe_in(&self.close_session, window, |this, _, event, _, cx| {
      if matches!(event, ButtonEvent::Click) {
        this.close_active_terminal_session(cx);
      }
    });
    subscription.detach();

    let profile_select = self.profile_select.clone();
    let profile_select_for_event = profile_select.clone();
    let subscription: Subscription = cx.subscribe_in(&profile_select, window, move |this, _, event, window, cx| {
      let SelectInputEvent::SelectionChange {
        selected_id: Some(selected_id),
      } = event
      else {
        return;
      };
      let Some(profile) = TerminalProfile::from_id(selected_id.as_ref()) else {
        return;
      };
      this.create_terminal_session(profile, window, cx);
      let profile_select = profile_select_for_event.clone();
      cx.defer_in(window, move |_, _, cx| {
        profile_select.update(cx, |state, cx| state.clear_selection(cx));
      });
    });
    subscription.detach();
  }

  /// Connects input and tab activation for one session.
  fn subscribe_session(&self, session: &ManagedTerminalSession, window: &mut Window, cx: &mut Context<ChitinApp>) {
    let id = session.id;
    let terminal = session.terminal.clone();
    let subscription: Subscription =
      cx.subscribe_in(&terminal, window, move |this, _, event, window, cx| match *event {
        TerminalEmulatorEvent::InputWritten => this.process_builtin_terminal_input(id, window, cx),
        TerminalEmulatorEvent::Exited { code } => this.terminal_session_exited(id, code, cx),
      });
    subscription.detach();

    let tab = session.tab.clone();
    let subscription: Subscription = cx.subscribe_in(&tab, window, move |this, _, event, window, cx| {
      if matches!(event, ButtonEvent::Click) {
        this.activate_terminal_session(id, window, cx);
      }
    });
    subscription.detach();
  }

  /// Returns the active terminal session.
  pub(super) fn active(&self) -> Option<&ManagedTerminalSession> {
    self.sessions.iter().find(|session| session.id == self.active_session)
  }

  /// Returns a session by stable identity.
  pub(super) fn session(&self, id: TerminalSessionId) -> Option<&ManagedTerminalSession> {
    self.sessions.iter().find(|session| session.id == id)
  }

  /// Returns a mutable session by stable identity.
  pub(super) fn session_mut(&mut self, id: TerminalSessionId) -> Option<&mut ManagedTerminalSession> {
    self.sessions.iter_mut().find(|session| session.id == id)
  }

  /// Returns the focus handle owned by the active terminal surface.
  pub(crate) fn focus(&self, cx: &gpui::App) -> Option<gpui::FocusHandle> {
    self
      .active()
      .map(|session| session.terminal.read(cx).focus_handle().clone())
  }
}

impl ChitinApp {
  /// Returns terminal controls, creating and subscribing them on first use.
  pub(crate) fn terminal_panel_controls(
    &mut self,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Option<TerminalPanelControls> {
    if let Some(controls) = self.terminal_panel_controls.as_ref() {
      return Some(controls.clone());
    }

    let working_directory = self.terminal_working_directory();
    let controls = match TerminalPanelControls::new(&working_directory, cx) {
      Ok(controls) => controls,
      Err(error) => {
        log::error!("failed to create built-in terminal: {error}");
        return None;
      }
    };
    controls.subscribe(window, cx);
    self.terminal_panel_controls = Some(controls.clone());
    Some(controls)
  }

  /// Shows or hides the bottom terminal panel.
  pub(crate) fn toggle_terminal(&mut self, cx: &mut Context<Self>) {
    let visible = self.bottom_dock.toggle(TERMINAL_DOCK_ITEM_ID);
    self.terminal_panel.request_focus(visible);
    cx.notify();
  }

  /// Creates or activates a terminal session for the selected profile.
  fn create_terminal_session(&mut self, profile: TerminalProfile, window: &mut Window, cx: &mut Context<Self>) {
    if profile == TerminalProfile::BuiltinShell
      && let Some(existing) = self
        .terminal_panel_controls
        .as_ref()
        .and_then(|controls| controls.sessions.iter().find(|session| session.profile == profile))
        .map(|session| session.id)
    {
      self.activate_terminal_session(existing, window, cx);
      return;
    }

    let working_directory = self.terminal_working_directory();
    let Some(id) = self
      .terminal_panel_controls
      .as_ref()
      .map(|controls| TerminalSessionId(controls.next_session_id))
    else {
      return;
    };
    let session = match TerminalPanelControls::build_session(id, profile, &working_directory, cx) {
      Ok(session) => session,
      Err(error) => {
        log::error!("failed to create {} terminal session: {error}", profile.label());
        self.show_toast(
          Toast::new(format!("Could not start {}", profile.label()))
            .description(error.to_string())
            .variant(ToastVariant::Error),
          cx,
        );
        return;
      }
    };
    if let Some(controls) = self.terminal_panel_controls.as_ref() {
      controls.subscribe_session(&session, window, cx);
    }
    if let Some(controls) = self.terminal_panel_controls.as_mut() {
      controls.sessions.push(session);
      controls.active_session = id;
      controls.next_session_id += 1;
      // The new tab is the last child, so revealing it keeps the active session in
      // view without the session manager having to know the strip's geometry.
      controls.tab_scroll.scroll_to_bottom();
    }
    self.terminal_panel.request_focus(true);
    cx.notify();
  }

  /// Activates one existing session and transfers keyboard focus to it.
  fn activate_terminal_session(&mut self, id: TerminalSessionId, window: &mut Window, cx: &mut Context<Self>) {
    let Some(controls) = self.terminal_panel_controls.as_mut() else {
      return;
    };
    if controls.session(id).is_none() {
      return;
    }
    controls.active_session = id;
    if let Some(focus) = controls.focus(cx) {
      window.focus(&focus, cx);
    }
    cx.notify();
  }

  /// Closes the active session and selects its nearest remaining neighbor.
  fn close_active_terminal_session(&mut self, cx: &mut Context<Self>) {
    let Some(id) = self
      .terminal_panel_controls
      .as_ref()
      .and_then(TerminalPanelControls::active)
      .map(|session| session.id)
    else {
      return;
    };
    self.close_terminal_session(id, cx);
  }

  /// Retires a session whose terminal backend stopped on its own.
  ///
  /// A program that exited cleanly has nothing left to show, so its tab goes away with
  /// it. One that failed keeps its tab, because the surface is showing the exit status
  /// and closing it would discard the last screen along with the reason for it.
  fn terminal_session_exited(&mut self, id: TerminalSessionId, code: u32, cx: &mut Context<Self>) {
    if code == 0 {
      self.close_terminal_session(id, cx);
    }
  }

  /// Closes one session and moves the selection when the active one is gone.
  fn close_terminal_session(&mut self, id: TerminalSessionId, cx: &mut Context<Self>) {
    let running = self
      .terminal_panel_controls
      .as_ref()
      .and_then(|controls| controls.session(id))
      .is_some_and(|session| session.active_shell_command.is_some());
    if running && let Err(error) = self.builtin_shell().cancel_active() {
      log::warn!("failed to cancel active built-in terminal command: {error}");
    }
    let Some(controls) = self.terminal_panel_controls.as_mut() else {
      return;
    };
    let Some(index) = controls.sessions.iter().position(|session| session.id == id) else {
      return;
    };
    let was_active = controls.active_session == id;
    controls.sessions.remove(index);
    if controls.sessions.is_empty() {
      self.terminal_panel_controls = None;
      self.bottom_dock.close();
      self.terminal_panel.request_focus(false);
      cx.notify();
      return;
    }
    if was_active {
      if let Some(session) = controls.sessions.get(index.min(controls.sessions.len() - 1)) {
        controls.active_session = session.id;
      }
      self.terminal_panel.request_focus(true);
    }
    cx.notify();
  }

  /// Resolves the directory inherited by newly created terminal sessions.
  fn terminal_working_directory(&self) -> PathBuf {
    self
      .builtin_shell()
      .snapshot()
      .map(|snapshot| snapshot.working_directory)
      .or_else(|_| std::env::current_dir())
      .unwrap_or_else(|_| PathBuf::from("."))
  }
}
