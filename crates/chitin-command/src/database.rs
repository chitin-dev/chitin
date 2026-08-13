/// Database-provider commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DatabaseCommand {
  /// Open the RCSB structure download workflow.
  DownloadRcsbStructure,
}

impl DatabaseCommand {
  /// Returns the stable command identifier.
  pub fn id(&self) -> &'static str {
    match self {
      Self::DownloadRcsbStructure => "database.rcsb.download_structure",
    }
  }
}

/// Returns the database command descriptors.
pub fn command_descriptors() -> Vec<crate::CommandDescriptor> {
  vec![crate::CommandDescriptor {
    id: DatabaseCommand::DownloadRcsbStructure.id(),
    title: "Download RCSB Structure",
    category: crate::CommandCategory::Database,
    keywords: &["pdb", "rcsb", "mmcif", "structure"],
    shortcut: None,
    invocation: crate::CommandInvocationKind::Form,
    command: DatabaseCommand::DownloadRcsbStructure.into(),
  }]
}
