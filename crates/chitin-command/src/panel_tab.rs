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
