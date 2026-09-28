//! Desktop adapter for the reusable structured command terminal.

mod completion;
mod controller;
mod presenter;
mod render;

use std::{
  path::Path,
  sync::{Arc, Mutex},
};

use chitin_builtin_shell::{BuiltinTerminalProgram, ShellCommandId};
use chitin_terminal::{TerminalSessionError, TerminalSize};
use chitin_ui::primitive::terminal::{TerminalEmulatorEvent, TerminalEmulatorState};
use gpui::{AppContext, Context, Entity, Subscription, Window};

use crate::app::ChitinApp;

pub(crate) use render::render_terminal_bottom_dock;

pub(crate) const TERMINAL_DOCK_ITEM_ID: &str = "terminal";

/// Terminal focus and active built-in-shell command state.
pub(crate) struct TerminalPanelState {
  focus_requested: bool,
  pub(super) active_shell_command: Option<ShellCommandId>,
  pub(super) rendered_running_output: String,
}

impl TerminalPanelState {
  /// Creates a closed terminal panel.
  pub(crate) fn new() -> Self {
    Self {
      focus_requested: false,
      active_shell_command: None,
      rendered_running_output: String::new(),
    }
  }

  /// Updates whether terminal input should receive focus on the next render.
  pub(crate) fn request_focus(&mut self, requested: bool) {
    self.focus_requested = requested;
  }

  /// Takes the pending input-focus request.
  pub(crate) fn take_focus_request(&mut self) -> bool {
    std::mem::take(&mut self.focus_requested)
  }

  /// Tracks the portable shell command currently writing terminal output.
  pub(super) fn start_command(&mut self, command_id: ShellCommandId) {
    self.active_shell_command = Some(command_id);
    self.rendered_running_output.clear();
  }

  /// Clears the active output projection after its command reaches a terminal state.
  pub(super) fn finish_command(&mut self) {
    self.active_shell_command = None;
    self.rendered_running_output.clear();
  }
}

impl Default for TerminalPanelState {
  fn default() -> Self {
    Self::new()
  }
}

/// Persistent primitive and composite controls for the bottom terminal panel.
#[derive(Clone)]
pub(crate) struct TerminalPanelControls {
  pub(super) terminal: Entity<TerminalEmulatorState>,
  pub(super) program: Arc<Mutex<BuiltinTerminalProgram>>,
}

impl TerminalPanelControls {
  /// Creates terminal state and panel controls for one working directory.
  fn new(working_directory: &Path, cx: &mut Context<ChitinApp>) -> Result<Self, TerminalSessionError> {
    let prompt = presenter::terminal_prompt_ansi(working_directory);
    let size = TerminalSize::new(80, 24, 9, 21);
    let (session, program) = BuiltinTerminalProgram::connect(size, prompt)?;
    Ok(Self {
      terminal: cx.new(|cx| TerminalEmulatorState::new(session, size, cx)),
      program: Arc::new(Mutex::new(program)),
    })
  }

  /// Connects primitive semantic events to the desktop shell adapter.
  fn subscribe(&self, window: &mut Window, cx: &mut Context<ChitinApp>) {
    let terminal = self.terminal.clone();
    let subscription: Subscription = cx.subscribe_in(&terminal, window, |this, _, event, window, cx| {
      if *event == TerminalEmulatorEvent::InputWritten {
        this.process_builtin_terminal_input(window, cx);
      }
    });
    subscription.detach();
  }

  /// Returns the focus handle owned by the VT terminal surface.
  pub(crate) fn focus(&self, cx: &gpui::App) -> gpui::FocusHandle {
    self.terminal.read(cx).focus_handle().clone()
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

    let working_directory = self
      .builtin_shell()
      .snapshot()
      .map(|snapshot| snapshot.working_directory)
      .unwrap_or_else(|_| std::path::PathBuf::from("."));
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
}
