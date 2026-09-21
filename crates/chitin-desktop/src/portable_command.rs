//! Desktop adapter for executing portable commands as background tasks.

use chitin_command::{CommandEventSink, CommandExecutionContext, DatabaseCommand, PortableCommand, StructureCommand};
use chitin_command_runtime::{CommandExecutionError, CommandExecutor, CommandOutcome, resolve_rcsb_download_paths};
use chitin_databases::CancellationToken;
use tokio::sync::oneshot;

use crate::tasks::{BackgroundTaskCenter, TaskCenterError, TaskFailure, TaskHandle, TaskKind, TaskTarget};

/// Terminal state reported to the frontend that originated a portable task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortableCommandCompletion {
  /// The command completed and produced a typed outcome.
  Succeeded,
  /// Cooperative cancellation was requested before execution finished.
  Cancelled,
  /// Execution failed without a cancellation request.
  Failed(String),
}

/// Complete desktop submission parameters for one portable command.
pub struct PortableCommandSubmission {
  kind: TaskKind,
  command: PortableCommand,
  context: CommandExecutionContext,
  cancellation: CancellationToken,
  events: CommandEventSink,
  completion: Option<Box<dyn FnOnce(PortableCommandCompletion) + Send>>,
}

impl PortableCommandSubmission {
  /// Creates a submission with silent frontend events and context cancellation.
  pub fn new(kind: TaskKind, command: PortableCommand, context: CommandExecutionContext) -> Self {
    let cancellation = context.cancellation.clone();
    Self {
      kind,
      command,
      context,
      cancellation,
      events: CommandEventSink::silent(),
      completion: None,
    }
  }

  /// Replaces the cancellation token shared with the task center.
  pub fn with_cancellation(mut self, cancellation: CancellationToken) -> Self {
    self.cancellation = cancellation;
    self
  }

  /// Forwards executor events to an additional frontend observer.
  pub fn with_events(mut self, events: CommandEventSink) -> Self {
    self.events = events;
    self
  }

  /// Installs a callback invoked exactly once when execution terminates.
  pub fn on_completion(mut self, completion: impl FnOnce(PortableCommandCompletion) + Send + 'static) -> Self {
    self.completion = Some(Box::new(completion));
    self
  }
}

/// Application-wide adapter from typed portable commands to background tasks.
#[derive(Clone)]
pub struct DesktopPortableCommandRunner {
  executor: CommandExecutor,
}

impl DesktopPortableCommandRunner {
  /// Creates a runner around the shared frontend-independent executor.
  pub fn new(executor: CommandExecutor) -> Self {
    Self { executor }
  }

  /// Resolves protected targets and submits a portable command.
  ///
  /// # Parameters
  ///
  /// * `center` owns the application background runtime and task registry.
  /// * `submission` contains the command, execution context, task metadata,
  ///   event observer, and optional lifecycle callback.
  ///
  /// # Returns
  ///
  /// A running task with its observation handle and typed outcome receiver.
  ///
  /// # Errors
  ///
  /// Returns [`DesktopPortableCommandError`] when command targets cannot be
  /// resolved or the task center rejects the submission.
  pub fn submit(
    &self,
    center: &BackgroundTaskCenter,
    submission: PortableCommandSubmission,
  ) -> Result<DesktopPortableCommandTask, DesktopPortableCommandError> {
    let PortableCommandSubmission {
      kind,
      command,
      context,
      cancellation,
      events,
      completion,
    } = submission;
    let targets = command_targets(&command, &context)?;
    let target_count = targets.len();
    let executor = self.executor.clone();
    let execution_cancellation = cancellation.clone();
    let (result_sender, result_receiver) = oneshot::channel();
    let task = center.submit_with_cancellation(kind, targets, cancellation, move |task_context| async move {
      let event_context = task_context.clone();
      let task_events = CommandEventSink::new(move |event| {
        event_context.report_command_event(event.clone());
        events.emit(event);
      });
      let execution_context = context.with_cancellation(task_context.cancellation_token());
      let result = executor.execute(command, execution_context, task_events).await;
      match result {
        Ok(_) if execution_cancellation.is_cancelled() => {
          finish_submission(completion, PortableCommandCompletion::Cancelled);
          let _ = result_sender.send(PortableCommandResult::Cancelled);
          Ok(())
        }
        Ok(outcome) => {
          finish_submission(completion, PortableCommandCompletion::Succeeded);
          let _ = result_sender.send(PortableCommandResult::Succeeded(outcome));
          Ok(())
        }
        Err(error) if execution_cancellation.is_cancelled() => {
          let failure = TaskFailure::new(error.to_string());
          finish_submission(completion, PortableCommandCompletion::Cancelled);
          let _ = result_sender.send(PortableCommandResult::Cancelled);
          Err(failure)
        }
        Err(error) => {
          let message = error.to_string();
          finish_submission(completion, PortableCommandCompletion::Failed(message.clone()));
          let _ = result_sender.send(PortableCommandResult::Failed(error));
          Err(TaskFailure::new(message))
        }
      }
    })?;
    Ok(DesktopPortableCommandTask {
      task,
      target_count,
      result: result_receiver,
    })
  }
}

/// Running portable command exposed to desktop presenters and protocol hosts.
pub struct DesktopPortableCommandTask {
  task: TaskHandle,
  target_count: usize,
  result: oneshot::Receiver<PortableCommandResult>,
}

impl DesktopPortableCommandTask {
  /// Returns the background task observation handle.
  pub const fn task(&self) -> &TaskHandle {
    &self.task
  }

  /// Returns the number of resources reserved by the task.
  pub const fn target_count(&self) -> usize {
    self.target_count
  }

  /// Consumes the result receiver and returns the task observation handle.
  pub fn into_task_handle(self) -> TaskHandle {
    self.task
  }

  /// Waits for and returns the typed executor outcome.
  pub async fn result(self) -> Result<CommandOutcome, DesktopPortableCommandError> {
    match self
      .result
      .await
      .map_err(|_| DesktopPortableCommandError::ResultChannelClosed)?
    {
      PortableCommandResult::Succeeded(outcome) => Ok(outcome),
      PortableCommandResult::Cancelled => Err(DesktopPortableCommandError::Cancelled),
      PortableCommandResult::Failed(error) => Err(DesktopPortableCommandError::Execution(error)),
    }
  }
}

/// Typed terminal result transferred from the task runtime to its caller.
enum PortableCommandResult {
  /// Execution completed with a domain outcome.
  Succeeded(CommandOutcome),
  /// Cooperative cancellation won the terminal-state race.
  Cancelled,
  /// Execution failed before producing an outcome.
  Failed(CommandExecutionError),
}

/// Failure while preparing, submitting, or observing a portable desktop task.
#[derive(Debug, thiserror::Error)]
pub enum DesktopPortableCommandError {
  /// Portable command preparation or execution failed.
  #[error(transparent)]
  Execution(#[from] CommandExecutionError),
  /// The application task center rejected the command.
  #[error(transparent)]
  TaskCenter(#[from] TaskCenterError),
  /// The task ended without publishing its typed outcome.
  #[error("portable command result channel closed before completion")]
  ResultChannelClosed,
  /// Cooperative cancellation completed before a result was presented.
  #[error("portable command was cancelled")]
  Cancelled,
}

/// Notifies an optional origin-specific lifecycle observer.
fn finish_submission(
  completion: Option<Box<dyn FnOnce(PortableCommandCompletion) + Send>>,
  state: PortableCommandCompletion,
) {
  if let Some(completion) = completion {
    completion(state);
  }
}

/// Resolves resources protected while a portable command is active.
fn command_targets(
  command: &PortableCommand,
  context: &CommandExecutionContext,
) -> Result<Vec<TaskTarget>, CommandExecutionError> {
  match command {
    PortableCommand::Database(DatabaseCommand::DownloadRcsbStructure(arguments)) => {
      resolve_rcsb_download_paths(arguments, context).map(|paths| paths.into_iter().map(TaskTarget::File).collect())
    }
    PortableCommand::Structure(StructureCommand::Inspect(arguments)) => {
      Ok(vec![TaskTarget::File(arguments.input.input.clone())])
    }
    PortableCommand::Structure(StructureCommand::Validate(arguments)) => {
      Ok(vec![TaskTarget::File(arguments.input.input.clone())])
    }
  }
}

#[cfg(test)]
mod tests {
  use std::path::PathBuf;

  use chitin_command::{CommandOutputFormat, StructureInputArguments, StructureValidateArguments};
  use chitin_databases::{ClientConfig, providers::rcsb::StructureFormat};

  use super::*;

  #[test]
  fn runner_should_execute_structure_validation_and_publish_result() -> Result<(), Box<dyn std::error::Error>> {
    let center = BackgroundTaskCenter::new();
    let runner = DesktopPortableCommandRunner::new(CommandExecutor::new(ClientConfig::default()));
    let command = StructureCommand::Validate(StructureValidateArguments {
      input: StructureInputArguments {
        input: PathBuf::from("-"),
        format: Some(StructureFormat::Pdb),
      },
      output: CommandOutputFormat::Text,
    });
    let context = CommandExecutionContext::new(".").with_standard_input(
      b"ATOM      1  CA  GLY A   1       1.000   2.000   3.000  1.00 20.00           C  \nEND\n".to_vec(),
    );
    let task = runner.submit(
      &center,
      PortableCommandSubmission::new(
        TaskKind::BuiltinShell {
          command_id: command.id(),
        },
        command.into(),
        context,
      ),
    )?;
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;

    let outcome = runtime.block_on(task.result())?;

    assert!(matches!(
      outcome,
      CommandOutcome::StructureValidation(validation) if validation.is_valid()
    ));
    Ok(())
  }
}
