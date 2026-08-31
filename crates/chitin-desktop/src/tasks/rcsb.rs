//! RCSB database adapter for the application task center.

use std::collections::BTreeSet;

use chitin_databases::{
  Client, ClientConfig,
  providers::rcsb::{RcsbBatchDownloadEvent, RcsbBatchDownloadRequest, RcsbDownloadError, RcsbError},
};

use super::{
  BackgroundTaskCenter, TaskCenterError, TaskContext, TaskFailure, TaskHandle, TaskKind, TaskOutput, TaskProgress,
  TaskTarget,
};

/// Submits a batch of RCSB downloads, coalescing repeated destination paths.
pub(crate) fn submit_downloads(
  center: &BackgroundTaskCenter,
  mut requests: Vec<RcsbBatchDownloadRequest>,
) -> Result<TaskHandle, TaskCenterError> {
  let mut destinations = BTreeSet::new();
  requests.retain(|request| destinations.insert(request.destination.clone()));
  let targets = requests
    .iter()
    .map(|request| TaskTarget::File(request.destination.clone()))
    .collect();
  center.submit(
    TaskKind::DatabaseDownload {
      provider: "RCSB".to_string(),
    },
    targets,
    move |context| run_downloads(context, requests),
  )
}

/// Runs the provider batch inside the task center's shared Tokio runtime.
async fn run_downloads(context: TaskContext, requests: Vec<RcsbBatchDownloadRequest>) -> Result<(), TaskFailure> {
  context.log(format!("downloading {} RCSB structure artifacts", requests.len()));
  let client = Client::new(ClientConfig::default())
    .map_err(|error| TaskFailure::new(RcsbDownloadError::Provider(RcsbError::Transport(error)).to_string()))?;
  let event_context = context.clone();
  client
    .rcsb()
    .download_structures_to_paths_with_cancellation(
      &requests,
      move |event| report_download_event(&event_context, event),
      context.cancellation_token(),
    )
    .await
    .map_err(|error| TaskFailure::new(error.to_string()))?;
  Ok(())
}

/// Translates provider-specific progress into task-center events.
fn report_download_event(context: &TaskContext, event: RcsbBatchDownloadEvent) {
  match event {
    RcsbBatchDownloadEvent::Started { index, total, id } => {
      context.report_progress(TaskProgress {
        completed: 0,
        total: None,
        stage_index: index,
        stage_count: total,
        stage_label: Some(id.to_string()),
      });
    }
    RcsbBatchDownloadEvent::Progress {
      index,
      total,
      id,
      received,
      total_bytes,
    } => {
      context.report_progress(TaskProgress {
        completed: received,
        total: total_bytes,
        stage_index: index,
        stage_count: total,
        stage_label: Some(id.to_string()),
      });
    }
    RcsbBatchDownloadEvent::Completed { id, artifact, .. } => {
      context.log(format!("persisted RCSB structure {id} to {}", artifact.path.display()));
      context.output(TaskOutput::PersistedArtifact(artifact));
    }
  }
}
