//! Desktop adapter for the reusable structured command terminal.

mod controller;
mod presenter;
mod render;

use std::path::Path;

use chitin_builtin_shell::ShellCommandId;
use chitin_ui::{
  composite::command_terminal::{CommandTerminalId, CommandTerminalState},
  primitive::input::text::{TextInputEvent, TextInputState},
};
use gpui::{AppContext, Context, Entity, Subscription, Window};

use crate::app::ChitinApp;

pub(crate) use render::render_terminal_bottom_dock;

pub(crate) const TERMINAL_DOCK_ITEM_ID: &str = "terminal";
pub(super) const TERMINAL_FONT_FAMILY: &str = "Cascadia Code";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TerminalShellBinding {
  pub(super) shell_id: ShellCommandId,
  pub(super) terminal_id: CommandTerminalId,
}

/// Terminal focus and shell-to-view identity mapping.
pub(crate) struct TerminalPanelState {
  focus_requested: bool,
  pub(super) bindings: Vec<TerminalShellBinding>,
}

impl TerminalPanelState {
  /// Creates a closed terminal panel.
  pub(crate) fn new() -> Self {
    Self {
      focus_requested: false,
      bindings: Vec::new(),
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

  /// Associates one shell execution with its visible command block.
  pub(super) fn bind(&mut self, shell_id: ShellCommandId, terminal_id: CommandTerminalId) {
    self.bindings.push(TerminalShellBinding { shell_id, terminal_id });
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
  pub(super) terminal: Entity<CommandTerminalState>,
}

impl TerminalPanelControls {
  /// Creates terminal state and panel controls for one working directory.
  fn new(working_directory: &Path, cx: &mut Context<ChitinApp>) -> Self {
    let prompt = presenter::terminal_prompt(working_directory);
    Self {
      terminal: cx.new(|cx| CommandTerminalState::new(prompt, cx)),
    }
  }

  /// Connects primitive semantic events to the desktop shell adapter.
  fn subscribe(&self, window: &mut Window, cx: &mut Context<ChitinApp>) {
    let input = self.input(cx);
    let input_subscription: Subscription = cx.subscribe_in(&input, window, |this, _, event, window, cx| match event {
      TextInputEvent::Submit { value } => this.submit_terminal_line(value.to_string(), window, cx),
      TextInputEvent::Cancel => this.close_terminal(cx),
      _ => {}
    });
    input_subscription.detach();
  }

  /// Returns the live prompt's editable input.
  pub(crate) fn input(&self, cx: &gpui::App) -> Entity<TextInputState> {
    self.terminal.read(cx).input().clone()
  }
}

impl ChitinApp {
  /// Returns terminal controls, creating and subscribing them on first use.
  pub(crate) fn terminal_panel_controls(
    &mut self,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> TerminalPanelControls {
    if let Some(controls) = self.terminal_panel_controls.as_ref() {
      return controls.clone();
    }

    let working_directory = self
      .builtin_shell()
      .snapshot()
      .map(|snapshot| snapshot.working_directory)
      .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let controls = TerminalPanelControls::new(&working_directory, cx);
    controls.subscribe(window, cx);
    self.terminal_panel_controls = Some(controls.clone());
    controls
  }

  /// Shows or hides the bottom terminal panel.
  pub(crate) fn toggle_terminal(&mut self, cx: &mut Context<Self>) {
    let visible = self.bottom_dock.toggle(TERMINAL_DOCK_ITEM_ID);
    self.terminal_panel.request_focus(visible);
    cx.notify();
  }

  /// Hides the bottom terminal panel.
  fn close_terminal(&mut self, cx: &mut Context<Self>) {
    self.close_bottom_dock(cx);
  }
}
