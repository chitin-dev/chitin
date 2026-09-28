//! Built-in shell routing for the VT terminal backend.

use chitin_builtin_shell::{BuiltinShell, BuiltinTerminalEvent, ShellBuiltinEffect, ShellInvocationSource};
use gpui::{AppContext, AsyncApp, Context, WeakEntity, Window};

use super::{
  completion::completion_action,
  presenter::{shell_output_lines, terminal_lines_ansi},
};
use crate::{app::ChitinApp, builtin_shell::DesktopShellDispatch, components::terminal::completion::CompletionAction};

impl ChitinApp {
  /// Drains byte-stream events produced by the in-process shell backend.
  pub(super) fn process_builtin_terminal_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(controls) = self.terminal_panel_controls.as_ref().cloned() else {
      return;
    };
    let events = match controls.program.lock() {
      Ok(mut program) => match program.poll() {
        Ok(events) => events,
        Err(error) => {
          log::error!("failed to poll built-in terminal input: {error}");
          return;
        }
      },
      Err(_) => {
        log::error!("built-in terminal program lock is poisoned");
        return;
      }
    };

    for event in events {
      self.handle_builtin_terminal_event(event, window, cx);
    }
  }

  /// Routes one semantic line-discipline event to shell or terminal behavior.
  fn handle_builtin_terminal_event(
    &mut self,
    event: BuiltinTerminalEvent,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    match event {
      BuiltinTerminalEvent::Submit(line) => self.submit_terminal_line(line, window, cx),
      BuiltinTerminalEvent::Interrupt => {
        if self
          .builtin_shell()
          .snapshot()
          .is_ok_and(|snapshot| snapshot.active.is_some())
        {
          let _ = self.builtin_shell().cancel_active();
        }
      }
      BuiltinTerminalEvent::Complete(line) => self.complete_terminal_line(line),
      BuiltinTerminalEvent::PreviousHistory => {
        let draft = self
          .terminal_panel_controls
          .as_ref()
          .and_then(|controls| controls.program.lock().ok().map(|program| program.line().to_owned()))
          .unwrap_or_default();
        if let Ok(Some(line)) = self.builtin_shell().previous_history(&draft) {
          self.replace_terminal_line(line);
        }
      }
      BuiltinTerminalEvent::NextHistory => {
        if let Ok(Some(line)) = self.builtin_shell().next_history() {
          self.replace_terminal_line(line);
        }
      }
      BuiltinTerminalEvent::Resize(_) | BuiltinTerminalEvent::Shutdown => {}
    }
    cx.notify();
  }

  /// Submits one line through the existing typed built-in-shell dispatcher.
  fn submit_terminal_line(&mut self, line: String, window: &mut Window, cx: &mut Context<Self>) {
    let line = line.trim().to_owned();
    if line.is_empty() {
      self.finish_terminal_command("");
      return;
    }
    match self.submit_builtin_shell_line(line, ShellInvocationSource::Interactive, window, cx) {
      Ok(DesktopShellDispatch::Display { output }) => self.finish_terminal_command(&output),
      Ok(DesktopShellDispatch::ShellBuiltin {
        effect: ShellBuiltinEffect::ClearScrollback,
      }) => self.clear_terminal_screen(),
      Ok(DesktopShellDispatch::Frontend { .. }) => self.finish_terminal_command(""),
      Ok(DesktopShellDispatch::Portable(task)) => {
        let command_id = task.command_id();
        self.terminal_panel.start_command(command_id);
        let window_handle = window.window_handle();
        cx.spawn(async move |app: WeakEntity<ChitinApp>, async_cx: &mut AsyncApp| {
          let result = task.result().await;
          let _ = async_cx.update_window(window_handle, |_, window, cx| {
            let _ = app.update(cx, |this, cx| {
              let output = match result {
                Ok(result) => {
                  let mut lines = this
                    .builtin_shell()
                    .snapshot()
                    .map(|snapshot| shell_output_lines(&snapshot, command_id))
                    .unwrap_or_default();
                  let (_, result_lines) = super::presenter::terminal_outcome(result.outcome);
                  lines.extend(result_lines);
                  terminal_lines_ansi(&lines)
                }
                Err(error) => match this.builtin_shell().snapshot() {
                  Ok(snapshot) => {
                    let output = terminal_lines_ansi(&shell_output_lines(&snapshot, command_id));
                    if output.is_empty() {
                      format!("\x1b[31merror: {error}\x1b[0m")
                    } else {
                      output
                    }
                  }
                  Err(_) => format!("\x1b[31merror: {error}\x1b[0m"),
                },
              };
              this.finish_terminal_command(&output);
              if let Some(controls) = this.terminal_panel_controls.as_ref() {
                window.focus(&controls.focus(cx), cx);
              }
              cx.notify();
            });
          });
        })
        .detach();
      }
      Err(error) => self.finish_terminal_command(&format!("\x1b[31merror: {error}\x1b[0m")),
    }
  }

  /// Applies built-in shell completion to the terminal-owned editable line.
  fn complete_terminal_line(&self, line: String) {
    match completion_action(BuiltinShell::complete(&line)) {
      CompletionAction::Insert(completed) => self.replace_terminal_line(completed),
      CompletionAction::List(candidates) => {
        let output = candidates.join("\n");
        if let Some(controls) = self.terminal_panel_controls.as_ref()
          && let Ok(program) = controls.program.lock()
          && let Err(error) = program.show_notice(&output)
        {
          log::error!("failed to display terminal completions: {error}");
        }
      }
      CompletionAction::None => {}
    }
  }

  /// Replaces the line edited by the in-process terminal backend.
  fn replace_terminal_line(&self, line: String) {
    if let Some(controls) = self.terminal_panel_controls.as_ref()
      && let Ok(mut program) = controls.program.lock()
      && let Err(error) = program.replace_line(line)
    {
      log::error!("failed to replace built-in terminal line: {error}");
    }
  }

  /// Writes final command output and restores the shell prompt.
  fn finish_terminal_command(&mut self, output: &str) {
    if let Some(controls) = self.terminal_panel_controls.as_ref()
      && let Ok(mut program) = controls.program.lock()
      && let Err(error) = program.finish_command(output)
    {
      log::error!("failed to finish built-in terminal command: {error}");
    }
    self.terminal_panel.finish_command();
  }

  /// Clears the VT grid and resets the built-in line discipline.
  fn clear_terminal_screen(&mut self) {
    if let Some(controls) = self.terminal_panel_controls.as_ref()
      && let Ok(mut program) = controls.program.lock()
      && let Err(error) = program.clear_screen()
    {
      log::error!("failed to clear built-in terminal: {error}");
    }
    self.terminal_panel.finish_command();
  }

  /// Projects the active command's replaceable transcript into the VT grid.
  pub(crate) fn sync_terminal_output(&mut self) {
    let Some(command_id) = self.terminal_panel.active_shell_command else {
      return;
    };
    let Ok(snapshot) = self.builtin_shell().snapshot() else {
      return;
    };
    let output = terminal_lines_ansi(&shell_output_lines(&snapshot, command_id));
    if output == self.terminal_panel.rendered_running_output {
      return;
    }
    let Some(controls) = self.terminal_panel_controls.as_ref() else {
      return;
    };
    let Ok(mut program) = controls.program.lock() else {
      log::error!("built-in terminal program lock is poisoned");
      return;
    };
    if let Err(error) = program.update_command_output(&output) {
      log::error!("failed to update built-in terminal output: {error}");
      return;
    }
    self.terminal_panel.rendered_running_output = output;
  }
}
