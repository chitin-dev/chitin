use std::path::PathBuf;

use chitin_databases::providers::rcsb::{PdbId, StructureFormat};

use crate::CommandId;

/// Arguments for downloading one or more structures from RCSB.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RcsbDownloadArguments {
  /// Canonical RCSB identifiers to download in input order.
  pub ids: Vec<PdbId>,
  /// Structure file format requested from RCSB.
  pub format: StructureFormat,
  /// Optional output file or directory override.
  pub output: Option<PathBuf>,
}

/// Executable database-provider commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatabaseCommand {
  /// Download one or more RCSB structure files.
  DownloadRcsbStructure(RcsbDownloadArguments),
}

impl DatabaseCommand {
  /// Returns the stable command identifier.
  pub fn id(&self) -> CommandId {
    match self {
      Self::DownloadRcsbStructure(_) => CommandId::DatabaseDownloadRcsbStructure,
    }
  }
}

/// Returns the database command registrations.
pub fn command_registrations() -> Vec<crate::CommandRegistration> {
  vec![crate::CommandRegistration {
    descriptor: crate::CommandDescriptor {
      id: CommandId::DatabaseDownloadRcsbStructure,
      title: "Download RCSB Structure",
      requires_arguments: true,
    },
    category: crate::CommandCategory::Database,
    keywords: &["pdb", "rcsb", "mmcif", "structure"],
    shortcut: None,
  }]
}
