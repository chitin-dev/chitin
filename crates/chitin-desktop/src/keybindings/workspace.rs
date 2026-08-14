use gpui::{KeyBinding, actions};

use super::CommandShortcut;

pub(crate) const PROJECT_TREE_KEY_CONTEXT: &str = "ProjectTree";
const FOCUS_PREVIOUS_SHORTCUTS: [CommandShortcut; 2] = [
  CommandShortcut::new("up", "Up", Some(PROJECT_TREE_KEY_CONTEXT)),
  CommandShortcut::new("k", "K", Some(PROJECT_TREE_KEY_CONTEXT)),
];
const FOCUS_NEXT_SHORTCUTS: [CommandShortcut; 2] = [
  CommandShortcut::new("down", "Down", Some(PROJECT_TREE_KEY_CONTEXT)),
  CommandShortcut::new("j", "J", Some(PROJECT_TREE_KEY_CONTEXT)),
];
const ACTIVATE_FOCUSED_SHORTCUTS: [CommandShortcut; 1] =
  [CommandShortcut::new("enter", "Enter", Some(PROJECT_TREE_KEY_CONTEXT))];
const FOCUS_FIRST_SHORTCUTS: [CommandShortcut; 2] = [
  CommandShortcut::new("home", "Home", Some(PROJECT_TREE_KEY_CONTEXT)),
  CommandShortcut::new("g g", "G G", Some(PROJECT_TREE_KEY_CONTEXT)),
];
const FOCUS_LAST_SHORTCUTS: [CommandShortcut; 2] = [
  CommandShortcut::new("end", "End", Some(PROJECT_TREE_KEY_CONTEXT)),
  CommandShortcut::new("G", "Shift+G", Some(PROJECT_TREE_KEY_CONTEXT)),
];
const TOGGLE_WORKSPACE_SHORTCUTS: [CommandShortcut; 1] = [CommandShortcut::new("shift-e", "Shift+E", None)];

actions!(
  workspace,
  [
    /// Move project tree focus to the previous visible entry.
    FocusPreviousEntry,
    /// Move project tree focus to the next visible entry.
    FocusNextEntry,
    /// Activate the focused project tree entry.
    ActivateFocusedEntry,
    /// Move project tree focus to the first visible entry.
    FocusFirstEntry,
    /// Move project tree focus to the last visible entry.
    FocusLastEntry,
    /// Show or hide the project workspace sidebar.
    ToggleWorkspace,
  ]
);

pub(super) fn default_key_bindings() -> [KeyBinding; 10] {
  [
    FOCUS_PREVIOUS_SHORTCUTS[0].binding(FocusPreviousEntry),
    FOCUS_PREVIOUS_SHORTCUTS[1].binding(FocusPreviousEntry),
    FOCUS_NEXT_SHORTCUTS[0].binding(FocusNextEntry),
    FOCUS_NEXT_SHORTCUTS[1].binding(FocusNextEntry),
    ACTIVATE_FOCUSED_SHORTCUTS[0].binding(ActivateFocusedEntry),
    FOCUS_FIRST_SHORTCUTS[0].binding(FocusFirstEntry),
    FOCUS_LAST_SHORTCUTS[0].binding(FocusLastEntry),
    FOCUS_FIRST_SHORTCUTS[1].binding(FocusFirstEntry),
    FOCUS_LAST_SHORTCUTS[1].binding(FocusLastEntry),
    TOGGLE_WORKSPACE_SHORTCUTS[0].binding(ToggleWorkspace),
  ]
}
