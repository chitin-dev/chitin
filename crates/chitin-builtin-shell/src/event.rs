//! Observable events emitted by a built-in shell session.

use std::sync::Arc;

use chitin_command::CommandExecutionEvent;

use crate::{ShellCommandId, ShellExecutionRecord, ShellExecutionStatus};

/// A state transition emitted by a built-in shell session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellEvent {
  /// A parsed command was accepted as the active submission.
  Submitted(ShellExecutionRecord),
  /// A portable executor emitted progress, a message, or an artifact.
  CommandExecution {
    /// Shell-local identity of the active submission.
    id: ShellCommandId,
    /// Frontend-neutral event emitted by the command executor.
    event: CommandExecutionEvent,
  },
  /// The active command entered a new shell lifecycle state.
  StateChanged {
    /// Shell-local identity of the affected submission.
    id: ShellCommandId,
    /// Current lifecycle state.
    status: ShellExecutionStatus,
  },
}

/// Cloneable callback receiving built-in shell events.
#[derive(Clone)]
pub struct ShellEventSink {
  callback: Arc<dyn Fn(ShellEvent) + Send + Sync>,
}

impl ShellEventSink {
  /// Creates a shell event sink from a thread-safe callback.
  pub fn new(callback: impl Fn(ShellEvent) + Send + Sync + 'static) -> Self {
    Self {
      callback: Arc::new(callback),
    }
  }

  /// Emits one event to the shell host.
  pub fn emit(&self, event: ShellEvent) {
    (self.callback)(event);
  }

  /// Creates a sink that deliberately discards every event.
  pub fn silent() -> Self {
    Self::new(|_| {})
  }
}

impl Default for ShellEventSink {
  fn default() -> Self {
    Self::silent()
  }
}
