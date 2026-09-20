//! Portable database-command execution.

use std::{collections::BTreeSet, path::PathBuf};

use chitin_command::{
  CommandEventSink, CommandExecutionContext, CommandExecutionEvent, CommandMessage, CommandMessageLevel,
  CommandProgress, DatabaseCommand, RcsbDownloadArguments,
};
use chitin_databases::{
  Client,
  providers::rcsb::{RcsbBatchDownloadEvent, RcsbBatchDownloadRequest},
};

use crate::{CommandExecutionError, CommandOutcome};

/// Executes a portable database command through the shared provider client.
///
/// # Parameters
///
/// * `client` owns shared transport, retry, and concurrency policy.
/// * `command` contains validated provider-specific arguments.
/// * `context` supplies path resolution and cancellation resources.
/// * `events` receives frontend-neutral progress and artifacts.
///
/// # Returns
///
/// The typed outcome of the selected database workflow.
pub(super) async fn execute(
  client: &Client,
  command: DatabaseCommand,
  context: &CommandExecutionContext,
  events: &CommandEventSink,
) -> Result<CommandOutcome, CommandExecutionError> {
  match command {
    DatabaseCommand::DownloadRcsbStructure(arguments) => {
      execute_rcsb_download(client, arguments, context, events).await
    }
  }
}

/// Resolves the unique destination paths produced by an RCSB download.
///
/// # Parameters
///
/// * `arguments` contains identifiers, format, and an optional output override.
/// * `context` supplies the working directory and default download root.
///
/// # Returns
///
/// Final artifact paths in input order with duplicate destinations removed.
pub fn resolve_rcsb_download_paths(
  arguments: &RcsbDownloadArguments,
  context: &CommandExecutionContext,
) -> Result<Vec<PathBuf>, CommandExecutionError> {
  let multiple = arguments.ids.len() > 1;
  let mut destinations = BTreeSet::new();
  arguments
    .ids
    .iter()
    .map(|id| {
      let destination = match arguments.output.as_ref() {
        Some(output) => {
          let output = resolve_from_working_directory(output, &context.working_directory);
          if output.is_dir() {
            output.join(arguments.format.filename(id))
          } else if multiple && (output.extension().is_some() || output.exists()) {
            return Err(CommandExecutionError::MultipleOutputFile(output));
          } else if multiple {
            output.join(arguments.format.filename(id))
          } else {
            output
          }
        }
        None => context
          .default_download_root
          .as_ref()
          .ok_or(CommandExecutionError::MissingDownloadRoot {
            command_id: chitin_command::CommandId::DatabaseDownloadRcsbStructure,
          })
          .map(|root| resolve_from_working_directory(root, &context.working_directory))?
          .join(arguments.format.id())
          .join(arguments.format.filename(id)),
      };
      Ok(destination)
    })
    .filter_map(|result| match result {
      Ok(path) if destinations.insert(path.clone()) => Some(Ok(path)),
      Ok(_) => None,
      Err(error) => Some(Err(error)),
    })
    .collect()
}

/// Downloads all unique RCSB artifacts and forwards provider progress.
///
/// # Parameters
///
/// * `client` provides the configured RCSB provider.
/// * `arguments` contains validated identifiers and output preferences.
/// * `context` supplies path defaults and cooperative cancellation.
/// * `events` receives normalized progress, messages, and artifacts.
///
/// # Returns
///
/// A database outcome containing the persisted artifact count.
async fn execute_rcsb_download(
  client: &Client,
  arguments: RcsbDownloadArguments,
  context: &CommandExecutionContext,
  events: &CommandEventSink,
) -> Result<CommandOutcome, CommandExecutionError> {
  let destinations = resolve_rcsb_download_paths(&arguments, context)?;
  let mut seen = BTreeSet::new();
  let requests = arguments
    .ids
    .into_iter()
    .filter(|id| seen.insert(id.clone()))
    .zip(destinations)
    .map(|(id, destination)| RcsbBatchDownloadRequest {
      id,
      format: arguments.format,
      destination,
    })
    .collect::<Vec<_>>();
  let artifact_count = requests.len();
  let event_sink = events.clone();
  client
    .rcsb()
    .download_structures_to_paths_with_cancellation(
      &requests,
      move |event| emit_download_event(&event_sink, event),
      context.cancellation.clone(),
    )
    .await?;
  Ok(CommandOutcome::DatabaseDownload { artifact_count })
}

/// Translates provider-specific download events into the command protocol.
///
/// # Parameters
///
/// * `events` is the frontend observer receiving normalized events.
/// * `event` is the provider-specific update being projected.
///
/// # Returns
///
/// This function returns after synchronously forwarding the event.
fn emit_download_event(events: &CommandEventSink, event: RcsbBatchDownloadEvent) {
  match event {
    RcsbBatchDownloadEvent::Started { index, total, id } => {
      events.emit(CommandExecutionEvent::Progress(CommandProgress {
        completed: 0,
        total: None,
        stage_index: index,
        stage_count: total,
        stage_label: Some(id.to_string()),
      }));
    }
    RcsbBatchDownloadEvent::Progress {
      index,
      total,
      id,
      received,
      total_bytes,
    } => {
      events.emit(CommandExecutionEvent::Progress(CommandProgress {
        completed: received,
        total: total_bytes,
        stage_index: index,
        stage_count: total,
        stage_label: Some(id.to_string()),
      }));
    }
    RcsbBatchDownloadEvent::Completed { id, artifact, .. } => {
      events.emit(CommandExecutionEvent::Message(CommandMessage {
        level: CommandMessageLevel::Info,
        text: format!("downloaded RCSB structure {id}"),
      }));
      events.emit(CommandExecutionEvent::Artifact(artifact));
    }
  }
}

/// Resolves a relative path against the frontend working directory.
fn resolve_from_working_directory(path: &std::path::Path, working_directory: &std::path::Path) -> PathBuf {
  if path.is_absolute() {
    path.to_owned()
  } else {
    working_directory.join(path)
  }
}

#[cfg(test)]
mod tests {
  use chitin_databases::providers::rcsb::{PdbId, StructureFormat};

  use super::*;

  fn id(value: &str) -> Result<PdbId, Box<dyn std::error::Error>> {
    Ok(PdbId::new(value)?)
  }

  #[test]
  fn default_download_paths_should_use_format_directory() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = RcsbDownloadArguments {
      ids: vec![id("4hhb")?],
      format: StructureFormat::Mmcif,
      output: None,
    };
    let context = CommandExecutionContext::new("/workspace").with_default_download_root("/workspace/.chitin/download");

    let paths = resolve_rcsb_download_paths(&arguments, &context)?;

    assert_eq!(paths, [PathBuf::from("/workspace/.chitin/download/mmcif/4HHB.cif")]);
    Ok(())
  }

  #[test]
  fn repeated_ids_should_produce_one_destination() -> Result<(), Box<dyn std::error::Error>> {
    let repeated = id("4hhb")?;
    let arguments = RcsbDownloadArguments {
      ids: vec![repeated.clone(), repeated],
      format: StructureFormat::Pdb,
      output: None,
    };
    let context = CommandExecutionContext::new("/workspace").with_default_download_root("/workspace/.chitin/download");

    let paths = resolve_rcsb_download_paths(&arguments, &context)?;

    assert_eq!(paths.len(), 1);
    Ok(())
  }

  #[test]
  fn existing_directory_should_receive_generated_filename() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::temp_dir();
    let arguments = RcsbDownloadArguments {
      ids: vec![id("1yth")?],
      format: StructureFormat::Pdb,
      output: Some(directory.clone()),
    };
    let context = CommandExecutionContext::new("/workspace");

    let paths = resolve_rcsb_download_paths(&arguments, &context)?;

    assert_eq!(paths, [directory.join("1YTH.pdb")]);
    Ok(())
  }

  #[test]
  fn nonexistent_extensionless_single_output_should_remain_a_file() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = RcsbDownloadArguments {
      ids: vec![id("1yth")?],
      format: StructureFormat::Pdb,
      output: Some(PathBuf::from("custom/structure")),
    };
    let context = CommandExecutionContext::new("/workspace");

    let paths = resolve_rcsb_download_paths(&arguments, &context)?;

    assert_eq!(paths, [PathBuf::from("/workspace/custom/structure")]);
    Ok(())
  }

  #[test]
  fn multiple_downloads_should_reject_an_explicit_file() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = RcsbDownloadArguments {
      ids: vec![id("1yth")?, id("4hhb")?],
      format: StructureFormat::Mmcif,
      output: Some(PathBuf::from("structures/output.cif")),
    };
    let context = CommandExecutionContext::new("/workspace");

    let result = resolve_rcsb_download_paths(&arguments, &context);

    assert!(matches!(result, Err(CommandExecutionError::MultipleOutputFile(_))));
    Ok(())
  }
}
