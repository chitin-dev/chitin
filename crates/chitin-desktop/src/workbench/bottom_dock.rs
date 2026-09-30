//! Desktop-owned controls for the reusable workbench bottom dock.

use gpui::Context;

use crate::app::ChitinApp;

impl ChitinApp {
  /// Closes whichever tool is active in the bottom dock.
  pub(crate) fn close_bottom_dock(&mut self, cx: &mut Context<Self>) {
    if self
      .bottom_dock
      .is_active(crate::features::terminal::TERMINAL_DOCK_ITEM_ID)
    {
      if let Some(controls) = self.terminal_panel_controls.as_ref() {
        controls.cancel_active_commands();
      }
      self.terminal_panel_controls = None;
    }
    self.bottom_dock.close();
    self.terminal_panel.request_focus(false);
    cx.notify();
  }
}
