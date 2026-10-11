//! Object visibility independent of selection and representation layers.

use super::{AtomSpec, CommandId};

/// Objects resolved in the current rendering panel, including hidden objects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VisibilityTarget {
  /// The current canonical selection.
  Selected,
  /// Every coordinate-bearing atom in the model.
  All,
  /// An author-ID specification, resolved against the original structure.
  Match(AtomSpec),
}

/// Hide or restore objects without deleting data or changing selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibilityCommand {
  /// Objects resolved against the panel's original structure or selection.
  pub target: VisibilityTarget,
  /// Whether matched objects should be restored rather than hidden.
  pub visible: bool,
}

impl VisibilityCommand {
  /// Returns the stable command identity.
  pub const fn id(&self) -> CommandId {
    CommandId::Visibility
  }
}
