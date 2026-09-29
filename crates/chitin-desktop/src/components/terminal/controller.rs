//! Built-in shell routing for the VT terminal backend.

use chitin_builtin_shell::{BuiltinShell, BuiltinTerminalEvent, ShellBuiltinEffect, ShellInvocationSource};
use gpui::{AppContext, AsyncApp, Context, WeakEntity, Window};

use super::{
  TerminalSessionId,
  completion::completion_action,
  presenter::{shell_output_lines, terminal_lines_ansi},
};
use crate::{app::ChitinApp, builtin_shell::DesktopShellDispatch, components::terminal::completion::CompletionAction};

impl ChitinApp {
  /// Drains byte-stream events produced by the in-process shell backend.
  pub(super) fn process_builtin_terminal_input(
    &mut self,
    session_id: TerminalSessionId,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let Some(program) = self
      .terminal_panel_controls
      .as_ref()
      .and_then(|controls| controls.session(session_id))
      .and_then(|session| session.builtin.as_ref())
      .map(|builtin| builtin.program.clone())
    else {
      return;
    };
    let events = match program.lock() {
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
      self.handle_builtin_terminal_event(session_id, event, window, cx);
    }
  }

  /// Routes one semantic line-discipline event to shell or terminal behavior.
  fn handle_builtin_terminal_event(
    &mut self,
    session_id: TerminalSessionId,
    event: BuiltinTerminalEvent,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let Some(host) = self.terminal_shell_host(session_id) else {
      return;
    };
    match event {
      BuiltinTerminalEvent::Submit(line) => self.submit_terminal_line(session_id, line, window, cx),
      BuiltinTerminalEvent::Interrupt => {
        if host
          .session()
          .snapshot()
          .is_ok_and(|snapshot| snapshot.active.is_some())
        {
          let _ = host.session().cancel_active();
        }
      }
      BuiltinTerminalEvent::Complete(line) => self.complete_terminal_line(session_id, line),
      BuiltinTerminalEvent::PreviousHistory => {
        let draft = self
          .terminal_panel_controls
          .as_ref()
          .and_then(|controls| controls.session(session_id))
          .and_then(|session| session.builtin.as_ref())
          .map(|builtin| &builtin.program)
          .and_then(|program| program.lock().ok().map(|program| program.line().to_owned()))
          .unwrap_or_default();
        if let Ok(Some(line)) = host.session().previous_history(&draft) {
          self.replace_terminal_line(session_id, line);
        }
      }
      BuiltinTerminalEvent::NextHistory => {
        if let Ok(Some(line)) = host.session().next_history() {
          self.replace_terminal_line(session_id, line);
        }
      }
      BuiltinTerminalEvent::Resize(_) | BuiltinTerminalEvent::Shutdown => {}
    }
    cx.notify();
  }

  /// Submits one line through the existing typed built-in-shell dispatcher.
  fn submit_terminal_line(
    &mut self,
    session_id: TerminalSessionId,
    line: String,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let Some(host) = self.terminal_shell_host(session_id) else {
      return;
    };
    let line = line.trim().to_owned();
    if line.is_empty() {
      self.finish_terminal_command(session_id, "");
      return;
    }
    match self.submit_shell_line_with_host(&host, line, ShellInvocationSource::Interactive, window, cx) {
      Ok(DesktopShellDispatch::Display { output }) => self.finish_terminal_command(session_id, &output),
      Ok(DesktopShellDispatch::ShellBuiltin {
        effect: ShellBuiltinEffect::ClearScrollback,
      }) => self.clear_terminal_screen(session_id),
      Ok(DesktopShellDispatch::Frontend { .. }) => self.finish_terminal_command(session_id, ""),
      Ok(DesktopShellDispatch::Portable(task)) => {
        let command_id = task.command_id();
        if let Some(session) = self
          .terminal_panel_controls
          .as_mut()
          .and_then(|controls| controls.session_mut(session_id))
        {
          session.start_command(command_id);
        }
        let window_handle = window.window_handle();
        cx.spawn(async move |app: WeakEntity<ChitinApp>, async_cx: &mut AsyncApp| {
          let result = task.result().await;
          let _ = async_cx.update_window(window_handle, |_, window, cx| {
            let _ = app.update(cx, |this, cx| {
              let output = match result {
                Ok(result) => {
                  let mut lines = host
                    .session()
                    .snapshot()
                    .map(|snapshot| shell_output_lines(&snapshot, command_id))
                    .unwrap_or_default();
                  let (_, result_lines) = super::presenter::terminal_outcome(result.outcome);
                  lines.extend(result_lines);
                  terminal_lines_ansi(&lines)
                }
                Err(error) => match host.session().snapshot() {
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
              this.finish_terminal_command(session_id, &output);
              if let Some(focus) = this.terminal_panel_controls.as_ref().and_then(|controls| {
                (controls.active_session == session_id)
                  .then(|| controls.focus(cx))
                  .flatten()
              }) {
                window.focus(&focus, cx);
              }
              cx.notify();
            });
          });
        })
        .detach();
      }
      Err(error) => self.finish_terminal_command(session_id, &format!("\x1b[31merror: {error}\x1b[0m")),
    }
  }

  /// Applies built-in shell completion to the terminal-owned editable line.
  fn complete_terminal_line(&self, session_id: TerminalSessionId, line: String) {
    match completion_action(BuiltinShell::complete(&line)) {
      CompletionAction::Insert(completed) => self.replace_terminal_line(session_id, completed),
      CompletionAction::List(candidates) => {
        let output = candidates.join("\n");
        if let Some(program) = self
          .terminal_panel_controls
          .as_ref()
          .and_then(|controls| controls.session(session_id))
          .and_then(|session| session.builtin.as_ref())
          .map(|builtin| &builtin.program)
          && let Ok(program) = program.lock()
          && let Err(error) = program.show_notice(&output)
        {
          log::error!("failed to display terminal completions: {error}");
        }
      }
      CompletionAction::None => {}
    }
  }

  /// Replaces the line edited by the in-process terminal backend.
  fn replace_terminal_line(&self, session_id: TerminalSessionId, line: String) {
    if let Some(program) = self
      .terminal_panel_controls
      .as_ref()
      .and_then(|controls| controls.session(session_id))
      .and_then(|session| session.builtin.as_ref())
      .map(|builtin| &builtin.program)
      && let Ok(mut program) = program.lock()
      && let Err(error) = program.replace_line(line)
    {
      log::error!("failed to replace built-in terminal line: {error}");
    }
  }

  /// Writes final command output and restores the shell prompt.
  fn finish_terminal_command(&mut self, session_id: TerminalSessionId, output: &str) {
    if let Some(program) = self
      .terminal_panel_controls
      .as_ref()
      .and_then(|controls| controls.session(session_id))
      .and_then(|session| session.builtin.as_ref())
      .map(|builtin| &builtin.program)
      && let Ok(mut program) = program.lock()
      && let Err(error) = program.finish_command(output)
    {
      log::error!("failed to finish built-in terminal command: {error}");
    }
    if let Some(session) = self
      .terminal_panel_controls
      .as_mut()
      .and_then(|controls| controls.session_mut(session_id))
    {
      session.finish_command();
    }
  }

  /// Clears the VT grid and resets the built-in line discipline.
  fn clear_terminal_screen(&mut self, session_id: TerminalSessionId) {
    if let Some(program) = self
      .terminal_panel_controls
      .as_ref()
      .and_then(|controls| controls.session(session_id))
      .and_then(|session| session.builtin.as_ref())
      .map(|builtin| &builtin.program)
      && let Ok(mut program) = program.lock()
      && let Err(error) = program.clear_screen()
    {
      log::error!("failed to clear built-in terminal: {error}");
    }
    if let Some(session) = self
      .terminal_panel_controls
      .as_mut()
      .and_then(|controls| controls.session_mut(session_id))
    {
      session.finish_command();
    }
  }

  /// Projects every running session's replaceable transcript into its own VT grid.
  pub(crate) fn sync_terminal_output(&mut self) {
    let Some(controls) = self.terminal_panel_controls.as_ref() else {
      return;
    };
    let running: Vec<_> = controls
      .sessions
      .iter()
      .filter_map(|session| {
        Some((
          session.id,
          session.active_shell_command?,
          session.rendered_running_output.clone(),
          session.builtin.clone()?,
        ))
      })
      .collect();

    for (session_id, command_id, previous_output, builtin) in running {
      let Ok(snapshot) = builtin.host.session().snapshot() else {
        continue;
      };
      let output = terminal_lines_ansi(&shell_output_lines(&snapshot, command_id));
      if output == previous_output {
        continue;
      }
      let Ok(mut program) = builtin.program.lock() else {
        log::error!("built-in terminal program lock is poisoned");
        continue;
      };
      if let Err(error) = program.update_command_output(&output) {
        log::error!("failed to update built-in terminal output: {error}");
        continue;
      }
      if let Some(session) = self
        .terminal_panel_controls
        .as_mut()
        .and_then(|controls| controls.session_mut(session_id))
      {
        session.rendered_running_output = output;
      }
    }
  }
}
