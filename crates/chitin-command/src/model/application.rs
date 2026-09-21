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

/// Returns the application command registrations.
pub fn command_registrations() -> Vec<crate::CommandRegistration> {
  vec![
    crate::CommandRegistration {
      descriptor: crate::CommandDescriptor {
        id: ApplicationCommand::ToggleCommandPanel.id(),
        title: "Toggle Command Panel",
        requires_arguments: false,
      },
      category: crate::CommandCategory::Application,
      keywords: &["quick pick", "palette", "commands"],
      shortcut: Some("Ctrl/Cmd+Shift+P"),
    },
    crate::CommandRegistration {
      descriptor: crate::CommandDescriptor {
        id: ApplicationCommand::ToggleTerminal.id(),
        title: "Toggle Terminal",
        requires_arguments: false,
      },
      category: crate::CommandCategory::Application,
      keywords: &["built-in shell", "console", "command line"],
      shortcut: Some("Shift+T"),
    },
  ]
}
