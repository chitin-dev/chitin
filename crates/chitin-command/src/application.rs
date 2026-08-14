/// Application-shell commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplicationCommand {
  /// Show or hide the command panel.
  ToggleCommandPanel,
}

impl ApplicationCommand {
  /// Returns the stable command identifier.
  pub fn id(&self) -> &'static str {
    match self {
      Self::ToggleCommandPanel => "application.toggle_command_panel",
    }
  }
}

/// Returns the application command descriptors.
pub fn command_descriptors() -> Vec<crate::CommandDescriptor> {
  vec![crate::CommandDescriptor {
    id: ApplicationCommand::ToggleCommandPanel.id(),
    title: "Toggle Command Panel",
    category: crate::CommandCategory::Application,
    keywords: &["quick pick", "palette", "commands"],
    shortcut: Some("Ctrl/Cmd+Shift+P"),
    invocation: crate::CommandInvocationKind::Immediate,
    command: ApplicationCommand::ToggleCommandPanel.into(),
  }]
}
