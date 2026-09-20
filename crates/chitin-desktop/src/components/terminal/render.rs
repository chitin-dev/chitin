//! Bottom-panel chrome around the reusable command terminal.

use chitin_ui::{
  composite::command_terminal::CommandTerminal,
  primitive::{
    button::{Button, ButtonSize, ButtonStyle, ButtonVariant},
    icon::Icon,
  },
  themes::UIThemes,
};
use gpui::{CursorStyle, InteractiveElement, MouseButton, ParentElement, Pixels, WeakEntity, div, prelude::*, px};

use super::{TERMINAL_FONT_FAMILY, TerminalPanelControls};
use crate::{app::ChitinApp, keybindings::COMMAND_TERMINAL_KEY_CONTEXT};

/// Renders the bottom panel chrome around the reusable command terminal.
pub(crate) fn render_terminal_panel(
  controls: TerminalPanelControls,
  height: Pixels,
  theme: UIThemes,
  app: WeakEntity<ChitinApp>,
) -> impl gpui::IntoElement {
  let resize_app = app.clone();
  div()
    .relative()
    .flex()
    .flex_col()
    .h(height)
    .min_h(height)
    .border_t_1()
    .border_color(theme.border.primary)
    .bg(theme.background.primary)
    .key_context(COMMAND_TERMINAL_KEY_CONTEXT)
    .capture_key_down(move |event, window, cx| {
      let _ = app.update(cx, |this, cx| this.handle_terminal_key(event, window, cx));
    })
    .child(
      div()
        .flex()
        .items_center()
        .justify_between()
        .h(px(34.0))
        .min_h(px(34.0))
        .px_3()
        .border_b_1()
        .border_color(theme.border.muted)
        .child(
          div()
            .text_xs()
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .child("TERMINAL"),
        )
        .child(
          Button::new(controls.close)
            .theme(theme)
            .variant(ButtonVariant::Transparent)
            .size(ButtonSize::Small)
            .style(
              ButtonStyle::new()
                .width(px(26.0))
                .height(px(24.0))
                .horizontal_padding(px(0.0)),
            )
            .child(Icon::new("icons/window-close.svg").size(px(14.0)).theme(theme)),
        ),
    )
    .child(
      CommandTerminal::new(controls.terminal)
        .theme(theme)
        .font_family(TERMINAL_FONT_FAMILY),
    )
    .child(
      div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(5.0))
        .cursor(CursorStyle::ResizeUpDown)
        .hover(move |style| style.bg(theme.border.focus))
        .on_mouse_down(MouseButton::Left, move |event, _, cx| {
          let _ = resize_app.update(cx, |this, cx| {
            this.terminal_panel.start_resize(event.position.y);
            cx.notify();
          });
        }),
    )
}
