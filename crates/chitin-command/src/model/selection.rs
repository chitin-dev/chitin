//! Frontend selection requests with validated scientific specifications.

use chitin_bio::structure::selection::AtomSelection;
pub use chitin_bio::structure::selection::AtomSpec;

/// How matched atoms change the current selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionMode {
  /// Replace the current selection.
  Replace,
  /// Add matching atoms.
  Add,
  /// Remove matching atoms.
  Subtract,
  /// Toggle matching atoms as one group (mouse residue selection).
  Toggle,
}

impl SelectionMode {
  /// Applies one group of matches against the original selection.
  pub fn apply(self, current: &AtomSelection, matched: AtomSelection) -> AtomSelection {
    match self {
      Self::Replace => matched,
      Self::Add => current.union(&matched).copied().collect(),
      Self::Subtract => current.difference(&matched).copied().collect(),
      Self::Toggle if matched.is_subset(current) => current.difference(&matched).copied().collect(),
      Self::Toggle => current.union(&matched).copied().collect(),
    }
  }
}

/// One selection operation applied within a stable rendering context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectionCommand {
  /// Resolve a validated author-ID specification.
  Match { spec: AtomSpec, mode: SelectionMode },
  /// Apply identities supplied by renderer picking, scoped by the caller's view revision.
  Atoms { atoms: AtomSelection, mode: SelectionMode },
  /// Select all coordinate-bearing atoms in the current model.
  All,
  /// Clear the current selection.
  Clear,
  /// Describe the current selection without mutation.
  Status,
}

impl SelectionCommand {
  /// Returns the stable command identity.
  pub const fn id(&self) -> super::CommandId {
    super::CommandId::Selection
  }
}
