//! Process CLI schema and command-bus routing.

use std::{io::Read, path::PathBuf};

use chitin_command::{
  CommandExecutionContext, CommandExecutor, CommandReportStatus, DatabaseCommand, PortableCommand, PortableCommandArgs,
  RcsbDownloadArguments, StructureCommand,
};
use chitin_databases::ClientConfig;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};

use crate::{download::terminal_event_sink, error::CliError};

/// Root command parsed by the `chitin` binary.
#[derive(Debug, Parser)]
#[command(name = "chitin", version, about = "Chitin structural biology tools")]
pub(crate) struct Cli {
  /// Selected command-line workflow.
  #[command(subcommand)]
  pub(crate) command: CliCommand,
}

/// CLI-specific actions composed with the shared portable command tree.
#[derive(Debug, Subcommand)]
pub(crate) enum CliCommand {
  /// Execute a portable Chitin workflow.
  #[command(flatten)]
  Portable(PortableCommandArgs),
  /// Generate shell completion scripts.
  Completions { shell: Shell },
}

/// Dispatches a parsed CLI workflow.
///
/// # Parameters
///
/// * `command` is the validated top-level CLI action.
///
/// # Returns
///
/// The report status after execution or completion generation finishes.
///
/// # Errors
///
/// Returns [`CliError`] when typed conversion, execution, or output rendering
/// fails.
pub(crate) async fn dispatch(command: CliCommand) -> Result<CommandReportStatus, CliError> {
  match command {
    CliCommand::Completions { shell } => {
      let mut command = Cli::command();
      generate(shell, &mut command, "chitin", &mut std::io::stdout());
      Ok(CommandReportStatus::Succeeded)
    }
    CliCommand::Portable(arguments) => dispatch_command(arguments.into_command()?).await,
  }
}

/// Routes one CLI request through the shared portable command executor.
///
/// # Parameters
///
/// * `command` contains the validated portable command and arguments.
///
/// # Returns
///
/// The rendered command's domain-level completion status.
///
/// # Errors
///
/// Returns [`CliError`] when process context, command execution, or output
/// rendering fails.
async fn dispatch_command(command: PortableCommand) -> Result<CommandReportStatus, CliError> {
  let context = execution_context(&command)?;
  let executor = CommandExecutor::new(ClientConfig::default());
  let outcome = executor.execute(command, context, terminal_event_sink()).await?;
  Ok(crate::structure::render_outcome(&outcome))
}

/// Builds frontend-neutral execution context from the current process state.
///
/// # Parameters
///
/// * `command` identifies which optional process resources are required.
///
/// # Returns
///
/// Working-directory, download-root, and standard-input data for the executor.
fn execution_context(command: &PortableCommand) -> Result<CommandExecutionContext, CliError> {
  let working_directory = std::env::current_dir().map_err(CliError::WorkingDirectory)?;
  let mut context = CommandExecutionContext::new(working_directory);
  if requires_default_download_root(command) {
    context = context.with_default_download_root(home_directory()?.join(".chitin").join("download"));
  }
  if requires_standard_input(command) {
    let mut bytes = Vec::new();
    std::io::stdin()
      .read_to_end(&mut bytes)
      .map_err(CliError::StandardInput)?;
    context = context.with_standard_input(bytes);
  }
  Ok(context)
}

/// Returns whether a database command needs the process download root.
fn requires_default_download_root(command: &PortableCommand) -> bool {
  matches!(
    command,
    PortableCommand::Database(DatabaseCommand::DownloadRcsbStructure(RcsbDownloadArguments {
      output: None,
      ..
    }))
  )
}

/// Returns whether a structure command reads bytes from standard input.
fn requires_standard_input(command: &PortableCommand) -> bool {
  match command {
    PortableCommand::Structure(StructureCommand::Inspect(arguments)) => arguments.input.input.as_os_str() == "-",
    PortableCommand::Structure(StructureCommand::Validate(arguments)) => arguments.input.input.as_os_str() == "-",
    PortableCommand::Database(_) => false,
  }
}

/// Finds the platform home directory used by default downloads.
fn home_directory() -> Result<PathBuf, CliError> {
  std::env::var_os("HOME")
    .or_else(|| std::env::var_os("USERPROFILE"))
    .map(PathBuf::from)
    .ok_or(CliError::HomeDirectory)
}
