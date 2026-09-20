//! Desktop adapter for the reusable structured command terminal.

mod controller;
mod presenter;
mod render;

use std::path::Path;

use chitin_builtin_shell::ShellCommandId;
use chitin_ui::{
  composite::command_terminal::{CommandTerminalId, CommandTerminalState},
  primitive::{
    button::{ButtonEvent, ButtonState},
    input::text::{TextInputEvent, TextInputState},
    resize::ResizeGesture,
  },
};
use gpui::{AppContext, Context, Entity, Pixels, Subscription, Window, px};

use crate::app::ChitinApp;

pub(crate) use render::render_terminal_panel;

const DEFAULT_TERMINAL_HEIGHT: Pixels = px(260.0);
const MIN_TERMINAL_HEIGHT: Pixels = px(120.0);
const MIN_DOCUMENT_AREA_HEIGHT: Pixels = px(160.0);
pub(super) const TERMINAL_FONT_FAMILY: &str = "Cascadia Code";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TerminalShellBinding {
  pub(super) shell_id: ShellCommandId,
  pub(super) terminal_id: CommandTerminalId,
}

/// Visibility, focus, and shell-to-view identity mapping for the bottom panel.
pub(crate) struct TerminalPanelState {
  visible: bool,
  focus_requested: bool,
  height: Pixels,
  resize_drag: Option<ResizeGesture<(), Pixels>>,
  pub(super) bindings: Vec<TerminalShellBinding>,
}

impl TerminalPanelState {
  /// Creates a closed terminal panel.
  pub(crate) fn new() -> Self {
    Self {
      visible: false,
      focus_requested: false,
      height: DEFAULT_TERMINAL_HEIGHT,
      resize_drag: None,
      bindings: Vec::new(),
    }
  }

  /// Returns whether the terminal panel is visible.
  pub(crate) const fn is_visible(&self) -> bool {
    self.visible
  }

  /// Toggles panel visibility and requests input focus when opening.
  pub(crate) fn toggle(&mut self) {
    self.visible = !self.visible;
    self.focus_requested = self.visible;
    if !self.visible {
      self.resize_drag = None;
    }
  }

  /// Hides the terminal panel.
  pub(crate) fn close(&mut self) {
    self.visible = false;
    self.focus_requested = false;
  }

  /// Takes the pending input-focus request.
  pub(crate) fn take_focus_request(&mut self) -> bool {
    std::mem::take(&mut self.focus_requested)
  }

  /// Returns the current terminal panel height.
  pub(crate) const fn height(&self) -> Pixels {
    self.height
  }

  /// Starts resizing from the terminal's top edge.
  pub(crate) fn start_resize(&mut self, start_y: Pixels) {
    self.resize_drag = Some(ResizeGesture::new((), start_y, self.height));
  }

  /// Updates terminal height while preserving minimum document space.
  ///
  /// # Parameters
  ///
  /// * `current_y` is the latest vertical pointer position.
  /// * `available_height` is the workbench height below the window bar.
  ///
  /// # Returns
  ///
  /// `true` when an active drag changed or reaffirmed the terminal height.
  pub(crate) fn drag_resize(&mut self, current_y: Pixels, available_height: Pixels) -> bool {
    let Some(resize_drag) = self.resize_drag else {
      return false;
    };
    let maximum_height =
      px((f32::from(available_height) - f32::from(MIN_DOCUMENT_AREA_HEIGHT)).max(f32::from(MIN_TERMINAL_HEIGHT)));
    let desired_height = f32::from(resize_drag.start_value()) - resize_drag.delta(current_y);
    self.height = px(desired_height.clamp(f32::from(MIN_TERMINAL_HEIGHT), f32::from(maximum_height)));
    true
  }

  /// Stops the active terminal resize gesture.
  pub(crate) fn stop_resize(&mut self) -> bool {
    self.resize_drag.take().is_some()
  }

  /// Returns whether the terminal top edge is being dragged.
  pub(crate) const fn is_resizing(&self) -> bool {
    self.resize_drag.is_some()
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
  pub(super) close: Entity<ButtonState>,
}

impl TerminalPanelControls {
  /// Creates terminal state and panel controls for one working directory.
  fn new(working_directory: &Path, cx: &mut Context<ChitinApp>) -> Self {
    let prompt = presenter::terminal_prompt(working_directory);
    Self {
      terminal: cx.new(|cx| CommandTerminalState::new(prompt, cx)),
      close: cx.new(ButtonState::new),
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

    let close_subscription: Subscription = cx.subscribe_in(&self.close, window, |this, _, event, _, cx| {
      if matches!(event, ButtonEvent::Click) {
        this.close_terminal(cx);
      }
    });
    close_subscription.detach();
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
    self.terminal_panel.toggle();
    cx.notify();
  }

  /// Hides the bottom terminal panel.
  fn close_terminal(&mut self, cx: &mut Context<Self>) {
    self.terminal_panel.stop_resize();
    self.terminal_panel.close();
    cx.notify();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Verifies that dragging the top edge upward increases terminal height.
  #[test]
  fn drag_resize_should_increase_height_when_pointer_moves_up() {
    let mut state = TerminalPanelState::new();
    state.start_resize(px(400.0));

    assert!(state.drag_resize(px(350.0), px(800.0)));
    assert_eq!(state.height(), px(310.0));
  }

  /// Verifies that terminal resizing reserves space for the document area.
  #[test]
  fn drag_resize_should_preserve_minimum_document_height() {
    let mut state = TerminalPanelState::new();
    state.start_resize(px(400.0));

    assert!(state.drag_resize(px(0.0), px(600.0)));
    assert_eq!(state.height(), px(440.0));
  }

  /// Verifies that dragging downward cannot collapse the terminal completely.
  #[test]
  fn drag_resize_should_preserve_minimum_terminal_height() {
    let mut state = TerminalPanelState::new();
    state.start_resize(px(400.0));

    assert!(state.drag_resize(px(800.0), px(600.0)));
    assert_eq!(state.height(), MIN_TERMINAL_HEIGHT);
  }
}
