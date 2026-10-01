//! Desktop content inside GPUI Kit's platform-aware title bar.

use gpui::prelude::FluentBuilder as _;
use gpui::{ParentElement, Styled, div, px};
use gpui_kit::component::{Icon, Sizable as _, TitleBar, theme::ThemeColor};

/// Renders the app logo while Kit owns native drag and window controls.
///
/// # Parameters
///
/// * `theme` supplies Kit's workbench background and text colors.
///
/// # Returns
///
/// A Kit title bar with the same background as the document chrome.
pub fn render_window_bar(theme: ThemeColor) -> TitleBar {
  TitleBar::new()
    .bg(theme.background)
    .h(px(30.0))
    .border_0()
    // macOS keeps Kit's native traffic-light inset.
    .when(!cfg!(target_os = "macos"), |bar| bar.pl_3())
    .on_close_window(|_, _, cx| cx.quit())
    .child(
      div()
        .flex()
        .size_full()
        .items_center()
        .child(Icon::default().path("logo-app.svg").with_size(px(20.0))),
    )
}
