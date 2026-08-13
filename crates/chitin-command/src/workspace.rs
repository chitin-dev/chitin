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
  pub fn id(&self) -> &'static str {
    match self {
      Self::FocusPrevious => "workspace.focus_previous_entry",
      Self::FocusNext => "workspace.focus_next_entry",
      Self::ActivateFocused => "workspace.activate_focused_entry",
      Self::FocusFirst => "workspace.focus_first_entry",
      Self::FocusLast => "workspace.focus_last_entry",
      Self::ToggleWorkspace => "workspace.toggle_workspace",
      Self::PanelTab(command) => command.id(),
    }
  }
}
