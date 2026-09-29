/// Application-shell commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplicationCommand {
  /// Show or hide the command panel.
  ToggleCommandPanel,
  /// Show or hide the built-in terminal panel.
  ToggleTerminal,
}

impl ApplicationCommand {
  /// Returns the stable command identifier.
  pub fn id(&self) -> crate::CommandId {
    match self {
      Self::ToggleCommandPanel => crate::CommandId::ApplicationToggleCommandPanel,
      Self::ToggleTerminal => crate::CommandId::ApplicationToggleTerminal,
    }
  }
}
