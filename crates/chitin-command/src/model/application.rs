/// Application-shell commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplicationCommand {
  /// Restore the previous committed workbench edit.
  Undo,
  /// Reapply a workbench edit that was undone.
  Redo,
  /// Show or hide the command panel.
  ToggleCommandPanel,
  /// Show or hide the built-in terminal panel.
  ToggleTerminal,
}

impl ApplicationCommand {
  /// Returns the stable command identifier.
  pub fn id(&self) -> crate::CommandId {
    match self {
      Self::Undo => crate::CommandId::ApplicationUndo,
      Self::Redo => crate::CommandId::ApplicationRedo,
      Self::ToggleCommandPanel => crate::CommandId::ApplicationToggleCommandPanel,
      Self::ToggleTerminal => crate::CommandId::ApplicationToggleTerminal,
    }
  }
}
