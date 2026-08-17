//! Command-line schema and command-bus routing.

use std::path::PathBuf;

use chitin_command::{ChitinCommand, DatabaseCommand, StructureCommand};
use chitin_databases::providers::rcsb::StructureFormat;
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};

use crate::{download::download_rcsb, error::CliError};

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
  match command {
    StructureSubcommand::Inspect(args) => {
      crate::structure::dispatch(
        ChitinCommand::from(StructureCommand::Inspect),
        args.input,
        Some(args.output),
        args.verbose,
      )
      .await
    }
    StructureSubcommand::Validate(args) => {
      crate::structure::dispatch(
        ChitinCommand::from(StructureCommand::Validate),
        args.input,
        Some(args.output),
        false,
      )
      .await
    }
  }
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
      let command = ChitinCommand::from(DatabaseCommand::DownloadRcsbStructure);
      dispatch_command(command, id, format.structure_format(), output).await
    }
  }
}

/// Routes the CLI request through the shared typed command bus.
///
/// # Parameters
///
/// * `command` identifies the typed command to dispatch.
/// * `raw_id` contains one or more comma-separated PDB identifiers.
/// * `format` selects the structure file format.
/// * `output` optionally overrides the generated output path.
///
/// # Returns
///
/// Returns `Ok(())` after the command completes successfully.
///
/// # Errors
///
/// Returns [`CliError`] when the command is unsupported or the RCSB download
/// fails.
async fn dispatch_command(
  command: ChitinCommand,
  raw_id: String,
  format: StructureFormat,
  output: Option<PathBuf>,
) -> Result<(), CliError> {
  match command {
    ChitinCommand::Database(DatabaseCommand::DownloadRcsbStructure) => download_rcsb(raw_id, format, output).await,
    other => Err(CliError::UnsupportedCommand(other.id())),
  }
}
