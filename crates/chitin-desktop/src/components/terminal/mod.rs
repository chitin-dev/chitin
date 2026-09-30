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
use chitin_command::CommandExecutionContext;
use chitin_terminal::{ShellCatalog, ShellDefinition, TerminalSession, TerminalSessionError, TerminalSize};
use chitin_ui::{
  composite::select_item::IconSelectItem,
  composite::toast::{Toast, ToastVariant},
  primitive::terminal::{TerminalEmulatorEvent, TerminalEmulatorState},
};
use gpui::{AppContext, Context, Entity, ScrollHandle, Subscription, Window};
use gpui_kit::component::select::{SelectEvent, SelectState};

use crate::{
  app::ChitinApp,
  builtin_shell::{DesktopShellHost, desktop_shell_context},
};

pub(crate) use render::render_terminal_bottom_dock;

pub(crate) const TERMINAL_DOCK_ITEM_ID: &str = "terminal";
const INITIAL_TERMINAL_SIZE: TerminalSize = TerminalSize::new(80, 24, 9, 21);

/// Returns the desktop asset used to represent one terminal profile.
pub(super) fn terminal_profile_icon(id: &str) -> &'static str {
  match id {
    "builtin-shell" => "icons/terminal-builtin.svg",
    "bash" | "git-bash" => "icons/terminal-bash.svg",
    "fish" => "icons/terminal-fish.svg",
    "powershell" | "pwsh" => "icons/terminal-powershell.svg",
    "zsh" => "icons/terminal-zsh.svg",
    "nushell" => "icons/terminal-nushell.svg",
    _ => "icons/terminal-shell.svg",
  }
}

/// One launchable desktop choice; only PTY shells come from platform discovery.
#[derive(Clone)]
enum DesktopTerminalProfile {
  Builtin,
  Shell(ShellDefinition),
}

impl DesktopTerminalProfile {
  fn id(&self) -> &str {
    match self {
      Self::Builtin => "builtin-shell",
      Self::Shell(shell) => &shell.id,
    }
  }

  fn label(&self) -> &str {
    match self {
      Self::Builtin => "Built-in shell",
      Self::Shell(shell) => &shell.label,
    }
  }
}

/// Stable identity of one terminal session in the bottom dock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TerminalSessionId(u64);

/// Terminal focus state that belongs to the bottom dock rather than a session.
pub(crate) struct TerminalPanelState {
  focus_requested: bool,
  next_session_id: u64,
}

impl TerminalPanelState {
  /// Creates a closed terminal panel.
  pub(crate) fn new() -> Self {
    Self {
      focus_requested: false,
      next_session_id: 1,
    }
  }

  /// Updates whether the active session should receive focus on the next render.
  pub(crate) fn request_focus(&mut self, requested: bool) {
    self.focus_requested = requested;
  }

  /// Takes the pending input-focus request.
  pub(crate) fn take_focus_request(&mut self) -> bool {
    std::mem::take(&mut self.focus_requested)
  }

  /// Allocates an identity that is not reused when the dock is closed and reopened.
  fn next_session_id(&mut self) -> TerminalSessionId {
    let id = TerminalSessionId(self.next_session_id);
    self.next_session_id += 1;
    id
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
  profile: DesktopTerminalProfile,
  pub(super) terminal: Entity<TerminalEmulatorState>,
  pub(super) builtin: Option<ManagedBuiltinShell>,
  pub(super) active_shell_command: Option<ShellCommandId>,
  pub(super) rendered_running_output: String,
}

/// Command state and byte-stream program owned by one built-in terminal tab.
#[derive(Clone)]
pub(super) struct ManagedBuiltinShell {
  pub(super) host: DesktopShellHost,
  pub(super) program: Arc<Mutex<BuiltinTerminalProgram>>,
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
  catalog: ShellCatalog,
  pub(super) sessions: Vec<ManagedTerminalSession>,
  pub(super) active_session: TerminalSessionId,
  pub(super) profile_select: Entity<SelectState<Vec<IconSelectItem>>>,
  /// Scroll position of the session tab strip.
  ///
  /// This has to outlive a render: a handle created while rendering would start every
  /// frame at the top of the strip and the lane would never move.
  pub(super) tab_scroll: ScrollHandle,
}

impl TerminalPanelControls {
  /// Creates the session manager with an initial built-in shell.
  fn new(
    initial_id: TerminalSessionId,
    working_directory: &Path,
    shell_context: CommandExecutionContext,
    window: &mut Window,
    cx: &mut Context<ChitinApp>,
  ) -> Result<Self, TerminalSessionError> {
    let catalog = ShellCatalog::discover();
    let profile_select = cx.new(|cx| {
      SelectState::new(
        std::iter::once(
          IconSelectItem::new("builtin-shell", "Built-in shell").icon(terminal_profile_icon("builtin-shell")),
        )
        .chain(
          catalog
            .available()
            .iter()
            .map(|shell| IconSelectItem::new(&shell.id, &shell.label).icon(terminal_profile_icon(&shell.id))),
        )
        .collect::<Vec<_>>(),
        None,
        window,
        cx,
      )
    });
    let session = Self::build_session(
      initial_id,
      &DesktopTerminalProfile::Builtin,
      working_directory,
      shell_context,
      cx,
    )?;
    Ok(Self {
      catalog,
      sessions: vec![session],
      active_session: initial_id,
      profile_select,
      tab_scroll: ScrollHandle::new(),
    })
  }

  /// Builds a terminal surface for one launch profile.
  fn build_session(
    id: TerminalSessionId,
    profile: &DesktopTerminalProfile,
    working_directory: &Path,
    mut shell_context: CommandExecutionContext,
    cx: &mut Context<ChitinApp>,
  ) -> Result<ManagedTerminalSession, TerminalSessionError> {
    let (session, builtin) = match profile {
      DesktopTerminalProfile::Builtin => {
        // The prompt and relative-path execution must start in the same directory.
        shell_context.working_directory = working_directory.to_path_buf();
        let prompt = presenter::terminal_prompt_ansi(working_directory);
        let (session, program) = BuiltinTerminalProgram::connect(INITIAL_TERMINAL_SIZE, prompt)?;
        (
          session,
          Some(ManagedBuiltinShell {
            host: DesktopShellHost::new(shell_context),
            program: Arc::new(Mutex::new(program)),
          }),
        )
      }
      DesktopTerminalProfile::Shell(shell) => (
        TerminalSession::spawn_shell(shell, working_directory, INITIAL_TERMINAL_SIZE)?,
        None,
      ),
    };
    Ok(ManagedTerminalSession {
      id,
      profile: profile.clone(),
      terminal: cx.new(|cx| TerminalEmulatorState::new(session, INITIAL_TERMINAL_SIZE, cx)),
      builtin,
      active_shell_command: None,
      rendered_running_output: String::new(),
    })
  }

  /// Connects session-manager and initial-session semantic controls.
  fn subscribe(&self, window: &mut Window, cx: &mut Context<ChitinApp>) {
    self.subscribe_session(&self.sessions[0], window, cx);

    let profile_select = self.profile_select.clone();
    let profile_select_for_event = profile_select.clone();
    let subscription: Subscription = cx.subscribe_in(&profile_select, window, move |this, _, event, window, cx| {
      let SelectEvent::Confirm(Some(selected_id)) = event else {
        return;
      };
      let Some(profile) = this.terminal_panel_controls.as_ref().and_then(|controls| {
        if selected_id.as_ref() == "builtin-shell" {
          Some(DesktopTerminalProfile::Builtin)
        } else {
          controls
            .catalog
            .get(selected_id.as_ref())
            .cloned()
            .map(DesktopTerminalProfile::Shell)
        }
      }) else {
        return;
      };
      this.create_terminal_session(profile, window, cx);
      let profile_select = profile_select_for_event.clone();
      cx.defer_in(window, move |_, window, cx| {
        profile_select.update(cx, |state, cx| state.set_selected_index(None, window, cx));
      });
    });
    subscription.detach();
  }

  /// Connects terminal input events for one session.
  ///
  /// Tab activation is not subscribed here: the session tab button owns its own
  /// click handler and reaches the session manager through the app entity.
  fn subscribe_session(&self, session: &ManagedTerminalSession, window: &mut Window, cx: &mut Context<ChitinApp>) {
    let id = session.id;
    let terminal = session.terminal.clone();
    let subscription: Subscription =
      cx.subscribe_in(&terminal, window, move |this, _, event, window, cx| match *event {
        TerminalEmulatorEvent::InputWritten => this.process_builtin_terminal_input(id, window, cx),
        TerminalEmulatorEvent::Exited { code } => this.terminal_session_exited(id, code, cx),
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
  /// Returns the command host owned by one built-in terminal session.
  fn terminal_shell_host(&self, id: TerminalSessionId) -> Option<DesktopShellHost> {
    self
      .terminal_panel_controls
      .as_ref()?
      .session(id)?
      .builtin
      .as_ref()
      .map(|builtin| builtin.host.clone())
  }

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
    let shell_context = desktop_shell_context(self.workspace.as_ref().map(|workspace| workspace.root.clone()));
    let initial_id = self.terminal_panel.next_session_id();
    let controls = match TerminalPanelControls::new(initial_id, &working_directory, shell_context, window, cx) {
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

  /// Creates a new terminal session for the selected profile.
  fn create_terminal_session(&mut self, profile: DesktopTerminalProfile, window: &mut Window, cx: &mut Context<Self>) {
    let working_directory = self.terminal_working_directory();
    let shell_context = desktop_shell_context(self.workspace.as_ref().map(|workspace| workspace.root.clone()));
    if self.terminal_panel_controls.is_none() {
      return;
    }
    let id = self.terminal_panel.next_session_id();
    let session = match TerminalPanelControls::build_session(id, &profile, &working_directory, shell_context, cx) {
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
    let builtin = self
      .terminal_panel_controls
      .as_ref()
      .and_then(|controls| controls.session(id))
      .and_then(|session| session.builtin.clone());
    if let Some(builtin) = builtin
      && let Err(error) = builtin.host.session().cancel_active()
    {
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
      .terminal_panel_controls
      .as_ref()
      .and_then(TerminalPanelControls::active)
      .and_then(|session| session.builtin.as_ref())
      .map(|builtin| builtin.host.session())
      .unwrap_or_else(|| self.builtin_shell())
      .snapshot()
      .map(|snapshot| snapshot.working_directory)
      .or_else(|_| std::env::current_dir())
      .unwrap_or_else(|_| PathBuf::from("."))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn terminal_session_ids_should_not_repeat_after_a_dock_reopen() {
    let mut state = TerminalPanelState::new();
    let first = state.next_session_id();
    let second = state.next_session_id();

    assert_ne!(first, second);
  }

  #[test]
  fn terminal_shell_icons_should_resolve_to_existing_assets() {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let all_exist = ["powershell", "pwsh", "zsh", "nushell"]
      .into_iter()
      .map(terminal_profile_icon)
      .all(|icon| assets.join(icon).is_file());

    assert!(all_exist);
  }
}
