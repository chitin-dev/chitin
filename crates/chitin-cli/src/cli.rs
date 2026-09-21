//! Command-line schema and command-bus routing.

use std::{io::Read, path::PathBuf};

use chitin_command::{
  CommandExecutionContext, CommandOutputFormat, DatabaseCommand, PortableCommand, RcsbDownloadArguments,
  StructureCommand, StructureInputArguments, StructureInspectArguments, StructureValidateArguments,
};
use chitin_command_runtime::CommandExecutor;
use chitin_databases::{
  ClientConfig,
  providers::rcsb::{PdbId, StructureFormat},
};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};

use crate::{download::terminal_event_sink, error::CliError};

/// Root command parsed by the `chitin` binary.
#[derive(Debug, Parser)]
#[command(name = "chitin", version, about = "Chitin structural biology tools")]
pub(crate) struct Cli {
  /// Selected top-level workflow.
  #[command(subcommand)]
  pub(crate) command: CliCommand,
}

/// Top-level CLI workflows.
#[derive(Debug, Subcommand)]
pub(crate) enum CliCommand {
  /// Work with external biological databases.
  #[command(name = "db", visible_alias = "databases")]
  Database(DatabaseCommandArgs),
  /// Inspect or validate a local PDB/mmCIF structure file.
  Structure(StructureCommandArgs),
  /// Generate shell completion scripts.
  Completions { shell: Shell },
}

#[derive(Debug, Args)]
pub(crate) struct StructureCommandArgs {
  #[command(subcommand)]
  command: StructureSubcommand,
}

#[derive(Debug, Subcommand)]
enum StructureSubcommand {
  /// Print a human-readable or JSON structure summary.
  Inspect(StructureInspectArgs),
  /// Parse a structure and verify its indexed model invariants.
  Validate(StructureValidateArgs),
}

#[derive(Debug, Args)]
struct StructureInspectArgs {
  #[command(flatten)]
  input: StructureInputArgs,
  /// Output representation.
  #[arg(long, value_enum, default_value_t = OutputArg::Text)]
  output: OutputArg,
  /// Include chains, metadata, assembly, and diagnostics.
  #[arg(long)]
  verbose: bool,
}

#[derive(Debug, Args)]
struct StructureValidateArgs {
  #[command(flatten)]
  input: StructureInputArgs,
  /// Output representation.
  #[arg(long, value_enum, default_value_t = OutputArg::Text)]
  output: OutputArg,
}

#[derive(Debug, Args)]
pub(crate) struct StructureInputArgs {
  /// Structure file path, or `-` to read stdin.
  #[arg(value_name = "FILE")]
  pub(crate) input: PathBuf,
  /// Input format; inferred from the extension when omitted.
  #[arg(long, value_enum)]
  pub(crate) format: Option<FormatArg>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum OutputArg {
  /// Human-readable terminal output.
  Text,
  /// Stable machine-readable JSON output.
  Json,
}

#[derive(Debug, Args)]
pub(crate) struct DatabaseCommandArgs {
  #[command(subcommand)]
  command: DatabaseSubcommand,
}

#[derive(Debug, Subcommand)]
enum DatabaseSubcommand {
  /// Download an RCSB structure file.
  Rcsb(RcsbCommandArgs),
}

#[derive(Debug, Args)]
struct RcsbCommandArgs {
  #[command(subcommand)]
  command: RcsbSubcommand,
}

#[derive(Debug, Subcommand)]
enum RcsbSubcommand {
  /// Download a PDB or mmCIF structure.
  Download {
    /// Comma-separated list of four-character PDB identifiers, such as 4HHB,1YTH.
    #[arg(long, value_name = "PDB_ID")]
    id: String,
    /// Structure format to download.
    #[arg(long, value_enum, default_value_t = FormatArg::Pdb)]
    format: FormatArg,
    /// Output file or directory. Existing directories receive a generated filename.
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,
  },
}

/// CLI spelling for the two RCSB structure formats.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum FormatArg {
  /// Legacy PDB format.
  Pdb,
  /// PDBx/mmCIF format.
  Mmcif,
}

impl FormatArg {
  /// Converts the CLI value into the shared provider format.
  pub(crate) fn structure_format(self) -> StructureFormat {
    match self {
      Self::Pdb => StructureFormat::Pdb,
      Self::Mmcif => StructureFormat::Mmcif,
    }
  }
}

impl From<OutputArg> for CommandOutputFormat {
  fn from(output: OutputArg) -> Self {
    match output {
      OutputArg::Text => Self::Text,
      OutputArg::Json => Self::Json,
    }
  }
}

impl StructureInputArgs {
  /// Converts parsed CLI input into frontend-independent command arguments.
  fn into_command_arguments(self) -> StructureInputArguments {
    StructureInputArguments {
      input: self.input,
      format: self.format.map(FormatArg::structure_format),
    }
  }
}

/// Dispatches a parsed CLI workflow.
///
/// # Parameters
///
/// * `command` is the validated top-level CLI command.
///
/// # Returns
///
/// Returns `Ok(())` after the selected workflow completes.
///
/// # Errors
///
/// Returns [`CliError`] when command execution or output generation fails.
pub(crate) async fn dispatch(command: CliCommand) -> Result<(), CliError> {
  match command {
    CliCommand::Completions { shell } => {
      let mut command = Cli::command();
      generate(shell, &mut command, "chitin", &mut std::io::stdout());
      Ok(())
    }
    CliCommand::Database(database) => dispatch_database_command(database.command).await,
    CliCommand::Structure(structure) => dispatch_structure_command(structure.command).await,
  }
}

/// Dispatches a structure command through the shared command bus.
async fn dispatch_structure_command(command: StructureSubcommand) -> Result<(), CliError> {
  let command = match command {
    StructureSubcommand::Inspect(args) => StructureCommand::Inspect(StructureInspectArguments {
      input: args.input.into_command_arguments(),
      output: args.output.into(),
      verbose: args.verbose,
    }),
    StructureSubcommand::Validate(args) => StructureCommand::Validate(StructureValidateArguments {
      input: args.input.into_command_arguments(),
      output: args.output.into(),
    }),
  };
  dispatch_command(command.into()).await
}

/// Dispatches a database command to its provider-specific workflow.
///
/// # Parameters
///
/// * `command` is the parsed database subcommand.
///
/// # Returns
///
/// Returns `Ok(())` after the database workflow completes.
///
/// # Errors
///
/// Returns [`CliError`] when the selected workflow fails.
async fn dispatch_database_command(command: DatabaseSubcommand) -> Result<(), CliError> {
  match command {
    DatabaseSubcommand::Rcsb(command) => dispatch_rcsb_command(command.command).await,
  }
}

/// Dispatches an RCSB subcommand to the shared command bus.
///
/// # Parameters
///
/// * `command` is the parsed RCSB subcommand and its download options.
///
/// # Returns
///
/// Returns `Ok(())` after the RCSB workflow completes.
///
/// # Errors
///
/// Returns [`CliError`] when the RCSB download or output resolution fails.
async fn dispatch_rcsb_command(command: RcsbSubcommand) -> Result<(), CliError> {
  match command {
    RcsbSubcommand::Download { id, format, output } => {
      let command = DatabaseCommand::DownloadRcsbStructure(RcsbDownloadArguments {
        ids: PdbId::parse_many(&id)?,
        format: format.structure_format(),
        output,
      });
      dispatch_command(command.into()).await
    }
  }
}

/// Routes the CLI request through the shared typed command bus.
///
/// # Parameters
///
/// * `command` identifies the typed command to dispatch.
/// # Returns
///
/// Returns `Ok(())` after the command completes successfully.
///
/// # Errors
///
/// Returns [`CliError`] when the command is unsupported or the RCSB download
/// fails.
async fn dispatch_command(command: PortableCommand) -> Result<(), CliError> {
  let context = execution_context(&command)?;
  let executor = CommandExecutor::new(ClientConfig::default());
  let outcome = executor.execute(command, context, terminal_event_sink()).await?;
  crate::structure::render_outcome(outcome)
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

/// Returns whether a database command needs the process default download root.
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
    _ => false,
  }
}

/// Finds the platform home directory used by default downloads.
fn home_directory() -> Result<PathBuf, CliError> {
  std::env::var_os("HOME")
    .or_else(|| std::env::var_os("USERPROFILE"))
    .map(PathBuf::from)
    .ok_or(CliError::HomeDirectory)
}
