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
