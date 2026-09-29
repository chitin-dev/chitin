use crate::PanelTabCommand;

/// Workspace and project-tree commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceCommand {
  /// Focus the previous project entry.
  FocusPrevious,
  /// Focus the next project entry.
  FocusNext,
  /// Activate the focused project entry.
  ActivateFocused,
  /// Focus the first project entry.
  FocusFirst,
  /// Focus the last project entry.
  FocusLast,
  /// Show or hide the workspace sidebar.
  ToggleWorkspace,
  /// Execute a document-panel tab command.
  PanelTab(PanelTabCommand),
}

impl WorkspaceCommand {
  /// Returns the stable command identifier.
  pub fn id(&self) -> crate::CommandId {
    match self {
      Self::FocusPrevious => crate::CommandId::WorkspaceFocusPrevious,
      Self::FocusNext => crate::CommandId::WorkspaceFocusNext,
      Self::ActivateFocused => crate::CommandId::WorkspaceActivateFocused,
      Self::FocusFirst => crate::CommandId::WorkspaceFocusFirst,
      Self::FocusLast => crate::CommandId::WorkspaceFocusLast,
      Self::ToggleWorkspace => crate::CommandId::WorkspaceToggle,
      Self::PanelTab(command) => command.id(),
    }
  }
}
