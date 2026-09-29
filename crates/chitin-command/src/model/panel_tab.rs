/// Commands for document-panel tabs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PanelTabCommand {
  /// Focus the previous tab.
  FocusPrevious,
  /// Focus the next tab.
  FocusNext,
  /// Close the active tab.
  Close,
}

impl PanelTabCommand {
  /// Returns the stable command identifier.
  pub fn id(&self) -> crate::CommandId {
    match self {
      Self::FocusPrevious => crate::CommandId::PanelTabFocusPrevious,
      Self::FocusNext => crate::CommandId::PanelTabFocusNext,
      Self::Close => crate::CommandId::PanelTabClose,
    }
  }
}
