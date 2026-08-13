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
