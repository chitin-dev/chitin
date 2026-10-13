//! Terminal-owned session navigation and content; workbench owns the overlay.

use chitin_ui::{
  views::terminal::TerminalEmulator,
  workbench::{WorkbenchStyle, bottom_dock::BOTTOM_DOCK_BAR_HEIGHT},
};
use gpui::{ElementId, InteractiveElement, ParentElement, Styled, WeakEntity, div, prelude::FluentBuilder, px};
use gpui_kit::component::{
  Disableable as _, Icon, Sizable as _,
  button::{Button, ButtonVariant, ButtonVariants as _},
  menu::{DropdownMenu as _, PopupMenuItem},
  tab::{Tab, TabBar},
  theme::ThemeColor,
};

use super::{DesktopTerminalProfile, TerminalPanelControls, terminal_profile_icon};
use crate::{app::ChitinApp, fonts::TERMINAL_FONT_FAMILY, keybindings::COMMAND_TERMINAL_KEY_CONTEXT};

/// Renders persistent sessions without owning dock geometry or visibility.
pub(crate) fn render_terminal_panel(
  controls: TerminalPanelControls,
  theme: ThemeColor,
  app: WeakEntity<ChitinApp>,
) -> impl gpui::IntoElement {
  let style = WorkbenchStyle::new(theme);
  let active_terminal = controls.active().map(|session| session.terminal.clone());
  let selected = controls
    .sessions
    .iter()
    .position(|session| session.id == controls.active_session)
    .unwrap_or(0);
  let tabs = controls.sessions.iter().map(|session| {
    let id = session.id;
    let close_app = app.clone();
    let label = if session.profile.id() == "builtin-shell" {
      "chitin"
    } else {
      session.profile.label()
    };
    Tab::new()
      .mr_1()
      .pl_2()
      .label(label.to_owned())
      .prefix(
        Icon::default()
          .path(terminal_profile_icon(session.profile.id()))
          .with_size(px(12.0)),
      )
      .suffix(
        Button::new(ElementId::from(("terminal-session-close", id.0)))
          .with_variant(ButtonVariant::Ghost)
          .tooltip("Close session")
          .w(px(20.0))
          .h(px(20.0))
          .px(px(0.0))
          .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let _ = close_app.update(cx, |this, cx| this.close_terminal_session(id, cx));
          })
          .child(Icon::default().path("icons/window-close.svg").with_size(px(12.0))),
      )
  });
  let ids: Vec<_> = controls.sessions.iter().map(|session| session.id).collect();
  let activate_app = app.clone();
  let navigation = TabBar::new("terminal-session-tabs")
    .segmented()
    .selected_background(style.raised_background)
    .small()
    .bg(style.panel_background)
    .menu(false)
    .track_scroll(&controls.tab_scroll)
    .selected_index(selected)
    .children(tabs)
    .on_click(move |index, window, cx| {
      if let Some(id) = ids.get(*index) {
        let _ = activate_app.update(cx, |this, cx| this.activate_terminal_session(*id, window, cx));
      }
    });
  let new_session = new_terminal_menu(&controls, app.clone());
  let clear_app = app;
  let actions = div()
    .flex()
    .flex_none()
    .items_center()
    .gap_1()
    .child(new_session)
    .child(
      terminal_action_button("terminal-clear-scrollback", "icons/terminal-trash.svg", "Clear")
        .tooltip("Clear scrollback")
        .disabled(active_terminal.is_none())
        .on_click(move |_, _, cx| {
          let _ = clear_app.update(cx, |this, cx| this.clear_active_terminal_scrollback(cx));
        }),
    );
  div()
    .flex()
    .flex_col()
    .flex_1()
    .min_h_0()
    .min_w_0()
    .bg(style.panel_background)
    .key_context(COMMAND_TERMINAL_KEY_CONTEXT)
    .child(
      div()
        .flex()
        .items_center()
        .h(BOTTOM_DOCK_BAR_HEIGHT)
        .flex_none()
        .px_2()
        .child(div().flex_1().min_w_0().child(navigation))
        .child(actions),
    )
    .child(
      // Utility content belongs to the shared dock surface, not a viewport card.
      div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .bg(style.panel_background)
        .p_2()
        .when_some(active_terminal.clone(), |content, terminal| {
          content.child(
            TerminalEmulator::new(terminal)
              .theme(theme)
              .background(style.panel_background)
              .font_family(TERMINAL_FONT_FAMILY),
          )
        })
        .when(active_terminal.is_none(), |content| {
          content.child(
            div()
              .text_xs()
              .text_color(theme.muted_foreground)
              .child("No terminal sessions. Use + to start a shell."),
          )
        }),
    )
}

/// Keeps both toolbar actions on the same Kit button appearance and sizing.
fn terminal_action_button(id: &'static str, icon: &'static str, label: &'static str) -> Button {
  Button::new(id)
    .with_variant(ButtonVariant::Ghost)
    .small()
    .h(px(24.0))
    .text_xs()
    .px_2()
    .child(Icon::default().path(icon).with_size(px(14.0)))
    .child(label)
}

/// Kit owns menu focus, keyboard navigation and dismissal; callbacks only launch shells.
fn new_terminal_menu(controls: &TerminalPanelControls, app: WeakEntity<ChitinApp>) -> impl gpui::IntoElement {
  let profiles: Vec<_> = std::iter::once(DesktopTerminalProfile::Builtin)
    .chain(
      controls
        .catalog
        .available()
        .iter()
        .cloned()
        .map(DesktopTerminalProfile::Shell),
    )
    .collect();
  terminal_action_button("terminal-new-session", "icons/terminal-add.svg", "New session").dropdown_menu(
    move |menu, _, _| {
      profiles.iter().fold(menu.min_w(px(180.0)), |menu, profile| {
        let launch_app = app.clone();
        let launch_profile = profile.clone();
        menu.item(
          PopupMenuItem::new(profile.label().to_owned())
            .icon(Icon::default().path(terminal_profile_icon(profile.id())))
            .on_click(move |_, window, cx| {
              let _ = launch_app.update(cx, |this, cx| {
                this.create_terminal_session(launch_profile.clone(), window, cx)
              });
            }),
        )
      })
    },
  )
}
