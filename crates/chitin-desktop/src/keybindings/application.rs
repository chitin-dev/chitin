use gpui::{KeyBinding, actions};

use super::CommandShortcut;

pub(crate) const COMMAND_TERMINAL_KEY_CONTEXT: &str = "CommandTerminal";
pub(crate) const WORKBENCH_KEY_CONTEXT: &str = "Workbench";
const TOGGLE_COMMAND_PANEL_SHORTCUTS: [CommandShortcut; 1] =
  [CommandShortcut::new("secondary-shift-p", "Ctrl/Cmd+Shift+P", None)];
const TOGGLE_TERMINAL_SHORTCUTS: [CommandShortcut; 1] =
  [CommandShortcut::new("shift-t", "Shift+T", Some("!CommandTerminal"))];

actions!(
  application,
  [
    /// Show or hide the command panel.
    ToggleCommandPanel,
    /// Show or hide the built-in terminal panel.
    ToggleTerminal,
    /// Undo a committed workbench edit.
    UndoWorkbenchEdit,
    /// Redo an undone workbench edit.
    RedoWorkbenchEdit,
  ]
);

pub(super) fn default_key_bindings() -> [KeyBinding; 4] {
  [
    KeyBinding::new(
      "ctrl-z",
      UndoWorkbenchEdit,
      Some("Workbench && !Input && !CommandTerminal"),
    ),
    KeyBinding::new(
      "ctrl-shift-z",
      RedoWorkbenchEdit,
      Some("Workbench && !Input && !CommandTerminal"),
    ),
    TOGGLE_COMMAND_PANEL_SHORTCUTS[0].binding(ToggleCommandPanel),
    TOGGLE_TERMINAL_SHORTCUTS[0].binding(ToggleTerminal),
  ]
}
