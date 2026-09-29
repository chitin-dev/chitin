use std::path::PathBuf;

use chitin_databases::providers::rcsb::StructureFormat;

use crate::CommandId;

/// Output representation requested by a command frontend.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommandOutputFormat {
  /// Human-readable text.
  #[default]
  Text,
  /// Stable machine-readable JSON.
  Json,
}

/// Shared input arguments for local structure commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureInputArguments {
  /// Structure file path, or `-` when the frontend supports standard input.
  pub input: PathBuf,
  /// Explicit input format, or `None` to infer it from the path.
  pub format: Option<StructureFormat>,
}

/// Arguments for printing a parsed structure summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureInspectArguments {
  /// Local structure input.
  pub input: StructureInputArguments,
  /// Output representation.
  pub output: CommandOutputFormat,
  /// Whether detailed metadata and diagnostics should be included.
  pub verbose: bool,
}

/// Arguments for validating a local structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureValidateArguments {
  /// Local structure input.
  pub input: StructureInputArguments,
  /// Output representation.
  pub output: CommandOutputFormat,
}

/// Executable structure parsing and inspection commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructureCommand {
  /// Print a parsed structure summary.
  Inspect(StructureInspectArguments),
  /// Parse a structure and verify its cross-table invariants.
  Validate(StructureValidateArguments),
}

impl StructureCommand {
  /// Returns the stable command identifier.
  pub fn id(&self) -> CommandId {
    match self {
      Self::Inspect(_) => CommandId::StructureInspect,
      Self::Validate(_) => CommandId::StructureValidate,
    }
  }
}
