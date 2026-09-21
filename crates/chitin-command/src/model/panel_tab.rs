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

/// Returns the document-panel tab command registrations.
pub fn command_registrations() -> Vec<crate::CommandRegistration> {
  [
    (
      PanelTabCommand::FocusPrevious,
      "Focus Previous Tab",
      &["document", "panel", "tab", "previous"][..],
      Some("Shift+J"),
    ),
    (
      PanelTabCommand::FocusNext,
      "Focus Next Tab",
      &["document", "panel", "tab", "next"][..],
      Some("Shift+K"),
    ),
    (
      PanelTabCommand::Close,
      "Close Active Tab",
      &["document", "panel", "tab", "close"][..],
      Some("Shift+X"),
    ),
  ]
  .into_iter()
  .map(|(command, title, keywords, shortcut)| crate::CommandRegistration {
    descriptor: crate::CommandDescriptor {
      id: command.id(),
      title,
      requires_arguments: false,
    },
    category: crate::CommandCategory::Workspace,
    keywords,
    shortcut,
  })
  .collect()
}
