//! RCSB command adapter for the application task center.

use super::{BackgroundTaskCenter, TaskCenterError, TaskContext, TaskFailure, TaskHandle, TaskKind, TaskTarget};
use chitin_command::{
  ChitinCommand, CommandEventSink, CommandExecutionContext, DatabaseCommand, RcsbDownloadArguments,
};
use chitin_command_runtime::{CommandExecutionError, CommandExecutor, CommandOutcome, resolve_rcsb_download_paths};

/// Failure while preparing or submitting a database command task.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SubmitRcsbCommandError {
  /// Portable command preparation failed before task submission.
  #[error(transparent)]
  Execution(#[from] CommandExecutionError),
  /// The desktop task center rejected the prepared task.
  #[error(transparent)]
  TaskCenter(#[from] TaskCenterError),
}

/// Submits a typed RCSB download through the shared command executor.
///
/// # Parameters
///
/// * `center` schedules and tracks the asynchronous desktop task.
/// * `executor` is the application-wide portable command executor.
/// * `arguments` contains validated download inputs.
/// * `execution_context` supplies workspace paths and cancellation defaults.
///
/// # Returns
///
/// The task subscription and number of unique output artifacts.
pub(crate) fn submit_download(
  center: &BackgroundTaskCenter,
  executor: CommandExecutor,
  arguments: RcsbDownloadArguments,
  execution_context: CommandExecutionContext,
) -> Result<(TaskHandle, usize), SubmitRcsbCommandError> {
  let paths = resolve_rcsb_download_paths(&arguments, &execution_context)?;
  let total_items = paths.len();
  let targets = paths.into_iter().map(TaskTarget::File).collect();
  let handle = center.submit(
    TaskKind::DatabaseDownload {
      provider: "RCSB".to_string(),
    },
    targets,
    move |context| run_download(context, executor, arguments, execution_context),
  )?;
  Ok((handle, total_items))
}

/// Runs the portable executor inside the desktop task center.
///
/// # Parameters
///
/// * `context` reports observable state to the desktop task center.
/// * `executor` executes the typed command without GPUI dependencies.
/// * `arguments` contains the validated download request.
/// * `execution_context` supplies workspace paths and cancellation defaults.
///
/// # Returns
///
/// `Ok(())` after all artifacts are persisted and reported.
async fn run_download(
  context: TaskContext,
  executor: CommandExecutor,
  arguments: RcsbDownloadArguments,
  execution_context: CommandExecutionContext,
) -> Result<(), TaskFailure> {
  let event_context = context.clone();
  let events = CommandEventSink::new(move |event| event_context.report_command_event(event));
  let command = ChitinCommand::from(DatabaseCommand::DownloadRcsbStructure(arguments));
  let execution_context = execution_context.with_cancellation(context.cancellation_token());
  let outcome = executor
    .execute(command, execution_context, events)
    .await
    .map_err(|error| TaskFailure::new(error.to_string()))?;
  if !matches!(outcome, CommandOutcome::DatabaseDownload { .. }) {
    return Err(TaskFailure::new("RCSB task produced an unexpected command outcome"));
  }
  Ok(())
}
