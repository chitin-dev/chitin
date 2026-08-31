//! Application-wide background task execution and state tracking.

use std::{
  collections::BTreeMap,
  future::Future,
  panic::{AssertUnwindSafe, catch_unwind},
  path::PathBuf,
  sync::atomic::{AtomicU64, Ordering},
  sync::{Arc, Mutex},
};

use chitin_databases::{CancellationToken, PersistedArtifact};
use futures_util::FutureExt;
use tokio::{
  runtime::Runtime,
  sync::{broadcast, watch},
};

pub(crate) mod rcsb;

/// Stable identity assigned to one submitted task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(u64);

impl std::fmt::Display for TaskId {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    self.0.fmt(formatter)
  }
}

/// Category and adapter identity for one task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskKind {
  /// An artifact download from an external database provider.
  DatabaseDownload { provider: String },
}

/// Lifecycle state shared by all background tasks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
  /// Accepted by the service but not started yet.
  Queued,
  /// Currently executing.
  Running,
  /// Completed successfully.
  Completed,
  /// Terminated with an error.
  Failed,
  /// Stopped after cooperative cancellation.
  Cancelled,
}

impl TaskState {
  /// Returns whether no more state changes are expected.
  pub fn is_terminal(self) -> bool {
    matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
  }

  /// Checks whether a lifecycle transition preserves the task state machine.
  fn can_transition_to(self, next: Self) -> bool {
    matches!(
      (self, next),
      (Self::Queued, Self::Running | Self::Failed | Self::Cancelled)
        | (Self::Running, Self::Completed | Self::Failed | Self::Cancelled)
    )
  }
}

/// Progress for one stage of a background task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskProgress {
  /// Completed units for the active stage.
  pub completed: u64,
  /// Total units when known.
  pub total: Option<u64>,
  /// One-based active stage index.
  pub stage_index: usize,
  /// Number of stages in the task.
  pub stage_count: usize,
  /// Human-readable active stage label.
  pub stage_label: Option<String>,
}

/// Typed output produced by a background task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskOutput {
  /// A downloaded artifact with checksum and provider provenance.
  PersistedArtifact(Box<PersistedArtifact>),
}

/// Resource protected from duplicate active work.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskTarget {
  /// A filesystem destination.
  File(PathBuf),
  /// A provider or tool resource identity.
  Resource(String),
}

impl std::fmt::Display for TaskTarget {
  fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::File(path) => path.display().fmt(formatter),
      Self::Resource(resource) => resource.fmt(formatter),
    }
  }
}

/// Current durable state of one background task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskSnapshot {
  /// Stable task identity.
  pub id: TaskId,
  /// Adapter category and source.
  pub kind: TaskKind,
  /// Resources protected from duplicate active work.
  pub targets: Vec<TaskTarget>,
  /// Current lifecycle state.
  pub state: TaskState,
  /// Most recent progress update.
  pub progress: Option<TaskProgress>,
  /// Ordered diagnostic messages emitted by the task.
  pub logs: Vec<String>,
  /// Typed outputs produced so far.
  pub outputs: Vec<TaskOutput>,
  /// Terminal failure description, when present.
  pub error: Option<String>,
}

/// Event emitted whenever observable task state changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskEvent {
  /// Task that emitted the event.
  pub id: TaskId,
  /// Typed event payload.
  pub kind: TaskEventKind,
}

/// Observable update emitted by the task center.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskEventKind {
  /// Lifecycle state changed.
  StateChanged {
    /// New lifecycle state.
    state: TaskState,
    /// Failure description for a failed terminal state.
    error: Option<String>,
  },
  /// Progress changed for the active stage.
  Progress(TaskProgress),
  /// A diagnostic message was appended.
  Log(String),
  /// A typed output was produced.
  Output(TaskOutput),
}

/// Failure returned by a submitted task adapter.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
#[error("{message}")]
pub struct TaskFailure {
  message: String,
}

impl TaskFailure {
  /// Creates a failure from a user-facing diagnostic message.
  pub fn new(message: impl Into<String>) -> Self {
    Self {
      message: message.into(),
    }
  }
}

/// Failure to submit or control a task.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum TaskCenterError {
  /// The shared Tokio executor could not be initialized.
  #[error("background task executor is unavailable: {message}")]
  ExecutorUnavailable { message: String },
  /// The shared task registry could not be accessed.
  #[error("background task registry is unavailable")]
  RegistryUnavailable,
  /// The requested task identity is not registered.
  #[error("background task {id} was not found")]
  UnknownTask { id: TaskId },
  /// Cancellation was requested after the task had already finished.
  #[error("background task {id} is already in terminal state {state:?}")]
  AlreadyFinished { id: TaskId, state: TaskState },
  /// An active task already owns the requested target.
  #[error("background task {existing} already targets '{target}'")]
  DuplicateTarget { target: TaskTarget, existing: TaskId },
}

/// Subscription returned to the UI for one submitted task.
pub struct TaskHandle {
  /// Stable identity of the submitted task.
  id: TaskId,
  /// Coalescing snapshot stream for this task.
  updates: watch::Receiver<TaskSnapshot>,
  /// Cancellation source shared with the task adapter.
  cancellation: TaskCancellationHandle,
}

/// Cloneable cooperative cancellation handle for one task.
#[derive(Clone)]
pub struct TaskCancellationHandle {
  /// Task identity associated with this cancellation source.
  id: TaskId,
  /// Cooperative cancellation token observed by the adapter.
  cancellation: CancellationToken,
}

impl TaskCancellationHandle {
  /// Returns the task controlled by this handle.
  pub fn id(&self) -> TaskId {
    self.id
  }

  /// Requests cooperative cancellation.
  pub fn cancel(&self) {
    self.cancellation.cancel();
  }
}

impl TaskHandle {
  /// Returns the stable identity of this task.
  pub fn id(&self) -> TaskId {
    self.id
  }

  /// Returns a cloneable cancellation handle.
  pub fn cancellation_handle(&self) -> TaskCancellationHandle {
    self.cancellation.clone()
  }

  /// Returns the latest available snapshot without waiting.
  pub fn latest(&self) -> TaskSnapshot {
    self.updates.borrow().clone()
  }

  /// Waits for and returns the next coalesced snapshot.
  pub async fn changed(&mut self) -> Option<TaskSnapshot> {
    self.updates.changed().await.ok()?;
    Some(self.updates.borrow_and_update().clone())
  }
}

/// Context supplied to a running task adapter.
#[derive(Clone)]
pub struct TaskContext {
  /// Registry reporter for progress, logs, outputs, and state changes.
  reporter: TaskReporter,
  /// Cooperative cancellation token for the running adapter.
  cancellation: CancellationToken,
}

impl TaskContext {
  /// Reports the most recent progress for the active stage.
  pub fn report_progress(&self, progress: TaskProgress) {
    self
      .reporter
      .update(|snapshot| snapshot.progress = Some(progress.clone()));
    self.reporter.publish(TaskEventKind::Progress(progress));
  }

  /// Appends one diagnostic message to the task log.
  pub fn log(&self, message: impl Into<String>) {
    let message = message.into();
    self.reporter.update(|snapshot| snapshot.logs.push(message.clone()));
    self.reporter.publish(TaskEventKind::Log(message));
  }

  /// Appends one typed output to the task result set.
  pub fn output(&self, output: TaskOutput) {
    self.reporter.update(|snapshot| snapshot.outputs.push(output.clone()));
    self.reporter.publish(TaskEventKind::Output(output));
  }

  /// Returns the cooperative cancellation token for adapter APIs.
  pub fn cancellation_token(&self) -> CancellationToken {
    self.cancellation.clone()
  }

  /// Returns whether cancellation has been requested.
  pub fn is_cancelled(&self) -> bool {
    self.cancellation.is_cancelled()
  }
}

/// Registry-owned state and notification channel for one task.
struct TaskRecord {
  /// Latest durable snapshot exposed to callers.
  snapshot: TaskSnapshot,
  /// Cancellation source shared with the running adapter.
  cancellation: CancellationToken,
  /// Coalescing update channel for subscribers of this task.
  updates: watch::Sender<TaskSnapshot>,
}

/// Synchronized task records and the application-wide event stream.
struct TaskRegistry {
  /// Active and terminal tasks indexed by their stable identifiers.
  records: Mutex<BTreeMap<TaskId, TaskRecord>>,
  /// Broadcast stream used by global task views and notifications.
  events: broadcast::Sender<TaskEvent>,
}

impl Default for TaskRegistry {
  fn default() -> Self {
    let (events, _) = broadcast::channel(256);
    Self {
      records: Mutex::new(BTreeMap::new()),
      events,
    }
  }
}

#[derive(Clone)]
/// Internal handle used by adapters to update their registry record.
struct TaskReporter {
  /// Identity of the task being updated.
  id: TaskId,
  /// Shared registry containing the task snapshot and event stream.
  registry: Arc<TaskRegistry>,
}

impl TaskReporter {
  /// Applies an in-place snapshot update and notifies per-task subscribers.
  fn update(&self, update: impl FnOnce(&mut TaskSnapshot)) {
    let Ok(mut records) = self.registry.records.lock() else {
      log::error!("background task registry lock was poisoned for task {}", self.id);
      return;
    };
    let Some(record) = records.get_mut(&self.id) else {
      log::error!("background task {} disappeared from the registry", self.id);
      return;
    };
    update(&mut record.snapshot);
    record.updates.send_replace(record.snapshot.clone());
  }

  /// Publishes an event without changing the durable snapshot.
  fn publish(&self, kind: TaskEventKind) {
    let _ = self.registry.events.send(TaskEvent { id: self.id, kind });
  }

  /// Applies a validated lifecycle transition and emits its state event.
  fn transition(&self, state: TaskState, error: Option<String>) {
    let mut changed = false;
    self.update(|snapshot| {
      if !snapshot.state.can_transition_to(state) {
        log::warn!(
          "ignored invalid background task transition: id={}, current={:?}, next={state:?}",
          snapshot.id,
          snapshot.state
        );
        return;
      }
      snapshot.state = state;
      snapshot.error = error.clone();
      changed = true;
    });
    if changed {
      self.publish(TaskEventKind::StateChanged { state, error });
    }
  }
}

/// Runtime creation result retained by the center for graceful submission errors.
enum TaskExecutor {
  /// Shared multi-thread runtime used by all submitted tasks.
  Available(Runtime),
  /// Initialization failure retained for diagnostics.
  Unavailable(String),
}

/// Application-wide executor and registry for background tasks.
pub struct BackgroundTaskCenter {
  /// Shared Tokio runtime used by every submitted adapter.
  executor: TaskExecutor,
  /// Registry retained for the lifetime of the application.
  registry: Arc<TaskRegistry>,
  /// Monotonic source for stable task identifiers.
  next_id: AtomicU64,
}

impl BackgroundTaskCenter {
  /// Creates one shared executor for the lifetime of the desktop application.
  pub fn new() -> Self {
    let executor = match tokio::runtime::Builder::new_multi_thread()
      .enable_all()
      .thread_name("chitin-task")
      .build()
    {
      Ok(runtime) => TaskExecutor::Available(runtime),
      Err(error) => TaskExecutor::Unavailable(error.to_string()),
    };
    Self {
      executor,
      registry: Arc::new(TaskRegistry::default()),
      next_id: AtomicU64::new(1),
    }
  }

  /// Submits an asynchronous adapter task and returns its update subscription.
  ///
  /// Targets are reserved while the task is non-terminal. A submission that
  /// overlaps an active target is rejected with [`TaskCenterError::DuplicateTarget`].
  /// The adapter receives a [`TaskContext`] for progress, logs, outputs, and
  /// cooperative cancellation.
  pub fn submit<Task, TaskFuture>(
    &self,
    kind: TaskKind,
    targets: Vec<TaskTarget>,
    task: Task,
  ) -> Result<TaskHandle, TaskCenterError>
  where
    Task: FnOnce(TaskContext) -> TaskFuture + Send + 'static,
    TaskFuture: Future<Output = Result<(), TaskFailure>> + Send + 'static,
  {
    let runtime = match &self.executor {
      TaskExecutor::Available(runtime) => runtime,
      TaskExecutor::Unavailable(message) => {
        return Err(TaskCenterError::ExecutorUnavailable {
          message: message.clone(),
        });
      }
    };
    let id = TaskId(self.next_id.fetch_add(1, Ordering::Relaxed));
    let cancellation = CancellationToken::new();
    let snapshot = TaskSnapshot {
      id,
      kind,
      targets,
      state: TaskState::Queued,
      progress: None,
      logs: Vec::new(),
      outputs: Vec::new(),
      error: None,
    };
    let (updates, receiver) = watch::channel(snapshot.clone());
    let mut records = self
      .registry
      .records
      .lock()
      .map_err(|_| TaskCenterError::RegistryUnavailable)?;
    for record in records.values().filter(|record| !record.snapshot.state.is_terminal()) {
      if let Some(target) = snapshot
        .targets
        .iter()
        .find(|target| record.snapshot.targets.contains(target))
      {
        return Err(TaskCenterError::DuplicateTarget {
          target: target.clone(),
          existing: record.snapshot.id,
        });
      }
    }
    records.insert(
      id,
      TaskRecord {
        snapshot,
        cancellation: cancellation.clone(),
        updates,
      },
    );
    drop(records);
    let _ = self.registry.events.send(TaskEvent {
      id,
      kind: TaskEventKind::StateChanged {
        state: TaskState::Queued,
        error: None,
      },
    });
    let reporter = TaskReporter {
      id,
      registry: self.registry.clone(),
    };
    let cancellation_handle = TaskCancellationHandle {
      id,
      cancellation: cancellation.clone(),
    };
    runtime.spawn(async move {
      reporter.transition(TaskState::Running, None);
      let context = TaskContext {
        reporter: reporter.clone(),
        cancellation: cancellation.clone(),
      };
      let result = match catch_unwind(AssertUnwindSafe(|| task(context))) {
        Ok(future) => match AssertUnwindSafe(future).catch_unwind().await {
          Ok(result) => result,
          Err(_) => Err(TaskFailure::new("background task panicked while running")),
        },
        Err(_) => Err(TaskFailure::new("background task panicked while starting")),
      };
      if cancellation.is_cancelled() {
        reporter.transition(TaskState::Cancelled, None);
      } else if let Err(error) = result {
        reporter.transition(TaskState::Failed, Some(error.to_string()));
      } else {
        reporter.transition(TaskState::Completed, None);
      }
    });
    Ok(TaskHandle {
      id,
      updates: receiver,
      cancellation: cancellation_handle,
    })
  }

  /// Subscribes to events from all current and future tasks.
  pub fn subscribe(&self) -> broadcast::Receiver<TaskEvent> {
    self.registry.events.subscribe()
  }

  /// Returns snapshots for all active and completed tasks.
  pub fn snapshots(&self) -> Result<Vec<TaskSnapshot>, TaskCenterError> {
    let records = self
      .registry
      .records
      .lock()
      .map_err(|_| TaskCenterError::RegistryUnavailable)?;
    Ok(records.values().map(|record| record.snapshot.clone()).collect())
  }

  /// Returns the latest snapshot for one registered task.
  pub fn snapshot(&self, id: TaskId) -> Result<TaskSnapshot, TaskCenterError> {
    let records = self
      .registry
      .records
      .lock()
      .map_err(|_| TaskCenterError::RegistryUnavailable)?;
    records
      .get(&id)
      .map(|record| record.snapshot.clone())
      .ok_or(TaskCenterError::UnknownTask { id })
  }

  /// Requests cooperative cancellation of a registered task.
  pub fn cancel(&self, id: TaskId) -> Result<(), TaskCenterError> {
    let records = self
      .registry
      .records
      .lock()
      .map_err(|_| TaskCenterError::RegistryUnavailable)?;
    let record = records.get(&id).ok_or(TaskCenterError::UnknownTask { id })?;
    if record.snapshot.state.is_terminal() {
      return Err(TaskCenterError::AlreadyFinished {
        id,
        state: record.snapshot.state,
      });
    }
    let cancellation = record.cancellation.clone();
    drop(records);
    cancellation.cancel();
    Ok(())
  }
}

impl Default for BackgroundTaskCenter {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn wait_for_terminal(handle: &mut TaskHandle) -> TaskSnapshot {
    let runtime = tokio::runtime::Builder::new_current_thread()
      .build()
      .unwrap_or_else(|error| panic!("test runtime should build: {error}"));
    runtime.block_on(async {
      loop {
        let snapshot = handle.latest();
        if snapshot.state.is_terminal() {
          return snapshot;
        }
        if handle.changed().await.is_none() {
          return handle.latest();
        }
      }
    })
  }

  #[test]
  fn submitted_task_should_publish_progress_logs_and_completion() {
    let center = BackgroundTaskCenter::new();
    let mut handle = center
      .submit(
        TaskKind::DatabaseDownload {
          provider: "test".to_string(),
        },
        Vec::new(),
        |context| async move {
          context.log("started");
          context.report_progress(TaskProgress {
            completed: 4,
            total: Some(8),
            stage_index: 1,
            stage_count: 1,
            stage_label: Some("artifact".to_string()),
          });
          Ok(())
        },
      )
      .unwrap_or_else(|error| panic!("task should submit: {error}"));

    let snapshot = wait_for_terminal(&mut handle);

    assert_eq!(snapshot.state, TaskState::Completed);
    assert_eq!(snapshot.logs, ["started"]);
    assert_eq!(snapshot.progress.map(|progress| progress.completed), Some(4));
  }

  #[test]
  fn cancelled_task_should_finish_in_cancelled_state() {
    let center = BackgroundTaskCenter::new();
    let mut handle = center
      .submit(
        TaskKind::DatabaseDownload {
          provider: "test".to_string(),
        },
        Vec::new(),
        |context| async move {
          while !context.is_cancelled() {
            tokio::task::yield_now().await;
          }
          Ok(())
        },
      )
      .unwrap_or_else(|error| panic!("task should submit: {error}"));

    let cancellation = handle.cancellation_handle();
    assert_eq!(cancellation.id(), handle.id());
    cancellation.cancel();
    let snapshot = wait_for_terminal(&mut handle);

    assert_eq!(snapshot.state, TaskState::Cancelled);
  }

  #[test]
  fn failed_task_should_preserve_failure_message() {
    let center = BackgroundTaskCenter::new();
    let mut handle = center
      .submit(
        TaskKind::DatabaseDownload {
          provider: "test".to_string(),
        },
        Vec::new(),
        |_| async move { Err(TaskFailure::new("download failed")) },
      )
      .unwrap_or_else(|error| panic!("task should submit: {error}"));

    let snapshot = wait_for_terminal(&mut handle);

    assert_eq!(snapshot.state, TaskState::Failed);
    assert_eq!(snapshot.error.as_deref(), Some("download failed"));
  }

  #[test]
  fn active_task_should_reject_a_duplicate_file_target() {
    let center = BackgroundTaskCenter::new();
    let target = TaskTarget::File(PathBuf::from("/tmp/chitin-duplicate.cif"));
    let mut first = center
      .submit(
        TaskKind::DatabaseDownload {
          provider: "test".to_string(),
        },
        vec![target.clone()],
        |context| async move {
          while !context.is_cancelled() {
            tokio::task::yield_now().await;
          }
          Ok(())
        },
      )
      .unwrap_or_else(|error| panic!("first task should submit: {error}"));

    let duplicate = center.submit(
      TaskKind::DatabaseDownload {
        provider: "test".to_string(),
      },
      vec![target.clone()],
      |_| async move { Ok(()) },
    );

    assert!(matches!(
      duplicate,
      Err(TaskCenterError::DuplicateTarget {
        target: duplicate_target,
        existing,
      }) if duplicate_target == target && existing == first.id()
    ));
    center
      .cancel(first.id())
      .unwrap_or_else(|error| panic!("first task should cancel: {error}"));
    assert_eq!(wait_for_terminal(&mut first).state, TaskState::Cancelled);
  }

  #[test]
  fn distinct_targets_should_allow_concurrent_submissions() {
    let center = BackgroundTaskCenter::new();
    let mut first = center
      .submit(
        TaskKind::DatabaseDownload {
          provider: "test".to_string(),
        },
        vec![TaskTarget::File(PathBuf::from("/tmp/chitin-first.cif"))],
        |_| async move { Ok(()) },
      )
      .unwrap_or_else(|error| panic!("first task should submit: {error}"));
    let mut second = center
      .submit(
        TaskKind::DatabaseDownload {
          provider: "test".to_string(),
        },
        vec![TaskTarget::File(PathBuf::from("/tmp/chitin-second.cif"))],
        |_| async move { Ok(()) },
      )
      .unwrap_or_else(|error| panic!("second task should submit: {error}"));

    assert_ne!(first.id(), second.id());
    assert_eq!(wait_for_terminal(&mut first).state, TaskState::Completed);
    assert_eq!(wait_for_terminal(&mut second).state, TaskState::Completed);
    assert_eq!(center.snapshots().map(|snapshots| snapshots.len()), Ok(2));
  }

  #[test]
  fn dropping_submission_handle_should_not_cancel_task() {
    let center = BackgroundTaskCenter::new();
    let handle = center
      .submit(
        TaskKind::DatabaseDownload {
          provider: "test".to_string(),
        },
        Vec::new(),
        |_| async move { Ok(()) },
      )
      .unwrap_or_else(|error| panic!("task should submit: {error}"));
    let id = handle.id();
    drop(handle);

    let runtime = tokio::runtime::Builder::new_current_thread()
      .build()
      .unwrap_or_else(|error| panic!("test runtime should build: {error}"));
    let snapshot = runtime.block_on(async {
      loop {
        let snapshot = center
          .snapshot(id)
          .unwrap_or_else(|error| panic!("task should remain registered: {error}"));
        if snapshot.state.is_terminal() {
          return snapshot;
        }
        tokio::task::yield_now().await;
      }
    });

    assert_eq!(snapshot.state, TaskState::Completed);
  }

  #[test]
  fn subscriber_should_receive_lifecycle_events() {
    let center = BackgroundTaskCenter::new();
    let mut events = center.subscribe();
    let mut handle = center
      .submit(
        TaskKind::DatabaseDownload {
          provider: "test".to_string(),
        },
        Vec::new(),
        |_| async move { Ok(()) },
      )
      .unwrap_or_else(|error| panic!("task should submit: {error}"));
    let _ = wait_for_terminal(&mut handle);
    let runtime = tokio::runtime::Builder::new_current_thread()
      .build()
      .unwrap_or_else(|error| panic!("test runtime should build: {error}"));

    let states = runtime.block_on(async {
      let mut states = Vec::new();
      loop {
        let event = events
          .recv()
          .await
          .unwrap_or_else(|error| panic!("task event should be received: {error}"));
        if let TaskEventKind::StateChanged { state, .. } = event.kind {
          states.push(state);
          if state.is_terminal() {
            return states;
          }
        }
      }
    });

    assert_eq!(states, [TaskState::Queued, TaskState::Running, TaskState::Completed]);
  }
}
