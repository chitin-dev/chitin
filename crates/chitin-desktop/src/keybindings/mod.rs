//! GPUI input adapters for shared Chitin commands.

mod application;
mod dispatch;
mod tab;
mod workspace;

pub(crate) use application::ToggleCommandPanel;
pub(crate) use tab::{CloseTab, FocusNextPanelTab, FocusPreviousPanelTab, PANEL_CONTAINER_KEY_CONTEXT};
pub(crate) use workspace::{
  ActivateFocusedEntry, FocusFirstEntry, FocusLastEntry, FocusNextEntry, FocusPreviousEntry, PROJECT_TREE_KEY_CONTEXT,
  ToggleWorkspace,
};

/// A GPUI shortcut declared once for keybinding and command-panel display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandShortcut {
  /// GPUI keystroke string.
  pub(crate) keystrokes: &'static str,
  /// User-facing shortcut label.
  pub(crate) label: &'static str,
  /// Optional GPUI key context.
  pub(crate) context: Option<&'static str>,
}

impl CommandShortcut {
  /// Creates a shortcut specification.
  pub(crate) const fn new(keystrokes: &'static str, label: &'static str, context: Option<&'static str>) -> Self {
    Self {
      keystrokes,
      label,
      context,
    }
  }

  /// Builds a GPUI keybinding for this shortcut.
  pub(crate) fn binding<A: gpui::Action>(&self, action: A) -> gpui::KeyBinding {
    gpui::KeyBinding::new(self.keystrokes, action, self.context)
  }
}

/// Builds all default GPUI keybindings.
pub fn default_key_bindings() -> Vec<gpui::KeyBinding> {
  workspace::default_key_bindings()
    .into_iter()
    .chain(tab::default_key_bindings())
    .chain(application::default_key_bindings())
    .collect()
}
