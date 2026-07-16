//! Desktop window bar composition.
//!
//! This module maps Chitin's window controls onto the generic
//! `chitin-ui::WindowBar` component.

use chitin_ui::{
  components::window_bar::{WindowBar, WindowBarItem, WindowBarSubtitlePosition},
  themes::builtins,
};
use gpui::{Context, IntoElement};

use crate::app::ChitinApp;

/// Renders the top window bar with app title and platform window controls.
pub fn render_window_bar(_cx: &mut Context<ChitinApp>) -> impl IntoElement {
  let theme = builtins::dark();

  WindowBar::new("Chitin", "logo-app.svg", WindowBarSubtitlePosition::Right)
    .theme(theme)
    .subtitle("open your project")
    .items([
      WindowBarItem::new("minimize", "Minimize", "icons/window-bar/lucide-minus.svg")
        .on_click(|_, window, _| window.minimize_window()),
      WindowBarItem::new("maximize", "Maximize", "icons/window-bar/lucide-square.svg")
        .on_click(|_, window, _| window.zoom_window()),
      WindowBarItem::new("close", "Close", "icons/window-bar/lucide-x.svg")
        .on_click(|_, _, cx| cx.quit()),
    ])
}
