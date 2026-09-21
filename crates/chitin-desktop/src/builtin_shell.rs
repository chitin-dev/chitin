//! Desktop host adapter for the frontend-independent built-in shell.

use std::path::PathBuf;

use chitin_builtin_shell::{
  BuiltinShell, BuiltinShellError, ShellBuiltinEffect, ShellCommandId, ShellCommandTarget, ShellExecutionResult,
  ShellInvocationSource, ShellLineSubmission, ShellSubmission,
};
use chitin_command::{ChitinCommand, CommandEventSink, CommandExecutionContext};
use gpui::{AppContext, AsyncApp, Context, WeakEntity, Window};

use crate::{
  app::ChitinApp,
  portable_command::{
    DesktopPortableCommandError, DesktopPortableCommandRunner, DesktopPortableCommandTask, PortableCommandCompletion,
    PortableCommandSubmission,
  },
  tasks::{BackgroundTaskCenter, TaskHandle, TaskKind},
};

/// Desktop-owned shell session and adapters for application background work.
#[derive(Clone)]
pub struct DesktopShellHost {
  session: BuiltinShell,
}

impl DesktopShellHost {
  /// Creates a desktop shell from workspace-aware execution defaults.
  pub fn new(context: CommandExecutionContext) -> Self {
    Self {
      session: BuiltinShell::from_context(context),
    }
  }

  /// Returns the session shared with terminal and agent callers.
  pub fn session(&self) -> &BuiltinShell {
    &self.session
  }

  /// Parses and reserves an attributed command-line submission.
  pub fn submit_line(
    &self,
    input: impl Into<String>,
    source: ShellInvocationSource,
  ) -> Result<ShellLineSubmission, DesktopShellHostError> {
    Ok(self.session.submit_line(input, source)?)
  }

  /// Reserves an already typed command for an agent or internal subsystem.
  ///
  /// # Parameters
  ///
  /// * `input` is a human-readable audit representation of the request.
  /// * `command` is the validated command sent across the shell/kernel bridge.
  /// * `source` attributes the invocation to its agent or subsystem.
  ///
  /// # Returns
  ///
  /// A correlated submission ready for desktop routing.
  pub fn submit_typed(
    &self,
    input: impl Into<String>,
    command: ChitinCommand,
    source: ShellInvocationSource,
  ) -> Result<ShellSubmission, DesktopShellHostError> {
    Ok(self.session.submit_typed(input, command, source)?)
  }

  /// Runs a portable shell submission through the application task center.
  ///
  /// # Parameters
  ///
  /// * `submission` contains the correlated typed command and execution context.
  /// * `tasks` owns the shared Tokio executor and durable task state.
  /// * `runner` resolves targets and dispatches portable work through the
  ///   application-wide command executor.
  ///
  /// # Returns
  ///
  /// A task handle for UI observation and a structured result receiver for
  /// terminal or agent callers.
  pub fn submit_portable(
    &self,
    submission: ShellSubmission,
    tasks: &BackgroundTaskCenter,
    runner: &DesktopPortableCommandRunner,
  ) -> Result<DesktopShellTask, DesktopShellHostError> {
    if submission.target() != ShellCommandTarget::Portable {
      return Err(DesktopShellHostError::WrongTarget {
        id: submission.id(),
        target: submission.target(),
      });
    }

    let id = submission.id();
    let command_id = submission.command().id();
    let ChitinCommand::Portable(command) = submission.command().clone() else {
      return Err(DesktopShellHostError::WrongTarget {
        id,
        target: submission.target(),
      });
    };
    let cancellation = submission.context().cancellation.clone();
    let context = submission.context().clone();
    let events = self
      .session
      .portable_event_sink(&submission, CommandEventSink::silent())?;
    let completion_session = self.session.clone();
    let failure_session = self.session.clone();
    let task = runner.submit(
      tasks,
      PortableCommandSubmission::new(TaskKind::BuiltinShell { command_id }, command, context)
        .with_cancellation(cancellation)
        .with_events(events)
        .on_completion(move |completion| {
          let result = match completion {
            PortableCommandCompletion::Succeeded => completion_session.complete_portable(id),
            PortableCommandCompletion::Cancelled => completion_session.cancel_portable(id),
            PortableCommandCompletion::Failed(message) => completion_session.fail_submission(id, message),
          };
          if let Err(error) = result {
            log::error!("failed to update built-in shell completion state: {error}");
          }
        }),
    );
    match task {
      Ok(task) => Ok(DesktopShellTask { command_id: id, task }),
      Err(error) => {
        let _ = failure_session.fail_submission(id, error.to_string());
        Err(error.into())
      }
    }
  }

  /// Completes a command after the desktop frontend applies its UI mutation.
  pub fn complete_frontend(&self, id: ShellCommandId) -> Result<(), DesktopShellHostError> {
    Ok(self.session.complete_frontend(id)?)
  }

  /// Records a desktop routing failure for a frontend command.
  pub fn fail_frontend(&self, id: ShellCommandId, message: impl Into<String>) -> Result<(), DesktopShellHostError> {
    Ok(self.session.fail_frontend(id, message)?)
  }
}

/// Running portable shell command exposed to desktop and agent consumers.
pub struct DesktopShellTask {
  command_id: ShellCommandId,
  task: DesktopPortableCommandTask,
}

impl DesktopShellTask {
  /// Returns the shell-local command identity.
  pub const fn command_id(&self) -> ShellCommandId {
    self.command_id
  }

  /// Returns the background task observation handle.
  pub const fn task(&self) -> &TaskHandle {
    self.task.task()
  }

  /// Waits for the structured portable-command result.
  pub async fn result(self) -> Result<ShellExecutionResult, DesktopShellHostError> {
    let outcome = self.task.result().await?;
    Ok(ShellExecutionResult {
      id: self.command_id,
      outcome,
    })
  }
}

/// Result of routing one shell submission through the desktop host.
pub enum DesktopShellDispatch {
  /// Clap produced help text without scheduling a command.
  Display { output: String },
  /// The shell session synchronously produced a presentation effect.
  ShellBuiltin { effect: ShellBuiltinEffect },
  /// The command mutated desktop state synchronously.
  Frontend { command_id: ShellCommandId },
  /// The command is running through the shared background executor.
  Portable(DesktopShellTask),
}

/// Failure while preparing or executing a desktop-hosted shell command.
#[derive(Debug, thiserror::Error)]
pub enum DesktopShellHostError {
  /// The frontend-independent shell rejected the request or execution.
  #[error(transparent)]
  Shell(#[from] BuiltinShellError),
  /// Portable command preparation, submission, or execution failed.
  #[error(transparent)]
  Portable(#[from] DesktopPortableCommandError),
  /// A submission was sent to an incompatible host route.
  #[error("shell command {} has target {target:?}, which cannot use this route", id.get())]
  WrongTarget {
    /// Shell-local identity of the incorrectly routed command.
    id: ShellCommandId,
    /// Target selected by the frontend-independent shell.
    target: ShellCommandTarget,
  },
}

/// Builds workspace-aware defaults for a desktop shell session.
pub(crate) fn desktop_shell_context(workspace_root: Option<PathBuf>) -> CommandExecutionContext {
  match workspace_root {
    Some(root) => CommandExecutionContext::new(&root)
      .with_workspace_root(&root)
      .with_default_download_root(root.join(".chitin").join("download")),
    None => CommandExecutionContext::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
  }
}

impl ChitinApp {
  /// Returns the shared built-in shell session used by desktop and agent callers.
  pub fn builtin_shell(&self) -> &BuiltinShell {
    self.builtin_shell.session()
  }

  /// Parses and routes one textual built-in shell command through the desktop host.
  ///
  /// # Parameters
  ///
  /// * `input` is one complete command line.
  /// * `source` identifies the interactive, agent, or system caller.
  /// * `window` provides the context required by frontend UI commands.
  /// * `cx` schedules portable task observation on the GPUI main thread.
  ///
  /// # Returns
  ///
  /// A completed frontend dispatch or a handle to the running portable command.
  pub fn submit_builtin_shell_line(
    &mut self,
    input: impl Into<String>,
    source: ShellInvocationSource,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Result<DesktopShellDispatch, DesktopShellHostError> {
    match self.builtin_shell.submit_line(input, source)? {
      ShellLineSubmission::Command(submission) => self.route_builtin_shell_submission(submission, window, cx),
      ShellLineSubmission::ShellBuiltin(effect) => Ok(DesktopShellDispatch::ShellBuiltin { effect }),
      ShellLineSubmission::Display(output) => Ok(DesktopShellDispatch::Display { output }),
    }
  }

  /// Routes an already typed command from an agent or application subsystem.
  ///
  /// # Parameters
  ///
  /// * `input` is a human-readable audit representation of the request.
  /// * `command` contains the validated command identity and arguments.
  /// * `source` identifies the agent or subsystem initiating the request.
  /// * `window` provides the context required by frontend UI commands.
  /// * `cx` schedules portable task observation on the GPUI main thread.
  ///
  /// # Returns
  ///
  /// A completed frontend dispatch or a handle to the running portable command.
  pub fn submit_builtin_shell_command(
    &mut self,
    input: impl Into<String>,
    command: ChitinCommand,
    source: ShellInvocationSource,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Result<DesktopShellDispatch, DesktopShellHostError> {
    let submission = self.builtin_shell.submit_typed(input, command, source)?;
    self.route_builtin_shell_submission(submission, window, cx)
  }

  /// Routes one prepared submission without reparsing its typed command.
  fn route_builtin_shell_submission(
    &mut self,
    submission: ShellSubmission,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Result<DesktopShellDispatch, DesktopShellHostError> {
    match submission.target() {
      ShellCommandTarget::Frontend => {
        let command_id = submission.id();
        let ChitinCommand::Frontend(command) = submission.command() else {
          return Err(DesktopShellHostError::WrongTarget {
            id: command_id,
            target: submission.target(),
          });
        };
        self.dispatch_command_with_window(command.clone(), window, cx);
        self.builtin_shell.complete_frontend(command_id)?;
        Ok(DesktopShellDispatch::Frontend { command_id })
      }
      ShellCommandTarget::Portable => {
        let running = self
          .builtin_shell
          .submit_portable(submission, &self.tasks, &self.portable_commands)?;
        observe_shell_task(running.task().clone(), window, cx);
        Ok(DesktopShellDispatch::Portable(running))
      }
    }
  }
}

/// Notifies GPUI while a portable shell command changes task state.
fn observe_shell_task(mut task: TaskHandle, window: &Window, cx: &mut Context<ChitinApp>) {
  let window_handle = window.window_handle();
  cx.spawn(async move |app: WeakEntity<ChitinApp>, async_cx: &mut AsyncApp| {
    while let Some(snapshot) = task.changed().await {
      let terminal = snapshot.state.is_terminal();
      let _ = async_cx.update_window(window_handle, |_, _, cx| {
        let _ = app.update(cx, |_, cx| cx.notify());
      });
      if terminal {
        break;
      }
    }
  })
  .detach();
}

#[cfg(test)]
mod tests {
  use chitin_command::{CommandExecutor, CommandOutputFormat};
  use chitin_databases::{ClientConfig, providers::rcsb::StructureFormat};

  use super::*;

  #[test]
  fn portable_submission_should_return_a_correlated_kernel_result() -> Result<(), Box<dyn std::error::Error>> {
    let host = DesktopShellHost::new(CommandExecutionContext::new(".").with_standard_input(
      b"ATOM      1  CA  GLY A   1       1.000   2.000   3.000  1.00 20.00           C  \nEND\n".to_vec(),
    ));
    let submission = host
      .submit_line(
        "structure validate - --format pdb",
        ShellInvocationSource::Agent {
          name: "validation-agent".to_owned(),
        },
      )?
      .into_command()?;
    let command_id = submission.id();
    let tasks = BackgroundTaskCenter::new();
    let runner = DesktopPortableCommandRunner::new(CommandExecutor::new(ClientConfig::default()));
    let running = host.submit_portable(submission, &tasks, &runner)?;
    let result_runtime = tokio::runtime::Builder::new_current_thread().build()?;

    let result = result_runtime.block_on(running.result())?;

    assert_eq!(result.id, command_id);
    assert!(matches!(
      result.outcome,
      chitin_command::CommandOutcome::StructureValidation(validation)
        if validation.format == StructureFormat::Pdb
          && validation.output == CommandOutputFormat::Text
          && validation.is_valid()
    ));
    Ok(())
  }

  #[test]
  fn frontend_submission_should_remain_available_for_desktop_dispatch() -> Result<(), DesktopShellHostError> {
    let host = DesktopShellHost::new(CommandExecutionContext::new("."));

    let submission = host
      .submit_line("tab.close", ShellInvocationSource::Interactive)?
      .into_command()?;

    assert_eq!(submission.target(), ShellCommandTarget::Frontend);
    host.complete_frontend(submission.id())?;
    assert!(host.session().snapshot()?.active.is_none());
    Ok(())
  }
}
