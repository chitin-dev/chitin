//! Desktop adapter for the shared command registry.

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
