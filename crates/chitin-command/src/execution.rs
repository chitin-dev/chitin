//! Frontend-neutral execution context and observable command events.

use std::{path::PathBuf, sync::Arc};

use chitin_databases::{CancellationToken, PersistedArtifact};

use crate::CommandId;

/// Runtime resources supplied by the frontend executing a command.
#[derive(Clone, Debug)]
pub struct CommandExecutionContext {
  /// Directory used to resolve relative input and output paths.
  pub working_directory: PathBuf,
  /// Active project root when the command runs inside a workspace.
  pub workspace_root: Option<PathBuf>,
  /// Default `.chitin/download` directory for downloads without `--output`.
  pub default_download_root: Option<PathBuf>,
  /// Bytes supplied for an input path of `-`.
  pub standard_input: Option<Arc<[u8]>>,
  /// Cooperative cancellation shared with the frontend task owner.
  pub cancellation: CancellationToken,
}

impl CommandExecutionContext {
  /// Creates an execution context rooted at the supplied working directory.
  pub fn new(working_directory: impl Into<PathBuf>) -> Self {
    Self {
      working_directory: working_directory.into(),
      workspace_root: None,
      default_download_root: None,
      standard_input: None,
      cancellation: CancellationToken::new(),
    }
  }

  /// Sets the active workspace root.
  pub fn with_workspace_root(mut self, workspace_root: impl Into<PathBuf>) -> Self {
    self.workspace_root = Some(workspace_root.into());
    self
  }

  /// Sets the default directory containing format-specific download folders.
  pub fn with_default_download_root(mut self, root: impl Into<PathBuf>) -> Self {
    self.default_download_root = Some(root.into());
    self
  }

  /// Supplies standard-input bytes for commands whose input path is `-`.
  pub fn with_standard_input(mut self, bytes: impl Into<Arc<[u8]>>) -> Self {
    self.standard_input = Some(bytes.into());
    self
  }

  /// Replaces the cooperative cancellation token.
  pub fn with_cancellation(mut self, cancellation: CancellationToken) -> Self {
    self.cancellation = cancellation;
    self
  }
}

/// Progress for the active stage of a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandProgress {
  /// Completed units for the active stage.
  pub completed: u64,
  /// Total units when the executor can determine them.
  pub total: Option<u64>,
  /// One-based active stage index.
  pub stage_index: usize,
  /// Number of stages in the command.
  pub stage_count: usize,
  /// Human-readable active stage label.
  pub stage_label: Option<String>,
}

/// Severity attached to a command diagnostic message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandMessageLevel {
  /// Normal execution information.
  Info,
  /// Recoverable condition worth presenting to the user.
  Warning,
}

/// Structured diagnostic emitted during command execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandMessage {
  /// Diagnostic severity.
  pub level: CommandMessageLevel,
  /// User-facing message without terminal or GPUI styling.
  pub text: String,
}

/// Observable event emitted by the frontend-independent executor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandExecutionEvent {
  /// Command execution has started.
  Started {
    /// Stable identity of the executing command.
    command_id: CommandId,
  },
  /// Progress changed for the active stage.
  Progress(CommandProgress),
  /// A diagnostic message was produced.
  Message(CommandMessage),
  /// A downloaded artifact was durably persisted.
  Artifact(Box<PersistedArtifact>),
  /// Command execution completed successfully.
  Completed {
    /// Stable identity of the completed command.
    command_id: CommandId,
  },
}

/// Cloneable event callback shared with asynchronous provider operations.
#[derive(Clone)]
pub struct CommandEventSink {
  callback: Arc<dyn Fn(CommandExecutionEvent) + Send + Sync>,
}

impl CommandEventSink {
  /// Creates an event sink from a thread-safe callback.
  pub fn new(callback: impl Fn(CommandExecutionEvent) + Send + Sync + 'static) -> Self {
    Self {
      callback: Arc::new(callback),
    }
  }

  /// Emits one event to the frontend adapter.
  pub fn emit(&self, event: CommandExecutionEvent) {
    (self.callback)(event);
  }

  /// Creates a sink that deliberately discards all events.
  pub fn silent() -> Self {
    Self::new(|_| {})
  }
}
