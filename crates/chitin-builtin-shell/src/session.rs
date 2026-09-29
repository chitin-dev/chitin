//! Built-in shell session state and command routing.

use std::{
  path::PathBuf,
  sync::{Arc, Mutex, MutexGuard},
};

use crate::grammar::{BuiltinCommandLine, ShellBuiltin, parse_builtin_command_line};
use chitin_command::{
  ChitinCommand, CommandEventSink, CommandExecutionContext, CommandExecutionDomain, CommandExecutionEvent, CommandId,
  CommandMessage, CommandProgress,
};
use chitin_command::{CommandExecutionError, CommandExecutor, CommandOutcome};
use chitin_databases::{CancellationToken, PersistedArtifact};

use crate::{ShellEvent, ShellEventSink};

/// Monotonic identity assigned to one shell submission.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShellCommandId(u64);

impl ShellCommandId {
  /// Returns the numeric identity used inside this shell session.
  pub const fn get(self) -> u64 {
    self.0
  }
}

/// Execution boundary selected for a parsed shell command.
pub type ShellCommandTarget = CommandExecutionDomain;

/// Origin of a command invocation crossing the built-in shell bridge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellInvocationSource {
  /// A person entered text in the built-in terminal.
  Interactive,
  /// An application agent submitted a structured command.
  Agent {
    /// Stable agent identity used for attribution and diagnostics.
    name: String,
  },
  /// Internal application automation submitted the command.
  System {
    /// Stable subsystem identity used for attribution and diagnostics.
    name: String,
  },
}

/// Lifecycle state retained for a submitted shell command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellExecutionStatus {
  /// The command has been accepted and is executing.
  Running,
  /// Cancellation was requested and execution is winding down.
  Cancelling,
  /// Execution completed successfully.
  Succeeded,
  /// Execution returned an error.
  Failed,
  /// Execution stopped after cooperative cancellation.
  Cancelled,
}

impl ShellExecutionStatus {
  /// Returns whether no further execution events are expected.
  pub const fn is_terminal(self) -> bool {
    matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
  }
}

/// Persistent history for one accepted shell command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellExecutionRecord {
  /// Shell-local submission identity.
  pub id: ShellCommandId,
  /// Original command text entered by the user.
  pub input: String,
  /// Typed request retained for audit, replay, and agent/kernel correlation.
  pub command: ChitinCommand,
  /// Stable command identity resolved by the parser.
  pub command_id: CommandId,
  /// Actor or subsystem that submitted the command.
  pub source: ShellInvocationSource,
  /// Executor boundary selected for the command.
  pub target: ShellCommandTarget,
  /// Current lifecycle state.
  pub status: ShellExecutionStatus,
}

/// Active command information exposed in a shell snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellActiveCommand {
  /// Shell-local submission identity.
  pub id: ShellCommandId,
  /// Stable identity of the parsed command.
  pub command_id: CommandId,
  /// Executor boundary selected for the command.
  pub target: ShellCommandTarget,
  /// Current non-terminal lifecycle state.
  pub status: ShellExecutionStatus,
}

/// Content retained in the built-in shell scrollback model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellTranscriptContent {
  /// Original input shown beside the shell prompt.
  Input(String),
  /// Most recent progress for a portable command.
  Progress(CommandProgress),
  /// Structured informational or warning message.
  Message(CommandMessage),
  /// Artifact persisted by a command.
  Artifact(Box<PersistedArtifact>),
  /// Successful terminal marker.
  Completed,
  /// Failed terminal marker and its user-facing message.
  Failed(String),
  /// Cooperative cancellation marker.
  Cancelled,
}

/// One ordered item in shell scrollback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellTranscriptEntry {
  /// Submission that produced this content.
  pub command_id: ShellCommandId,
  /// Render-neutral content interpreted by the eventual terminal view.
  pub content: ShellTranscriptContent,
}

/// Read-only copy of session state suitable for presentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuiltinShellSnapshot {
  /// Directory used to resolve relative command paths.
  pub working_directory: PathBuf,
  /// Currently executing command, when present.
  pub active: Option<ShellActiveCommand>,
  /// Accepted commands in submission order.
  pub executions: Vec<ShellExecutionRecord>,
  /// Ordered render-neutral shell scrollback.
  pub transcript: Vec<ShellTranscriptEntry>,
}

/// Parsed command and per-invocation resources prepared by a shell session.
#[derive(Clone, Debug)]
pub struct ShellSubmission {
  id: ShellCommandId,
  command: ChitinCommand,
  target: ShellCommandTarget,
  source: ShellInvocationSource,
  context: CommandExecutionContext,
}

/// Result of evaluating one textual built-in shell line.
#[derive(Clone, Debug)]
pub enum ShellLineSubmission {
  /// A typed command was reserved for execution.
  Command(ShellSubmission),
  /// The shell session applied a built-in command synchronously.
  ShellBuiltin(ShellBuiltinEffect),
  /// Help text should be displayed without invoking an executor.
  Display(String),
}

/// Presentation effect produced by a command owned by the shell session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellBuiltinEffect {
  /// Remove visible command blocks while preserving navigable history.
  ClearScrollback,
}

impl ShellLineSubmission {
  /// Extracts an executable submission from a command-producing line.
  pub fn into_command(self) -> Result<ShellSubmission, BuiltinShellError> {
    match self {
      Self::Command(submission) => Ok(submission),
      Self::Display(output) => Err(BuiltinShellError::DisplayOnly { output }),
      Self::ShellBuiltin(effect) => Err(BuiltinShellError::BuiltinOnly { effect }),
    }
  }
}

impl ShellSubmission {
  /// Returns the shell-local submission identity.
  pub const fn id(&self) -> ShellCommandId {
    self.id
  }

  /// Returns the parsed typed command.
  pub const fn command(&self) -> &ChitinCommand {
    &self.command
  }

  /// Returns the executor boundary selected for this command.
  pub const fn target(&self) -> ShellCommandTarget {
    self.target
  }

  /// Returns the actor or subsystem that initiated this command.
  pub const fn source(&self) -> &ShellInvocationSource {
    &self.source
  }

  /// Returns the resources prepared for this invocation.
  pub const fn context(&self) -> &CommandExecutionContext {
    &self.context
  }
}

/// Correlated structured result returned by the portable command kernel.
#[derive(Clone, Debug, PartialEq)]
pub struct ShellExecutionResult {
  /// Shell-local identity shared with the originating submission and events.
  pub id: ShellCommandId,
  /// Domain result returned without terminal-specific string conversion.
  pub outcome: CommandOutcome,
}

/// Failure while parsing, routing, or updating a built-in shell command.
#[derive(Debug, thiserror::Error)]
pub enum BuiltinShellError {
  /// The submitted command line is syntactically or semantically invalid.
  #[error(transparent)]
  Parse(#[from] crate::grammar::CommandLineParseError),
  /// A caller requiring execution received display-only help text.
  #[error("shell line produced display output instead of an executable command")]
  DisplayOnly {
    /// Help text produced by the command-line grammar.
    output: String,
  },
  /// A caller requiring execution received a synchronous shell builtin.
  #[error("shell line produced a built-in effect instead of an executable command")]
  BuiltinOnly {
    /// Effect already applied to the shell session.
    effect: ShellBuiltinEffect,
  },
  /// Another foreground command is still active.
  #[error("shell command {} is still active", active_id.get())]
  Busy {
    /// Identity of the active command preventing submission.
    active_id: ShellCommandId,
  },
  /// A portable executor was requested for a frontend command.
  #[error("shell command {} requires the frontend executor", id.get())]
  FrontendCommand {
    /// Identity of the incorrectly routed submission.
    id: ShellCommandId,
  },
  /// A completion or failure referred to a command other than the active one.
  #[error("shell command {} is not the active command", id.get())]
  InactiveCommand {
    /// Identity supplied by the caller.
    id: ShellCommandId,
  },
  /// The shared command executor rejected or failed the command.
  #[error(transparent)]
  Execution(#[from] CommandExecutionError),
  /// Session state could not be accessed after a panic poisoned its mutex.
  #[error("built-in shell session state is unavailable")]
  StateUnavailable,
}

#[derive(Debug)]
struct ActiveCommand {
  id: ShellCommandId,
  command_id: CommandId,
  target: ShellCommandTarget,
  status: ShellExecutionStatus,
  cancellation: CancellationToken,
}

#[derive(Debug)]
struct ShellState {
  context: CommandExecutionContext,
  next_id: u64,
  active: Option<ActiveCommand>,
  history: Vec<String>,
  history_cursor: Option<usize>,
  navigation_draft: String,
  executions: Vec<ShellExecutionRecord>,
  transcript: Vec<ShellTranscriptEntry>,
}

/// Thread-safe session for Chitin's built-in command language.
///
/// A session accepts one foreground command at a time. Portable commands are
/// executed through [`CommandExecutor`], while frontend commands are returned
/// to the host and must be finalized with [`BuiltinShell::complete_frontend`]
/// or [`BuiltinShell::fail_frontend`].
#[derive(Clone)]
pub struct BuiltinShell {
  state: Arc<Mutex<ShellState>>,
  events: ShellEventSink,
}

impl BuiltinShell {
  /// Creates a session rooted at the supplied working directory.
  pub fn new(working_directory: impl Into<PathBuf>) -> Self {
    Self::from_context(CommandExecutionContext::new(working_directory))
  }

  /// Creates a session that reports lifecycle changes to an event sink.
  pub fn with_event_sink(working_directory: impl Into<PathBuf>, events: ShellEventSink) -> Self {
    Self::from_context_and_event_sink(CommandExecutionContext::new(working_directory), events)
  }

  /// Creates a session from frontend-supplied execution defaults.
  pub fn from_context(context: CommandExecutionContext) -> Self {
    Self::from_context_and_event_sink(context, ShellEventSink::silent())
  }

  /// Creates a configured session that reports lifecycle changes to an event sink.
  pub fn from_context_and_event_sink(context: CommandExecutionContext, events: ShellEventSink) -> Self {
    Self {
      state: Arc::new(Mutex::new(ShellState {
        context,
        next_id: 1,
        active: None,
        history: Vec::new(),
        history_cursor: None,
        navigation_draft: String::new(),
        executions: Vec::new(),
        transcript: Vec::new(),
      })),
      events,
    }
  }

  /// Sets the workspace root supplied to subsequent commands.
  pub fn set_workspace_root(&self, workspace_root: Option<PathBuf>) -> Result<(), BuiltinShellError> {
    self.lock_state()?.context.workspace_root = workspace_root;
    Ok(())
  }

  /// Sets the default download root supplied to subsequent commands.
  pub fn set_default_download_root(&self, root: Option<PathBuf>) -> Result<(), BuiltinShellError> {
    self.lock_state()?.context.default_download_root = root;
    Ok(())
  }

  /// Sets standard-input bytes supplied to the next command submission.
  pub fn set_standard_input(&self, bytes: Option<Arc<[u8]>>) -> Result<(), BuiltinShellError> {
    self.lock_state()?.context.standard_input = bytes;
    Ok(())
  }

  /// Parses a command line and reserves it as the active foreground command.
  ///
  /// # Parameters
  ///
  /// * `input` is one complete built-in shell command line.
  ///
  /// # Returns
  ///
  /// A typed submission carrying its execution target and isolated cancellation
  /// token.
  ///
  /// # Errors
  ///
  /// Returns [`BuiltinShellError::Busy`] while another command is active, or a
  /// parse error when the command line is invalid.
  pub fn submit(&self, input: impl Into<String>) -> Result<ShellLineSubmission, BuiltinShellError> {
    self.submit_line(input, ShellInvocationSource::Interactive)
  }

  /// Submits a command line attributed to a specific invocation source.
  ///
  /// # Parameters
  ///
  /// * `input` is the command text used for history and human-readable audit.
  /// * `source` identifies the interactive, agent, or system caller.
  ///
  /// # Returns
  ///
  /// A parsed typed submission correlated with its shell-local identity.
  pub fn submit_line(
    &self,
    input: impl Into<String>,
    source: ShellInvocationSource,
  ) -> Result<ShellLineSubmission, BuiltinShellError> {
    let input = input.into();
    let parsed = match parse_builtin_command_line(&input) {
      Ok(parsed) => parsed,
      Err(error) => {
        self.remember_unsubmitted_input(input.clone())?;
        return Err(error.into());
      }
    };
    match parsed {
      BuiltinCommandLine::Portable(command) => self
        .submit_typed(input, command.into(), source)
        .map(ShellLineSubmission::Command),
      BuiltinCommandLine::Frontend(command) => self
        .submit_typed(input, command.into(), source)
        .map(ShellLineSubmission::Command),
      BuiltinCommandLine::ShellBuiltin(command) => self
        .execute_builtin(input, command)
        .map(ShellLineSubmission::ShellBuiltin),
      BuiltinCommandLine::Display(output) => {
        self.remember_unsubmitted_input(input)?;
        Ok(ShellLineSubmission::Display(output))
      }
    }
  }

  /// Applies a command whose state and lifecycle belong to the shell session.
  ///
  /// # Parameters
  ///
  /// * `input` is retained in navigable history after successful execution.
  /// * `command` identifies the synchronous session mutation to apply.
  ///
  /// # Returns
  ///
  /// A presentation effect for the host terminal to mirror.
  ///
  /// # Errors
  ///
  /// Returns [`BuiltinShellError::Busy`] while a foreground command is active,
  /// or [`BuiltinShellError::StateUnavailable`] when session state is poisoned.
  fn execute_builtin(&self, input: String, command: ShellBuiltin) -> Result<ShellBuiltinEffect, BuiltinShellError> {
    let mut state = self.lock_state()?;
    if let Some(active) = state.active.as_ref() {
      return Err(BuiltinShellError::Busy { active_id: active.id });
    }
    state.history.push(input);
    state.history_cursor = None;
    state.navigation_draft.clear();
    match command {
      ShellBuiltin::Clear => {
        state.transcript.clear();
        Ok(ShellBuiltinEffect::ClearScrollback)
      }
    }
  }

  /// Submits an already typed command from an agent or application subsystem.
  ///
  /// This is the primary non-interactive bridge API. It prevents agents from
  /// serializing validated arguments back into shell text merely to invoke a
  /// computation kernel.
  ///
  /// # Parameters
  ///
  /// * `input` is a human-readable audit representation of the request.
  /// * `command` contains the validated command identity and arguments.
  /// * `source` identifies the agent or subsystem initiating the request.
  ///
  /// # Returns
  ///
  /// A correlated submission ready for the portable or frontend executor.
  pub fn submit_typed(
    &self,
    input: impl Into<String>,
    command: ChitinCommand,
    source: ShellInvocationSource,
  ) -> Result<ShellSubmission, BuiltinShellError> {
    let input = input.into();
    let target = command.execution_domain();
    let cancellation = CancellationToken::new();
    let (submission, record) = {
      let mut state = self.lock_state()?;
      if let Some(active) = state.active.as_ref() {
        return Err(BuiltinShellError::Busy { active_id: active.id });
      }

      let id = ShellCommandId(state.next_id);
      state.next_id = state.next_id.saturating_add(1);
      let command_id = command.id();
      let context = state.context.clone().with_cancellation(cancellation.clone());
      let record = ShellExecutionRecord {
        id,
        input: input.clone(),
        command: command.clone(),
        command_id,
        source: source.clone(),
        target,
        status: ShellExecutionStatus::Running,
      };
      state.history.push(input.clone());
      state.history_cursor = None;
      state.navigation_draft.clear();
      state.executions.push(record.clone());
      state.transcript.push(ShellTranscriptEntry {
        command_id: id,
        content: ShellTranscriptContent::Input(input),
      });
      state.active = Some(ActiveCommand {
        id,
        command_id,
        target,
        status: ShellExecutionStatus::Running,
        cancellation,
      });
      (
        ShellSubmission {
          id,
          command,
          target,
          source,
          context,
        },
        record,
      )
    };
    self.events.emit(ShellEvent::Submitted(record));
    Ok(submission)
  }

  /// Executes a portable submission and records its events and terminal state.
  ///
  /// # Parameters
  ///
  /// * `submission` is the active value returned by [`BuiltinShell::submit`].
  /// * `executor` runs database and structure commands without frontend state.
  ///
  /// # Returns
  ///
  /// The typed portable-command outcome for presentation by the shell host.
  ///
  /// # Errors
  ///
  /// Returns an error when the submission is stale, requires the frontend, or
  /// fails in the shared command executor. Execution failures are retained in
  /// the transcript before being returned.
  pub async fn execute_portable(
    &self,
    submission: ShellSubmission,
    executor: &CommandExecutor,
  ) -> Result<ShellExecutionResult, BuiltinShellError> {
    self
      .execute_portable_with_events(submission, executor, CommandEventSink::silent())
      .await
  }

  /// Executes a portable submission while forwarding command events to a host.
  ///
  /// # Parameters
  ///
  /// * `submission` is the active value returned by [`BuiltinShell::submit`].
  /// * `executor` runs database and structure commands without frontend state.
  /// * `host_events` receives the same structured events recorded by the shell.
  ///
  /// # Returns
  ///
  /// The correlated typed outcome produced by the portable command kernel.
  ///
  /// # Errors
  ///
  /// Returns an error when routing is invalid or command execution fails. The
  /// shell records the terminal state before returning an execution failure.
  pub async fn execute_portable_with_events(
    &self,
    submission: ShellSubmission,
    executor: &CommandExecutor,
    host_events: CommandEventSink,
  ) -> Result<ShellExecutionResult, BuiltinShellError> {
    let id = submission.id;
    let events = self.portable_event_sink(&submission, host_events)?;
    let ChitinCommand::Portable(command) = submission.command else {
      return Err(BuiltinShellError::FrontendCommand { id: submission.id });
    };
    match executor.execute(command, submission.context, events).await {
      Ok(outcome) => {
        self.finish(id, ShellExecutionStatus::Succeeded, ShellTranscriptContent::Completed)?;
        Ok(ShellExecutionResult { id, outcome })
      }
      Err(error) => {
        let cancelled = self.active_is_cancelled(id)?;
        let status = if cancelled {
          ShellExecutionStatus::Cancelled
        } else {
          ShellExecutionStatus::Failed
        };
        let content = if cancelled {
          ShellTranscriptContent::Cancelled
        } else {
          ShellTranscriptContent::Failed(error.to_string())
        };
        self.finish(id, status, content)?;
        Err(BuiltinShellError::Execution(error))
      }
    }
  }

  /// Creates an event sink that records portable executor events in this session.
  ///
  /// # Parameters
  ///
  /// * `submission` identifies the active portable command receiving events.
  /// * `host_events` receives a copy for task-center or protocol projection.
  ///
  /// # Returns
  ///
  /// A sink that updates the shell transcript before forwarding each event.
  ///
  /// # Errors
  ///
  /// Returns [`BuiltinShellError`] when the submission is no longer active or
  /// targets the desktop frontend instead of the portable executor.
  pub fn portable_event_sink(
    &self,
    submission: &ShellSubmission,
    host_events: CommandEventSink,
  ) -> Result<CommandEventSink, BuiltinShellError> {
    self.validate_active_submission(submission)?;
    if submission.target != ShellCommandTarget::Portable {
      return Err(BuiltinShellError::FrontendCommand { id: submission.id });
    }
    let shell = self.clone();
    let id = submission.id;
    Ok(CommandEventSink::new(move |event| {
      shell.record_command_event(id, event.clone());
      host_events.emit(event);
    }))
  }

  /// Marks a portable command as successfully completed by its host executor.
  pub fn complete_portable(&self, id: ShellCommandId) -> Result<(), BuiltinShellError> {
    self.finish(id, ShellExecutionStatus::Succeeded, ShellTranscriptContent::Completed)
  }

  /// Marks a portable command as cooperatively cancelled by its host executor.
  pub fn cancel_portable(&self, id: ShellCommandId) -> Result<(), BuiltinShellError> {
    self.finish(id, ShellExecutionStatus::Cancelled, ShellTranscriptContent::Cancelled)
  }

  /// Marks a frontend-routed command as successfully completed.
  pub fn complete_frontend(&self, id: ShellCommandId) -> Result<(), BuiltinShellError> {
    self.finish(id, ShellExecutionStatus::Succeeded, ShellTranscriptContent::Completed)
  }

  /// Marks a frontend-routed command as failed with a user-facing message.
  pub fn fail_frontend(&self, id: ShellCommandId, message: impl Into<String>) -> Result<(), BuiltinShellError> {
    self.fail_submission(id, message)
  }

  /// Marks an active submission as failed before or during host routing.
  pub fn fail_submission(&self, id: ShellCommandId, message: impl Into<String>) -> Result<(), BuiltinShellError> {
    self.finish(
      id,
      ShellExecutionStatus::Failed,
      ShellTranscriptContent::Failed(message.into()),
    )
  }

  /// Requests cooperative cancellation of the active command.
  pub fn cancel_active(&self) -> Result<Option<ShellCommandId>, BuiltinShellError> {
    let id = {
      let mut state = self.lock_state()?;
      let Some(active) = state.active.as_mut() else {
        return Ok(None);
      };
      active.cancellation.cancel();
      active.status = ShellExecutionStatus::Cancelling;
      let id = active.id;
      if let Some(record) = state.executions.iter_mut().find(|record| record.id == id) {
        record.status = ShellExecutionStatus::Cancelling;
      }
      id
    };
    self.events.emit(ShellEvent::StateChanged {
      id,
      status: ShellExecutionStatus::Cancelling,
    });
    Ok(Some(id))
  }

  /// Selects the previous command-line history entry.
  pub fn previous_history(&self, current_input: &str) -> Result<Option<String>, BuiltinShellError> {
    let mut state = self.lock_state()?;
    if state.history.is_empty() {
      return Ok(None);
    }
    let index = match state.history_cursor {
      Some(index) => index.saturating_sub(1),
      None => {
        state.navigation_draft = current_input.to_owned();
        state.history.len() - 1
      }
    };
    state.history_cursor = Some(index);
    Ok(state.history.get(index).cloned())
  }

  /// Selects the next command-line history entry or restores the input draft.
  pub fn next_history(&self) -> Result<Option<String>, BuiltinShellError> {
    let mut state = self.lock_state()?;
    let Some(index) = state.history_cursor else {
      return Ok(None);
    };
    if index + 1 < state.history.len() {
      let next = index + 1;
      state.history_cursor = Some(next);
      return Ok(state.history.get(next).cloned());
    }
    state.history_cursor = None;
    Ok(Some(state.navigation_draft.clone()))
  }

  /// Returns full replacement lines matching the unfinished shell token.
  ///
  /// # Parameters
  ///
  /// * `input` is the current editable command line up to the caret.
  ///
  /// # Returns
  ///
  /// Sorted, de-duplicated replacement lines, or an empty list when the input
  /// is not a completable prefix.
  ///
  /// # Notes
  ///
  /// Completion is derived from the built-in grammar alone and never reads
  /// session state, so it stays an associated function rather than a method.
  pub fn complete(input: &str) -> Vec<String> {
    crate::grammar::complete_builtin_shell_line(input)
  }

  /// Returns a presentation-safe copy of the current session state.
  pub fn snapshot(&self) -> Result<BuiltinShellSnapshot, BuiltinShellError> {
    let state = self.lock_state()?;
    Ok(BuiltinShellSnapshot {
      working_directory: state.context.working_directory.clone(),
      active: state.active.as_ref().map(|active| ShellActiveCommand {
        id: active.id,
        command_id: active.command_id,
        target: active.target,
        status: active.status,
      }),
      executions: state.executions.clone(),
      transcript: state.transcript.clone(),
    })
  }

  /// Validates that a prepared submission still owns the foreground slot.
  fn validate_active_submission(&self, submission: &ShellSubmission) -> Result<(), BuiltinShellError> {
    let state = self.lock_state()?;
    match state.active.as_ref() {
      Some(active) if active.id == submission.id => Ok(()),
      _ => Err(BuiltinShellError::InactiveCommand { id: submission.id }),
    }
  }

  /// Returns whether cancellation was requested for the active submission.
  fn active_is_cancelled(&self, id: ShellCommandId) -> Result<bool, BuiltinShellError> {
    let state = self.lock_state()?;
    match state.active.as_ref() {
      Some(active) if active.id == id => Ok(active.cancellation.is_cancelled()),
      _ => Err(BuiltinShellError::InactiveCommand { id }),
    }
  }

  /// Records one frontend-neutral event without holding the lock during notification.
  fn record_command_event(&self, id: ShellCommandId, event: CommandExecutionEvent) {
    let recorded = self
      .lock_state()
      .map(|mut state| {
        if !matches!(state.active.as_ref(), Some(active) if active.id == id) {
          return false;
        }
        match &event {
          CommandExecutionEvent::Progress(progress) => update_progress(&mut state.transcript, id, progress.clone()),
          CommandExecutionEvent::Message(message) => state.transcript.push(ShellTranscriptEntry {
            command_id: id,
            content: ShellTranscriptContent::Message(message.clone()),
          }),
          CommandExecutionEvent::Artifact(artifact) => state.transcript.push(ShellTranscriptEntry {
            command_id: id,
            content: ShellTranscriptContent::Artifact(artifact.clone()),
          }),
          CommandExecutionEvent::Started { .. } | CommandExecutionEvent::Completed { .. } => {}
        }
        true
      })
      .unwrap_or(false);
    if recorded {
      self.events.emit(ShellEvent::CommandExecution { id, event });
    }
  }

  /// Moves the active command into a terminal state and appends its marker.
  fn finish(
    &self,
    id: ShellCommandId,
    status: ShellExecutionStatus,
    content: ShellTranscriptContent,
  ) -> Result<(), BuiltinShellError> {
    {
      let mut state = self.lock_state()?;
      if !matches!(state.active.as_ref(), Some(active) if active.id == id) {
        return Err(BuiltinShellError::InactiveCommand { id });
      }
      if let Some(record) = state.executions.iter_mut().find(|record| record.id == id) {
        record.status = status;
      }
      state.transcript.push(ShellTranscriptEntry {
        command_id: id,
        content,
      });
      state.active = None;
    }
    self.events.emit(ShellEvent::StateChanged { id, status });
    Ok(())
  }

  /// Locks the shared session state and maps mutex poisoning into a typed error.
  fn lock_state(&self) -> Result<MutexGuard<'_, ShellState>, BuiltinShellError> {
    self.state.lock().map_err(|_| BuiltinShellError::StateUnavailable)
  }

  /// Retains a rejected interactive line for conventional shell history.
  fn remember_unsubmitted_input(&self, input: String) -> Result<(), BuiltinShellError> {
    let mut state = self.lock_state()?;
    state.history.push(input);
    state.history_cursor = None;
    state.navigation_draft.clear();
    Ok(())
  }
}

/// Replaces the active progress row so frequent updates do not grow scrollback.
fn update_progress(transcript: &mut Vec<ShellTranscriptEntry>, id: ShellCommandId, progress: CommandProgress) {
  if let Some(entry) = transcript
    .iter_mut()
    .rev()
    .find(|entry| entry.command_id == id && matches!(entry.content, ShellTranscriptContent::Progress(_)))
  {
    entry.content = ShellTranscriptContent::Progress(progress);
  } else {
    transcript.push(ShellTranscriptEntry {
      command_id: id,
      content: ShellTranscriptContent::Progress(progress),
    });
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Mutex;

  use chitin_command::CommandOutputFormat;
  use chitin_databases::{ClientConfig, providers::rcsb::StructureFormat};

  use super::*;

  #[test]
  fn submit_should_route_portable_and_frontend_commands() -> Result<(), BuiltinShellError> {
    let portable_shell = BuiltinShell::new(".");
    let portable = portable_shell.submit("structure validate model.pdb")?.into_command()?;
    let frontend_shell = BuiltinShell::new(".");
    let frontend = frontend_shell.submit("tab.close")?.into_command()?;

    assert_eq!(portable.target(), ShellCommandTarget::Portable);
    assert_eq!(frontend.target(), ShellCommandTarget::Frontend);
    Ok(())
  }

  #[test]
  fn submit_should_reject_a_second_foreground_command() -> Result<(), BuiltinShellError> {
    let shell = BuiltinShell::new(".");
    let first = shell.submit("tab.close")?.into_command()?;

    let result = shell.submit("tab.focus_next");

    assert!(matches!(result, Err(BuiltinShellError::Busy { active_id }) if active_id == first.id()));
    Ok(())
  }

  #[test]
  fn history_navigation_should_restore_the_current_draft() -> Result<(), BuiltinShellError> {
    let shell = BuiltinShell::new(".");
    let first = shell.submit("tab.close")?.into_command()?;
    shell.complete_frontend(first.id())?;
    let second = shell.submit("workspace.toggle_workspace")?.into_command()?;
    shell.complete_frontend(second.id())?;

    let newest = shell.previous_history("structure inspect draft.pdb")?;
    let oldest = shell.previous_history("")?;
    let forward = shell.next_history()?;
    let draft = shell.next_history()?;

    assert_eq!(newest.as_deref(), Some("workspace.toggle_workspace"));
    assert_eq!(oldest.as_deref(), Some("tab.close"));
    assert_eq!(forward.as_deref(), Some("workspace.toggle_workspace"));
    assert_eq!(draft.as_deref(), Some("structure inspect draft.pdb"));
    Ok(())
  }

  #[test]
  fn completion_should_not_require_session_state() {
    assert!(BuiltinShell::complete("cl").contains(&"clear".to_owned()));
  }

  #[test]
  fn rejected_input_should_remain_in_history() -> Result<(), BuiltinShellError> {
    let shell = BuiltinShell::new(".");

    assert!(
      shell
        .submit_line("not-a-command", ShellInvocationSource::Interactive)
        .is_err()
    );
    assert_eq!(shell.previous_history("")?, Some("not-a-command".to_owned()));
    Ok(())
  }

  #[test]
  fn help_line_should_return_display_output() -> Result<(), BuiltinShellError> {
    let shell = BuiltinShell::new(".");

    let result = shell.submit("db")?;

    assert!(matches!(result, ShellLineSubmission::Display(help) if help.contains("Usage: chitin db <COMMAND>")));
    Ok(())
  }

  #[test]
  fn help_line_should_not_reserve_the_execution_slot() -> Result<(), BuiltinShellError> {
    let shell = BuiltinShell::new(".");

    let _ = shell.submit("db")?;

    assert!(shell.snapshot()?.active.is_none());
    Ok(())
  }

  #[test]
  fn clear_should_reset_transcript_without_erasing_history() -> Result<(), BuiltinShellError> {
    let shell = BuiltinShell::new(".");
    let submission = shell.submit("tab.close")?.into_command()?;
    shell.complete_frontend(submission.id())?;

    let result = shell.submit("clear")?;
    let snapshot = shell.snapshot()?;

    assert!(matches!(
      result,
      ShellLineSubmission::ShellBuiltin(ShellBuiltinEffect::ClearScrollback)
    ));
    assert!(snapshot.transcript.is_empty());
    assert_eq!(shell.previous_history("")?.as_deref(), Some("clear"));
    Ok(())
  }

  #[test]
  fn cancel_should_update_the_active_command_and_token() -> Result<(), BuiltinShellError> {
    let shell = BuiltinShell::new(".");
    let submission = shell.submit("structure validate model.pdb")?.into_command()?;

    let cancelled = shell.cancel_active()?;
    let snapshot = shell.snapshot()?;

    assert_eq!(cancelled, Some(submission.id()));
    assert!(submission.context().cancellation.is_cancelled());
    assert_eq!(
      snapshot.active.map(|active| active.status),
      Some(ShellExecutionStatus::Cancelling)
    );
    Ok(())
  }

  #[tokio::test]
  async fn portable_execution_should_record_validation_completion() -> Result<(), BuiltinShellError> {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let callback_observed = observed.clone();
    let shell = BuiltinShell::with_event_sink(
      ".",
      ShellEventSink::new(move |event| {
        if let Ok(mut events) = callback_observed.lock() {
          events.push(event);
        }
      }),
    );
    shell.set_standard_input(Some(Arc::from(
      b"ATOM      1  CA  GLY A   1       1.000   2.000   3.000  1.00 20.00           C  \nEND\n".as_slice(),
    )))?;
    let submission = shell.submit("structure validate - --format pdb")?.into_command()?;
    let executor = CommandExecutor::new(ClientConfig::default());

    let result = shell.execute_portable(submission, &executor).await?;
    let snapshot = shell.snapshot()?;

    assert!(matches!(
      result.outcome,
      CommandOutcome::StructureValidation(validation)
        if validation.format == StructureFormat::Pdb
          && validation.output == CommandOutputFormat::Text
          && validation.is_valid()
    ));
    assert!(snapshot.active.is_none());
    assert!(matches!(
      snapshot.executions.as_slice(),
      [ShellExecutionRecord {
        status: ShellExecutionStatus::Succeeded,
        ..
      }]
    ));
    assert!(observed.lock().map(|events| !events.is_empty()).unwrap_or(false));
    Ok(())
  }

  #[test]
  fn frontend_completion_should_append_a_terminal_marker() -> Result<(), BuiltinShellError> {
    let shell = BuiltinShell::new(".");
    let submission = shell.submit("tab.close")?.into_command()?;

    shell.complete_frontend(submission.id())?;
    let snapshot = shell.snapshot()?;

    assert!(snapshot.active.is_none());
    assert!(matches!(
      snapshot.transcript.last(),
      Some(ShellTranscriptEntry {
        content: ShellTranscriptContent::Completed,
        ..
      })
    ));
    Ok(())
  }

  #[test]
  fn typed_agent_submission_should_preserve_source_and_arguments() -> Result<(), BuiltinShellError> {
    let shell = BuiltinShell::new(".");
    let parsed = parse_builtin_command_line("structure validate model.pdb --format pdb")?;
    let BuiltinCommandLine::Portable(parsed) = parsed else {
      return Err(BuiltinShellError::DisplayOnly {
        output: "expected an executable structure command".to_owned(),
      });
    };
    let parsed = ChitinCommand::from(parsed);

    let submission = shell.submit_typed(
      "agent validation request",
      parsed.clone(),
      ShellInvocationSource::Agent {
        name: "structure-agent".to_owned(),
      },
    )?;
    let snapshot = shell.snapshot()?;

    assert_eq!(submission.command(), &parsed);
    assert_eq!(
      submission.source(),
      &ShellInvocationSource::Agent {
        name: "structure-agent".to_owned()
      }
    );
    assert!(matches!(
      snapshot.executions.as_slice(),
      [ShellExecutionRecord {
        command,
        source: ShellInvocationSource::Agent { name },
        ..
      }] if command == &parsed && name == "structure-agent"
    ));
    Ok(())
  }
}
