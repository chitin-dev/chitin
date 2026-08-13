//! Command-line schema and command-bus routing.

use std::path::PathBuf;

use chitin_command::{ChitinCommand, DatabaseCommand};
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
  /// Generate shell completion scripts.
  Completions { shell: Shell },
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
    /// Four-character PDB identifier, such as 4HHB.
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
enum FormatArg {
  /// Legacy PDB format.
  Pdb,
  /// PDBx/mmCIF format.
  Mmcif,
}

impl FormatArg {
  /// Converts the CLI value into the shared provider format.
  fn structure_format(self) -> StructureFormat {
    match self {
      Self::Pdb => StructureFormat::Pdb,
      Self::Mmcif => StructureFormat::Mmcif,
    }
  }
}

/// Dispatches a parsed CLI workflow.
pub(crate) async fn dispatch(command: CliCommand) -> Result<(), CliError> {
  match command {
    CliCommand::Completions { shell } => {
      let mut command = Cli::command();
      generate(shell, &mut command, "chitin", &mut std::io::stdout());
      Ok(())
    }
    CliCommand::Database(database) => dispatch_database_command(database.command).await,
  }
}

async fn dispatch_database_command(command: DatabaseSubcommand) -> Result<(), CliError> {
  match command {
    DatabaseSubcommand::Rcsb(command) => dispatch_rcsb_command(command.command).await,
  }
}

async fn dispatch_rcsb_command(command: RcsbSubcommand) -> Result<(), CliError> {
  match command {
    RcsbSubcommand::Download { id, format, output } => {
      let command = ChitinCommand::from(DatabaseCommand::DownloadRcsbStructure);
      dispatch_command(command, id, format.structure_format(), output).await
    }
  }
}

/// Routes the CLI request through the shared typed command bus.
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
