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

/// Returns the project-workspace command registrations.
pub fn command_registrations() -> Vec<crate::CommandRegistration> {
  [
    (
      WorkspaceCommand::ToggleWorkspace,
      "Toggle Workspace Sidebar",
      &["files", "project", "sidebar", "explorer"][..],
      Some("Shift+E"),
    ),
    (
      WorkspaceCommand::FocusPrevious,
      "Focus Previous Project Entry",
      &["tree", "up", "previous"][..],
      Some("Up"),
    ),
    (
      WorkspaceCommand::FocusNext,
      "Focus Next Project Entry",
      &["tree", "down", "next"][..],
      Some("Down"),
    ),
    (
      WorkspaceCommand::ActivateFocused,
      "Activate Focused Project Entry",
      &["open", "tree", "file", "directory"][..],
      Some("Enter"),
    ),
    (
      WorkspaceCommand::FocusFirst,
      "Focus First Project Entry",
      &["tree", "home", "first"][..],
      Some("Home"),
    ),
    (
      WorkspaceCommand::FocusLast,
      "Focus Last Project Entry",
      &["tree", "end", "last"][..],
      Some("End"),
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
