use gpui::{KeyBinding, actions};

use super::CommandShortcut;

const TOGGLE_COMMAND_PANEL_SHORTCUTS: [CommandShortcut; 1] =
  [CommandShortcut::new("secondary-shift-p", "Ctrl/Cmd+Shift+P", None)];

actions!(
  application,
  [
    /// Show or hide the command panel.
    ToggleCommandPanel,
  ]
);

pub(super) fn default_key_bindings() -> [KeyBinding; 1] {
  [TOGGLE_COMMAND_PANEL_SHORTCUTS[0].binding(ToggleCommandPanel)]
}
