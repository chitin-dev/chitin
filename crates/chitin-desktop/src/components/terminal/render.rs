//! Terminal content hosted by the reusable workbench bottom dock.

use chitin_ui::{
  composite::bottom_dock::{BottomDock, BottomDockResizeConfig},
  primitive::{
    button::{Button, ButtonSize, ButtonState, ButtonStyle, ButtonVariant},
    icon::Icon,
    input::select::{
      Select, SelectContent, SelectContentPosition, SelectGroup, SelectInputSize, SelectInputStyle, SelectInputVariant,
      SelectItem, SelectTrigger, SelectValue,
    },
    terminal::TerminalEmulator,
  },
  themes::{UIThemes, builtins},
};
use gpui::{
  Entity, InteractiveElement, ParentElement, Pixels, StatefulInteractiveElement, Styled, WeakEntity, div,
  prelude::FluentBuilder, px,
};

use super::{TerminalPanelControls, terminal_profile_icon};
use crate::{app::ChitinApp, fonts::TERMINAL_FONT_FAMILY, keybindings::COMMAND_TERMINAL_KEY_CONTEXT};
use chitin_terminal::TerminalProfile;

/// Renders the VT terminal emulator as the active bottom-dock item.
///
/// # Parameters
///
/// * `controls` contains the persistent emulator and in-process shell endpoint.
/// * `close` is the bottom dock's shared close-button state.
/// * `height` is the current workbench-level dock height.
/// * `theme` supplies semantic colors for the dock and terminal.
/// * `app` receives resize and keyboard events from the rendered content.
///
/// # Returns
///
/// A bottom-dock element containing the shared VT terminal surface.
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
      .border_l_1()
      .border_color(theme.border.muted)
      .overflow_y_scroll()
      .track_scroll(&tab_scroll),
    // The lane is only wide enough for one icon column, so the native bar would
    // cover the tabs themselves.
    px(0.0),
  );
  let sessions = controls.sessions.iter().fold(tab_strip, |sessions, session| {
    let selected = session.id == controls.active_session;
    sessions.child(
      Button::new(session.tab.clone())
        .size(ButtonSize::Small)
        .variant(ButtonVariant::Transparent)
        .style(
          ButtonStyle::new()
            .width(px(28.0))
            .height(px(28.0))
            .horizontal_padding(px(0.0))
            .background(if selected {
              theme.background.selection
            } else {
              builtins::TRANSPARENT
            }),
        )
        .theme(theme)
        .child(
          Icon::new(terminal_profile_icon(session.profile))
            .size(px(14.0))
            .color(if selected {
              theme.text.primary
            } else {
              theme.text.secondary
            })
            .hover_color(theme.text.primary)
            .theme(theme),
        ),
    )
  });
  let header_actions = div()
    .flex()
    .items_center()
    .gap_1()
    .child(new_terminal_select(controls.profile_select.clone(), theme))
    .child(
      Button::new(controls.close_session.clone())
        .size(ButtonSize::Small)
        .variant(ButtonVariant::Transparent)
        .style(
          ButtonStyle::new()
            .width(px(26.0))
            .height(px(24.0))
            .horizontal_padding(px(0.0)),
        )
        .theme(theme)
        .child(Icon::new("icons/terminal-trash.svg").size(px(14.0)).theme(theme)),
    );
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
        .child(div().flex().flex_col().flex_1().min_w_0().min_h_0().p_2().when_some(
          active_terminal,
          |terminal, active_terminal| {
            terminal.child(
              TerminalEmulator::new(active_terminal)
                .theme(theme)
                .font_family(TERMINAL_FONT_FAMILY),
            )
          },
        ))
        .child(sessions),
    );

  BottomDock::new("TERMINAL", close)
    .height(height)
    .theme(theme)
    .header_actions(header_actions)
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

/// Builds the icon-only selector used to create a terminal profile session.
fn new_terminal_select(
  profile_select: Entity<chitin_ui::primitive::input::select::SelectInputState>,
  theme: UIThemes,
) -> impl gpui::IntoElement {
  let group = TerminalProfile::ALL
    .into_iter()
    .fold(SelectGroup::new(), |group, profile| {
      group.item(SelectItem::new(profile.id(), profile.label()).icon(terminal_profile_icon(profile)))
    });
  Select::new(profile_select)
    .trigger(
      SelectTrigger::new()
        .value(SelectValue::new().placeholder(""))
        .icon("icons/terminal-add.svg")
        .show_indicator(false),
    )
    .content(
      SelectContent::new()
        .position(SelectContentPosition::Popper)
        .align_end()
        .group(group),
    )
    .variant(SelectInputVariant::Transparent)
    .size(SelectInputSize::Small)
    .style(
      SelectInputStyle::new()
        .width(px(28.0))
        .trigger_padding_x(px(6.0))
        .trigger_icon_size(px(16.0))
        .menu_width(px(180.0)),
    )
    .theme(theme)
}
