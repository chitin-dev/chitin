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
  pub fn id(&self) -> &'static str {
    match self {
      Self::FocusPrevious => "tab.focus_previous",
      Self::FocusNext => "tab.focus_next",
      Self::Close => "tab.close",
    }
  }
}

/// Returns the document-panel tab command descriptors.
pub fn command_descriptors() -> Vec<crate::CommandDescriptor> {
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
  .map(|(command, title, keywords, shortcut)| crate::CommandDescriptor {
    id: command.id(),
    title,
    category: crate::CommandCategory::Workspace,
    keywords,
    shortcut,
    invocation: crate::CommandInvocationKind::Immediate,
    command: crate::ChitinCommand::from(crate::WorkspaceCommand::PanelTab(command)),
  })
  .collect()
}
