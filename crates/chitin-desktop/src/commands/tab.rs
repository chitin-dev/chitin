use gpui::{KeyBinding, actions};

use crate::app::ChitinApp;

/// GPUI key context used by the document panel container.
pub(crate) const PANEL_CONTAINER_KEY_CONTEXT: &str = "PanelContainer";

actions!(
  panel_tab,
  [
    /// Move panel tab focus to previous visual tab
    FocusPreviousPanelTab,
    /// Move panel tab focus to next visual tab
    FocusNextPanelTab,
    /// Close current focused / activated visual tab
    CloseTab
  ]
);

/// Panel-tab-scoped commands supported by Chitin desktop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PanelTabCommand {
  /// Move panel tab focus to previous visual tab
  FocusPrevious,
  /// Move panel tab focus to next visual tab
  FocusNext,
  /// Close current focused / activated visual tab
  Close,
}

impl PanelTabCommand {
  /// Returns the stable dotted identifier for this panel-tab command.
  ///
  /// The ID is the string form that future text/config entry points should use
  /// when they need to refer to panel-tab behavior without linking directly to
  /// Rust enum variants.
  ///
  /// # Parameters
  ///
  /// This method reads `self`, the panel-tab command variant being identified.
  ///
  /// # Returns
  ///
  /// A static command ID such as `"tab.focus_previous"`.
  pub(crate) fn id(&self) -> &'static str {
    match self {
      Self::FocusPrevious => "tab.focus_previous",
      Self::FocusNext => "tab.focus_next",
      Self::Close => "tab.close",
    }
  }
}

impl ChitinApp {
  /// Executes a panel-tab command against the focused document panel.
  ///
  /// # Parameters
  ///
  /// `command` is the panel-tab command to execute.
  ///
  /// `cx` is the GPUI app context notified when the command mutates state.
  ///
  /// # Returns
  ///
  /// This function returns `()` after routing the command to document panel
  /// state.
  pub(crate) fn dispatch_panel_tab_command(&mut self, command: PanelTabCommand, cx: &mut gpui::Context<Self>) {
    let changed = match command {
      PanelTabCommand::FocusPrevious => self.focus_previous_document_panel_tab(),
      PanelTabCommand::FocusNext => self.focus_next_document_panel_tab(),
      PanelTabCommand::Close => self.close_focused_document_panel_tab(),
    };

    if changed {
      cx.notify();
    }
  }
}

/// Builds default keybindings for focused document panel containers.
///
/// `Shift+J` and `Shift+K` move the active tab in circular visual order, and
/// `Shift+X` closes the active tab. Every binding is scoped to
/// [`PANEL_CONTAINER_KEY_CONTEXT`] so these shortcuts only dispatch when the
/// panel container owns keyboard focus.
///
/// # Parameters
///
/// This function takes no parameters.
///
/// # Returns
///
/// Three GPUI keybindings for document panel tab navigation and close.
pub(crate) fn default_key_bindings() -> [KeyBinding; 3] {
  [
    KeyBinding::new("shift-j", FocusPreviousPanelTab, Some(PANEL_CONTAINER_KEY_CONTEXT)),
    KeyBinding::new("shift-k", FocusNextPanelTab, Some(PANEL_CONTAINER_KEY_CONTEXT)),
    KeyBinding::new("shift-x", CloseTab, Some(PANEL_CONTAINER_KEY_CONTEXT)),
  ]
}

impl From<PanelTabCommand> for crate::commands::ChitinCommand {
  /// Wraps a panel-tab command in the top-level desktop command hierarchy.
  ///
  /// # Parameters
  ///
  /// `command` is the panel-tab command to expose as a generic command.
  ///
  /// # Returns
  ///
  /// [`crate::commands::ChitinCommand::PanelTab`] containing the command.
  fn from(command: PanelTabCommand) -> Self {
    Self::PanelTab(command)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Verifies that panel-tab key bindings are scoped to the panel context.
  #[test]
  fn default_key_bindings_should_use_panel_container_context() {
    let bindings = default_key_bindings();

    assert_eq!(bindings.len(), 3);
    assert!(bindings.iter().all(|binding| binding.predicate().is_some()));
  }

  /// Verifies that panel-tab shortcuts use shifted letter keys.
  #[test]
  fn default_key_bindings_should_include_shifted_tab_shortcuts() {
    let bindings = default_key_bindings();

    for key in ["j", "k", "x"] {
      assert!(bindings.iter().any(|binding| {
        let keystrokes = binding.keystrokes();
        keystrokes.len() == 1 && keystrokes[0].key() == key && keystrokes[0].modifiers().shift
      }));
    }
  }
}
