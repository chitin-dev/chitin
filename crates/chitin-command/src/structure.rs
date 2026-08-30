/// Structure parsing and inspection commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructureCommand {
  /// Print a parsed structure summary.
  Inspect,
  /// Parse a structure and verify its cross-table invariants.
  Validate,
}

impl StructureCommand {
  /// Returns the stable command identifier.
  pub fn id(&self) -> &'static str {
    match self {
      Self::Inspect => "structure.inspect",
      Self::Validate => "structure.validate",
    }
  }
}
