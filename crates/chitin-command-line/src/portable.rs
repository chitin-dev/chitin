//! Portable command fragments shared by CLI-style frontends.

use std::path::PathBuf;

use chitin_command::{
  CommandOutputFormat, DatabaseCommand, PortableCommand, RcsbDownloadArguments, StructureCommand,
  StructureInputArguments, StructureInspectArguments, StructureValidateArguments,
};
use chitin_databases::providers::rcsb::{PdbId, PdbIdListError, StructureFormat};
use clap::{Args, Subcommand, ValueEnum};

/// Portable command tree embedded by the process CLI and built-in shell.
#[derive(Debug, Subcommand)]
pub enum PortableCommandArgs {
  /// Work with external biological databases.
  #[command(name = "db", visible_alias = "databases")]
  Database(DatabaseCommandArgs),
  /// Inspect or validate a local PDB/mmCIF structure file.
  Structure(StructureCommandArgs),
}

impl PortableCommandArgs {
  /// Converts validated command-line arguments into a portable typed command.
  pub fn into_command(self) -> Result<PortableCommand, PortableCommandLineError> {
    match self {
      Self::Database(arguments) => arguments.into_command(),
      Self::Structure(arguments) => Ok(arguments.into_command()),
    }
  }
}

/// Command-line conversion failure after Clap has validated the grammar.
#[derive(Debug, thiserror::Error)]
pub enum PortableCommandLineError {
  /// A comma-separated identifier list contains an invalid element.
  #[error(transparent)]
  InvalidPdbIds(#[from] PdbIdListError),
}

/// Database-provider command group.
#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct DatabaseCommandArgs {
  #[command(subcommand)]
  command: DatabaseSubcommand,
}

impl DatabaseCommandArgs {
  /// Converts the selected database workflow into a portable command.
  fn into_command(self) -> Result<PortableCommand, PortableCommandLineError> {
    match self.command {
      DatabaseSubcommand::Rcsb(arguments) => arguments.into_command(),
    }
  }
}

/// Supported database providers.
#[derive(Debug, Subcommand)]
enum DatabaseSubcommand {
  /// Download an RCSB structure file.
  Rcsb(RcsbCommandArgs),
}

/// RCSB command group.
#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
struct RcsbCommandArgs {
  #[command(subcommand)]
  command: RcsbSubcommand,
}

impl RcsbCommandArgs {
  /// Converts the selected RCSB workflow into a portable command.
  fn into_command(self) -> Result<PortableCommand, PortableCommandLineError> {
    match self.command {
      RcsbSubcommand::Download { id, format, output } => Ok(
        DatabaseCommand::DownloadRcsbStructure(RcsbDownloadArguments {
          ids: PdbId::parse_many(&id)?,
          format: format.into(),
          output,
        })
        .into(),
      ),
    }
  }
}

/// RCSB provider workflows.
#[derive(Debug, Subcommand)]
enum RcsbSubcommand {
  /// Download a PDB or mmCIF structure.
  Download {
    /// Comma-separated list of four-character PDB identifiers, such as 4HHB,1YTH.
    #[arg(long, value_name = "PDB_ID")]
    id: String,
    /// Structure format to download.
    #[arg(long, value_enum, default_value_t = StructureFormatArg::Pdb)]
    format: StructureFormatArg,
    /// Output file or directory. Existing directories receive a generated filename.
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,
  },
}

/// Structure command group.
#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct StructureCommandArgs {
  #[command(subcommand)]
  command: StructureSubcommand,
}

impl StructureCommandArgs {
  /// Converts the selected structure workflow into a portable command.
  fn into_command(self) -> PortableCommand {
    match self.command {
      StructureSubcommand::Inspect(arguments) => StructureCommand::Inspect(StructureInspectArguments {
        input: arguments.input.into_command_arguments(),
        output: arguments.output.into(),
        verbose: arguments.verbose,
      })
      .into(),
      StructureSubcommand::Validate(arguments) => StructureCommand::Validate(StructureValidateArguments {
        input: arguments.input.into_command_arguments(),
        output: arguments.output.into(),
      })
      .into(),
    }
  }
}

/// Structure parsing workflows.
#[derive(Debug, Subcommand)]
enum StructureSubcommand {
  /// Print a human-readable or JSON structure summary.
  Inspect(StructureInspectArgs),
  /// Parse a structure and verify its indexed model invariants.
  Validate(StructureValidateArgs),
}

/// Structure inspection arguments.
#[derive(Debug, Args)]
struct StructureInspectArgs {
  #[command(flatten)]
  input: StructureInputArgs,
  /// Output representation.
  #[arg(long, value_enum, default_value_t = OutputFormatArg::Text)]
  output: OutputFormatArg,
  /// Include chains, metadata, assembly, and diagnostics.
  #[arg(long)]
  verbose: bool,
}

/// Structure validation arguments.
#[derive(Debug, Args)]
struct StructureValidateArgs {
  #[command(flatten)]
  input: StructureInputArgs,
  /// Output representation.
  #[arg(long, value_enum, default_value_t = OutputFormatArg::Text)]
  output: OutputFormatArg,
}

/// Input path and optional format shared by structure workflows.
#[derive(Debug, Args)]
struct StructureInputArgs {
  /// Structure file path, or `-` to read stdin.
  #[arg(value_name = "FILE")]
  input: PathBuf,
  /// Input format; inferred from the extension when omitted.
  #[arg(long, value_enum)]
  format: Option<StructureFormatArg>,
}

impl StructureInputArgs {
  /// Converts parsed input options into frontend-independent arguments.
  fn into_command_arguments(self) -> StructureInputArguments {
    StructureInputArguments {
      input: self.input,
      format: self.format.map(StructureFormat::from),
    }
  }
}

/// Command-line spelling for supported structure formats.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum StructureFormatArg {
  /// Legacy PDB format.
  Pdb,
  /// PDBx/mmCIF format.
  #[value(alias = "cif")]
  Mmcif,
}

impl From<StructureFormatArg> for StructureFormat {
  fn from(format: StructureFormatArg) -> Self {
    match format {
      StructureFormatArg::Pdb => Self::Pdb,
      StructureFormatArg::Mmcif => Self::Mmcif,
    }
  }
}

/// Command-line spelling for structure command output formats.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormatArg {
  /// Human-readable output.
  Text,
  /// Stable machine-readable JSON output.
  Json,
}

impl From<OutputFormatArg> for CommandOutputFormat {
  fn from(output: OutputFormatArg) -> Self {
    match output {
      OutputFormatArg::Text => Self::Text,
      OutputFormatArg::Json => Self::Json,
    }
  }
}
