#![forbid(unsafe_code)]
//! Frontend-independent execution for portable Chitin commands.
//!
//! This crate performs database and structure workflows without printing to a
//! terminal or accessing GPUI. Frontends supply an execution context, observe
//! structured events, and render the returned outcome in their own language.

mod database;
mod structure;

use std::{
  path::PathBuf,
  sync::{Arc, OnceLock},
};

use chitin_bio::structure::StructureParseResult;
use chitin_command::{
  CommandEventSink, CommandExecutionContext, CommandExecutionEvent, CommandId, CommandOutputFormat, PortableCommand,
};
use chitin_databases::{Client, ClientConfig, TransportError, providers::rcsb::StructureFormat};

pub use database::resolve_rcsb_download_paths;

/// Successful result returned by a portable command.
#[derive(Clone, Debug, PartialEq)]
pub enum CommandOutcome {
  /// One or more RCSB artifacts were downloaded.
  DatabaseDownload {
    /// Number of unique persisted artifacts.
    artifact_count: usize,
  },
  /// A structure was parsed for inspection.
  StructureInspection(Box<StructureInspection>),
  /// A structure was parsed and checked for model validity.
  StructureValidation(StructureValidation),
}

/// Parsed structure data and presentation preferences for inspection output.
#[derive(Clone, Debug, PartialEq)]
pub struct StructureInspection {
  /// User-provided source path, or `-` for standard input.
  pub path: PathBuf,
  /// Resolved source format.
  pub format: StructureFormat,
  /// Number of source bytes parsed.
  pub byte_count: usize,
  /// Shared structure model and recoverable diagnostics.
  pub parsed: StructureParseResult,
  /// Output representation requested by the frontend.
  pub output: CommandOutputFormat,
  /// Whether detailed metadata and diagnostics were requested.
  pub verbose: bool,
}

/// Result of parsing and validating one local structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureValidation {
  /// User-provided source path, or `-` for standard input.
  pub path: PathBuf,
  /// Resolved source format.
  pub format: StructureFormat,
  /// Output representation requested by the frontend.
  pub output: CommandOutputFormat,
  /// Validation failure, or `None` when the structure is valid.
  pub error: Option<String>,
}

impl StructureValidation {
  /// Returns whether parsing and all model invariants succeeded.
  pub fn is_valid(&self) -> bool {
    self.error.is_none()
  }
}

/// Failure while preparing or executing a portable command.
#[derive(Debug, thiserror::Error)]
pub enum CommandExecutionError {
  /// No default download directory was supplied for a command without output.
  #[error("command '{command_id}' requires a default download directory")]
  MissingDownloadRoot {
    /// Command requiring the missing directory.
    command_id: CommandId,
  },
  /// Multiple downloads cannot target one explicit file.
  #[error("--output must be a directory when downloading multiple PDB IDs: {0}")]
  MultipleOutputFile(PathBuf),
  /// Standard input was requested but the frontend supplied no bytes.
  #[error("structure input '-' requires standard-input bytes from the frontend")]
  MissingStandardInput,
  /// The structure format could not be inferred.
  #[error("could not determine structure format for '{0}'; use --format pdb or --format mmcif")]
  UnknownStructureFormat(PathBuf),
  /// A local structure file could not be read.
  #[error("failed to read structure '{path}': {source}")]
  StructureRead {
    /// User-provided structure path.
    path: PathBuf,
    /// Filesystem failure.
    source: std::io::Error,
  },
  /// A structure parser rejected the source bytes.
  #[error("failed to parse structure '{path}': {message}")]
  StructureParse {
    /// User-provided structure path.
    path: PathBuf,
    /// Parser diagnostic.
    message: String,
  },
  /// The shared database client could not be initialized.
  #[error("failed to initialize database client: {0}")]
  DatabaseClient(#[source] TransportError),
  /// An RCSB provider or persistence operation failed.
  #[error("RCSB download failed: {0}")]
  RcsbDownload(#[from] chitin_databases::providers::rcsb::RcsbDownloadError),
}

/// Executor for commands whose behavior is shared by every frontend.
#[derive(Clone)]
pub struct CommandExecutor {
  config: ClientConfig,
  client: Arc<OnceLock<Result<Client, TransportError>>>,
}

impl CommandExecutor {
  /// Creates an executor that initializes its database client on first use.
  pub fn new(config: ClientConfig) -> Self {
    Self {
      config,
      client: Arc::new(OnceLock::new()),
    }
  }

  /// Executes a complete typed command and emits frontend-neutral events.
  ///
  /// # Parameters
  ///
  /// * `command` contains the validated command identity and arguments.
  /// * `context` supplies paths, standard input, and cancellation.
  /// * `events` receives progress, diagnostics, and persisted artifacts.
  ///
  /// # Returns
  ///
  /// A typed outcome that the calling frontend can render or consume.
  ///
  /// # Errors
  ///
  /// Returns [`CommandExecutionError`] when required context is missing,
  /// parsing fails, or provider execution fails.
  pub async fn execute(
    &self,
    command: PortableCommand,
    context: CommandExecutionContext,
    events: CommandEventSink,
  ) -> Result<CommandOutcome, CommandExecutionError> {
    let command_id = command.id();
    events.emit(CommandExecutionEvent::Started { command_id });
    let outcome = match command {
      PortableCommand::Database(command) => {
        database::execute(self.database_client()?, command, &context, &events).await
      }
      PortableCommand::Structure(command) => structure::execute(command, &context),
    }?;
    events.emit(CommandExecutionEvent::Completed { command_id });
    Ok(outcome)
  }

  /// Returns the lazily initialized shared database client.
  fn database_client(&self) -> Result<&Client, CommandExecutionError> {
    match self.client.get_or_init(|| Client::new(self.config.clone())) {
      Ok(client) => Ok(client),
      Err(error) => Err(CommandExecutionError::DatabaseClient(error.clone())),
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{path::PathBuf, sync::Mutex};

  use chitin_command::{CommandOutputFormat, StructureCommand, StructureInputArguments, StructureInspectArguments};

  use super::*;

  #[tokio::test]
  async fn executor_should_emit_lifecycle_events_around_structure_inspection() -> Result<(), CommandExecutionError> {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let callback_observed = observed.clone();
    let events = CommandEventSink::new(move |event| {
      if let Ok(mut observed) = callback_observed.lock() {
        observed.push(event);
      }
    });
    let command = StructureCommand::Inspect(StructureInspectArguments {
      input: StructureInputArguments {
        input: PathBuf::from("-"),
        format: Some(StructureFormat::Pdb),
      },
      output: CommandOutputFormat::Text,
      verbose: false,
    });
    let context = CommandExecutionContext::new(".").with_standard_input(
      b"ATOM      1  CA  GLY A   1       1.000   2.000   3.000  1.00 20.00           C  \nEND\n".to_vec(),
    );
    let executor = CommandExecutor::new(ClientConfig::default());

    let _ = executor.execute(command.into(), context, events).await?;
    let observed = match observed.lock() {
      Ok(observed) => observed.clone(),
      Err(_) => Vec::new(),
    };

    assert!(matches!(
      observed.as_slice(),
      [
        CommandExecutionEvent::Started {
          command_id: CommandId::StructureInspect
        },
        CommandExecutionEvent::Completed {
          command_id: CommandId::StructureInspect
        }
      ]
    ));
    Ok(())
  }
}
