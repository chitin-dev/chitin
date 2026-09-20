//! Keyboard and command-execution control for the desktop terminal.

use chitin_builtin_shell::ShellInvocationSource;
use chitin_ui::composite::command_terminal::CommandTerminalStatus;
use gpui::{AppContext, AsyncApp, Context, KeyDownEvent, WeakEntity, Window};

use super::presenter::{shell_output_lines, shell_terminal_status, terminal_outcome};
use crate::{app::ChitinApp, builtin_shell::DesktopShellDispatch};

impl ChitinApp {
  /// Submits one live-prompt line and binds all resulting output to its block.
  ///
  /// # Parameters
  ///
  /// * `line` is the complete command line emitted by the text input.
  /// * `window` supplies the frontend context required by UI commands.
  /// * `cx` creates the immutable command block and schedules portable work.
  pub(super) fn submit_terminal_line(&mut self, line: String, window: &mut Window, cx: &mut Context<Self>) {
    let line = line.trim().to_owned();
    if line.is_empty() {
      return;
    }
    let Some(controls) = self.terminal_panel_controls.as_ref().cloned() else {
      return;
    };
    if controls.terminal.read(cx).is_busy() {
      return;
    }
    let terminal_id = controls
      .terminal
      .update(cx, |terminal, cx| terminal.begin_submission(line.clone(), cx));

    match self.submit_builtin_shell_line(line, ShellInvocationSource::Interactive, window, cx) {
      Ok(DesktopShellDispatch::Frontend { command_id }) => {
        self.terminal_panel.bind(command_id, terminal_id);
        controls.terminal.update(cx, |terminal, cx| {
          terminal.finish(terminal_id, CommandTerminalStatus::Succeeded, Vec::new(), cx);
        });
      }
      Ok(DesktopShellDispatch::Portable(task)) => {
        self.terminal_panel.bind(task.command_id(), terminal_id);
        let viewport = controls.terminal.read(cx).viewport().clone();
        let focus = viewport.read(cx).focus_handle().clone();
        window.focus(&focus, cx);
        let window_handle = window.window_handle();
        cx.spawn(async move |app: WeakEntity<ChitinApp>, async_cx: &mut AsyncApp| {
          let result = task.result().await;
          let _ = async_cx.update_window(window_handle, |_, window, cx| {
            let _ = app.update(cx, |this, cx| {
              this.sync_terminal_output(cx);
              if let Some(controls) = this.terminal_panel_controls.as_ref() {
                let (status, lines) = match result {
                  Ok(result) => terminal_outcome(result.outcome),
                  Err(_) => {
                    let status = this
                      .builtin_shell()
                      .snapshot()
                      .ok()
                      .and_then(|snapshot| shell_terminal_status(&snapshot, terminal_id, &this.terminal_panel))
                      .unwrap_or(CommandTerminalStatus::Failed);
                    (status, Vec::new())
                  }
                };
                controls.terminal.update(cx, |terminal, cx| {
                  terminal.finish(terminal_id, status, lines, cx);
                });
                if this.terminal_panel.is_visible() {
                  let input = controls.input(cx);
                  let focus = input.read(cx).focus_handle().clone();
                  window.focus(&focus, cx);
                }
              }
              cx.notify();
            });
          });
        })
        .detach();
      }
      Err(error) => {
        controls
          .terminal
          .update(cx, |terminal, cx| terminal.reject(terminal_id, error.to_string(), cx));
      }
    }
    cx.notify();
  }

  /// Synchronizes running command events into their existing terminal blocks.
  pub(crate) fn sync_terminal_output(&mut self, cx: &mut Context<Self>) {
    let Some(controls) = self.terminal_panel_controls.as_ref() else {
      return;
    };
    let Ok(snapshot) = self.builtin_shell().snapshot() else {
      return;
    };

    for binding in &self.terminal_panel.bindings {
      let output = shell_output_lines(&snapshot, binding.shell_id);
      controls.terminal.update(cx, |terminal, cx| {
        terminal.set_output(binding.terminal_id, output, cx);
      });
    }
  }

  /// Applies history navigation and cancellation while the terminal owns focus.
  ///
  /// # Parameters
  ///
  /// * `event` is the focused GPUI keyboard event.
  /// * `window` verifies ownership of keyboard focus.
  /// * `cx` updates input/history state and stops consumed events.
  pub(crate) fn handle_terminal_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
    let Some(controls) = self.terminal_panel_controls.as_ref() else {
      return;
    };
    let input = controls.input(cx);
    let key = event.keystroke.key.as_str();
    if event.keystroke.modifiers.control && key == "c" {
      let viewport = controls.terminal.read(cx).viewport().clone();
      let terminal_focused =
        input.read(cx).focus_handle().is_focused(window) || viewport.read(cx).focus_handle().is_focused(window);
      if terminal_focused
        && self
          .builtin_shell()
          .snapshot()
          .is_ok_and(|snapshot| snapshot.active.is_some())
      {
        let _ = self.builtin_shell().cancel_active();
        cx.stop_propagation();
        cx.notify();
      }
      return;
    }

    if !input.read(cx).focus_handle().is_focused(window) {
      return;
    }

    let history = match key {
      "up" => self.builtin_shell().previous_history(input.read(cx).text()),
      "down" => self.builtin_shell().next_history(),
      _ => return,
    };
    if let Ok(Some(line)) = history {
      input.update(cx, |input, cx| {
        input.set_text(line, cx);
      });
    }
    cx.stop_propagation();
    cx.notify();
  }
}
