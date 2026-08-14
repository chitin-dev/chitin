use gpui::{KeyBinding, actions};

use super::CommandShortcut;

pub(crate) const PANEL_CONTAINER_KEY_CONTEXT: &str = "PanelContainer";
const FOCUS_PREVIOUS_TAB_SHORTCUTS: [CommandShortcut; 1] = [CommandShortcut::new(
  "shift-j",
  "Shift+J",
  Some(PANEL_CONTAINER_KEY_CONTEXT),
)];
const FOCUS_NEXT_TAB_SHORTCUTS: [CommandShortcut; 1] = [CommandShortcut::new(
  "shift-k",
  "Shift+K",
  Some(PANEL_CONTAINER_KEY_CONTEXT),
)];
const CLOSE_TAB_SHORTCUTS: [CommandShortcut; 1] = [CommandShortcut::new(
  "shift-x",
  "Shift+X",
  Some(PANEL_CONTAINER_KEY_CONTEXT),
)];

actions!(
  panel_tab,
  [
    /// Move panel tab focus to previous visual tab.
    FocusPreviousPanelTab,
    /// Move panel tab focus to next visual tab.
    FocusNextPanelTab,
    /// Close the current focused tab.
    CloseTab,
  ]
);

pub(super) fn default_key_bindings() -> [KeyBinding; 3] {
  [
    FOCUS_PREVIOUS_TAB_SHORTCUTS[0].binding(FocusPreviousPanelTab),
    FOCUS_NEXT_TAB_SHORTCUTS[0].binding(FocusNextPanelTab),
    CLOSE_TAB_SHORTCUTS[0].binding(CloseTab),
  ]
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn tab_bindings_should_use_panel_context() {
    let bindings = default_key_bindings();
    assert_eq!(bindings.len(), 3);
    assert!(bindings.iter().all(|binding| binding.predicate().is_some()));
  }
}
