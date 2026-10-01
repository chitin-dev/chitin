//! Terminal content hosted by the reusable workbench bottom dock.

use chitin_ui::widgets::select_item::IconSelectItem;
use chitin_ui::workbench::{WorkbenchStyle, viewport_container};
use chitin_ui::{views::terminal::TerminalEmulator, workbench::bottom_dock::BottomDock};
use gpui_kit::component::theme::ThemeColor;

use gpui::{
  ElementId, Entity, InteractiveElement, ParentElement, Pixels, StatefulInteractiveElement, Styled, WeakEntity, div,
  prelude::FluentBuilder, px,
};
use gpui_kit::component::{
  Icon, Selectable as _, Sizable as _,
  button::{Button, ButtonVariant, ButtonVariants as _},
  select::{Select, SelectState},
};

use super::{TerminalPanelControls, terminal_profile_icon};
use crate::{app::ChitinApp, fonts::TERMINAL_FONT_FAMILY, keybindings::COMMAND_TERMINAL_KEY_CONTEXT};

/// Renders the VT terminal emulator as the active bottom-dock item.
///
/// # Parameters
///
/// * `controls` contains the persistent emulator and in-process shell endpoint.
/// * `height` is the current workbench-level dock height.
/// * `theme` supplies semantic colors for the dock and terminal.
/// * `app` receives resize, close, and session activation from the rendered content.
///
/// # Returns
///
/// A bottom-dock element containing the shared VT terminal surface.
pub(crate) fn render_terminal_bottom_dock(
  controls: TerminalPanelControls,
  height: Pixels,
  theme: ThemeColor,
  app: WeakEntity<ChitinApp>,
) -> impl gpui::IntoElement {
  let style = WorkbenchStyle::new(theme);
  let resize_app = app.clone();
  let close_app = app.clone();
  let active_terminal = controls.active().map(|session| session.terminal.clone());
  let tab_scroll = controls.tab_scroll.clone();
  let tab_strip = Styled::scrollbar_width(
    div()
      .id("terminal-session-tabs")
      .flex()
      .flex_col()
      .gap_1()
      .w(px(38.0))
      .min_h_0()
      .p_1()
      .overflow_y_scroll()
      .track_scroll(&tab_scroll),
    // The lane is only wide enough for one icon column, so the native bar would
    // cover the tabs themselves.
    px(0.0),
  );
  let sessions = controls.sessions.iter().fold(tab_strip, |sessions, session| {
    let selected = session.id == controls.active_session;
    let session_id = session.id;
    let tab_app = app.clone();
    sessions.child(
      Button::new(ElementId::from(("terminal-session-tab", session_id.0)))
        .with_variant(ButtonVariant::Ghost)
        .selected(selected)
        .w(px(28.0))
        .h(px(28.0))
        .px(px(0.0))
        .on_click(move |_, window, cx| {
          let _ = tab_app.update(cx, |this, cx| this.activate_terminal_session(session_id, window, cx));
        })
        // The icon takes the button's foreground rather than a colour of its own,
        // so it stays legible against the tab's selected and hover backgrounds.
        .child(
          Icon::default()
            .path(terminal_profile_icon(session.profile.id()))
            .with_size(px(14.0)),
        ),
    )
  });
  let header_actions = div()
    .flex()
    .items_center()
    .gap_1()
    .child(new_terminal_select(controls.profile_select.clone()))
    .child({
      let session_close_app = app.clone();
      Button::new("terminal-session-close")
        .with_variant(ButtonVariant::Ghost)
        .w(px(26.0))
        .h(px(24.0))
        .px(px(0.0))
        .on_click(move |_, _, cx| {
          let _ = session_close_app.update(cx, |this, cx| this.close_active_terminal_session(cx));
        })
        .child(Icon::default().path("icons/terminal-trash.svg").with_size(px(14.0)))
    });
  let terminal = div()
    .flex()
    .flex_col()
    .flex_1()
    .min_h_0()
    .key_context(COMMAND_TERMINAL_KEY_CONTEXT)
    .child(
      div()
        .flex()
        .flex_row()
        .flex_1()
        .min_h_0()
        .child(
          viewport_container(style)
            .p_2()
            .when_some(active_terminal, |terminal, active_terminal| {
              terminal.child(
                TerminalEmulator::new(active_terminal)
                  .theme(theme)
                  .font_family(TERMINAL_FONT_FAMILY),
              )
            }),
        )
        .child(sessions),
    );

  BottomDock::new("Terminal")
    .on_close(move |_, _, cx| {
      let _ = close_app.update(cx, |this, cx| this.close_bottom_dock(cx));
    })
    .height(height)
    .on_resize(move |height, _, cx| {
      let _ = resize_app.update(cx, |this, cx| {
        if this.bottom_dock.set_height(height) {
          cx.notify();
        }
      });
    })
    .theme(theme)
    .header_actions(header_actions)
    .child(terminal)
}

/// Builds the icon-only selector used to create a terminal profile session.
fn new_terminal_select(profile_select: Entity<SelectState<Vec<IconSelectItem>>>) -> impl gpui::IntoElement {
  Select::new(&profile_select)
    .placeholder("")
    .accessibility_label("New terminal session")
    .icon(Icon::default().path("icons/terminal-add.svg"))
    .appearance(false)
    .small()
    .w(px(28.0))
    .px(px(4.0))
    .menu_width(px(180.0))
}
