//! Desktop window bar composition.
//!
//! This module maps desktop window actions onto the composite window bar.

use chitin_ui::{
  composite::window_bar::{WindowBar, WindowBarItem, WindowBarSubtitlePosition},
  primitive::{button::ButtonEvent, button::ButtonState, text::TextState},
  themes::UIThemes,
};
use gpui::{AppContext, Context, Entity, Subscription, Window};

use crate::app::ChitinApp;

/// Persistent primitive button states for desktop window controls.
#[derive(Clone)]
pub(crate) struct WindowBarControls {
  title: Entity<TextState>,
  minimize: Entity<ButtonState>,
  maximize: Entity<ButtonState>,
  close: Entity<ButtonState>,
}

impl WindowBarControls {
  /// Creates button state for each platform window action.
  pub(crate) fn new(cx: &mut Context<ChitinApp>) -> Self {
    Self {
      title: cx.new(|cx| TextState::new("chitin", cx)),
      minimize: cx.new(ButtonState::new),
      maximize: cx.new(ButtonState::new),
      close: cx.new(ButtonState::new),
    }
  }

  /// Subscribes desktop window actions to primitive button clicks.
  pub(crate) fn subscribe(&self, window: &mut Window, cx: &mut Context<ChitinApp>) {
    subscribe_window_action(&self.minimize, window, cx, |window, _| window.minimize_window());
    subscribe_window_action(&self.maximize, window, cx, |window, _| window.zoom_window());
    subscribe_window_action(&self.close, window, cx, |_, cx| cx.quit());
  }
}

/// Connects one primitive button to a desktop-only window action.
fn subscribe_window_action(
  button: &Entity<ButtonState>,
  window: &mut Window,
  cx: &mut Context<ChitinApp>,
  action: impl Fn(&mut Window, &mut gpui::App) + 'static,
) {
  let subscription: Subscription = cx.subscribe_in(button, window, move |_, _, event, window, cx| {
    if matches!(event, ButtonEvent::Click) {
      action(window, cx);
    }
  });
  subscription.detach();
}

/// Renders the top window bar with app title and platform window controls.
pub fn render_window_bar(theme: UIThemes, controls: WindowBarControls) -> WindowBar {
  WindowBar::new(controls.title, "logo-app.svg", WindowBarSubtitlePosition::Right)
    .theme(theme)
    .subtitle("open your project")
    .items([
      WindowBarItem::new(
        "minimize",
        "Minimize",
        "icons/window-bar/lucide-minus.svg",
        controls.minimize,
      ),
      WindowBarItem::new(
        "maximize",
        "Maximize",
        "icons/window-bar/lucide-square.svg",
        controls.maximize,
      ),
      WindowBarItem::new("close", "Close", "icons/window-bar/lucide-x.svg", controls.close),
    ])
}
