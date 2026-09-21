//! Terminal content hosted by the reusable workbench bottom dock.

use chitin_ui::{
  composite::{
    bottom_dock::{BottomDock, BottomDockResizeConfig},
    command_terminal::CommandTerminal,
  },
  primitive::button::ButtonState,
  themes::UIThemes,
};
use gpui::{Entity, InteractiveElement, ParentElement, Pixels, Styled, WeakEntity, div};

use super::{TERMINAL_FONT_FAMILY, TerminalPanelControls};
use crate::{app::ChitinApp, keybindings::COMMAND_TERMINAL_KEY_CONTEXT};

/// Renders the command terminal as the active bottom-dock item.
///
/// # Parameters
///
/// * `controls` contains the persistent terminal transcript and input state.
/// * `close` is the bottom dock's shared close-button state.
/// * `height` is the current workbench-level dock height.
/// * `theme` supplies semantic colors for the dock and terminal.
/// * `app` receives resize and keyboard events from the rendered content.
///
/// # Returns
///
/// A bottom-dock element containing the structured command terminal.
pub(crate) fn render_terminal_bottom_dock(
  controls: TerminalPanelControls,
  close: Entity<ButtonState>,
  height: Pixels,
  theme: UIThemes,
  app: WeakEntity<ChitinApp>,
) -> impl gpui::IntoElement {
  let resize_app = app.clone();
  let resize_move_app = app.clone();
  let resize_end_app = app.clone();
  let terminal = div()
    .flex()
    .flex_col()
    .flex_1()
    .min_h_0()
    .key_context(COMMAND_TERMINAL_KEY_CONTEXT)
    .capture_key_down(move |event, window, cx| {
      let _ = app.update(cx, |this, cx| this.handle_terminal_key(event, window, cx));
    })
    .child(
      CommandTerminal::new(controls.terminal)
        .theme(theme)
        .font_family(TERMINAL_FONT_FAMILY),
    );

  BottomDock::new("TERMINAL", close)
    .height(height)
    .theme(theme)
    .resizable(
      BottomDockResizeConfig::new(move |start_y, _, cx| {
        let _ = resize_app.update(cx, |this, cx| {
          this.bottom_dock.start_resize(start_y);
          cx.notify();
        });
      })
      .on_resize(move |current_y, window, cx| {
        let available_height = ChitinApp::document_panel_root_height(window.bounds().size.height);
        let _ = resize_move_app.update(cx, |this, cx| {
          if this.bottom_dock.drag_resize(current_y, available_height) {
            cx.notify();
          }
        });
      })
      .on_resize_end(move |_, cx| {
        let _ = resize_end_app.update(cx, |this, cx| {
          if this.bottom_dock.stop_resize() {
            cx.notify();
          }
        });
      }),
    )
    .child(terminal)
}
