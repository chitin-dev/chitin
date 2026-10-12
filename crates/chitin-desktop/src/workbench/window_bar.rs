//! Desktop content inside GPUI Kit's platform-aware title bar.

use crate::app::ChitinApp;
use gpui::prelude::FluentBuilder as _;
use gpui::{ParentElement, Styled, WeakEntity, div, px};
use gpui_kit::component::{
  Icon, Sizable as _, TitleBar,
  button::{Button, ButtonVariants as _},
  theme::ThemeColor,
};

/// Renders the app logo while Kit owns native drag and window controls.
///
/// # Parameters
///
/// * `theme` supplies Kit's workbench background and text colors.
///
/// # Returns
///
/// A Kit title bar with the same background as the document chrome.
pub fn render_window_bar(theme: ThemeColor, app: WeakEntity<ChitinApp>) -> TitleBar {
  let search_app = app.clone();
  let style = chitin_ui::workbench::WorkbenchStyle::new(theme);
  TitleBar::new()
    .bg(style.background)
    .h(style.header_height)
    .border_0()
    .border_b_1()
    .border_color(style.separator)
    // macOS keeps Kit's native traffic-light inset.
    .when(!cfg!(target_os = "macos"), |bar| bar.pl_3())
    .on_close_window(|_, _, cx| cx.quit())
    .child(
      div()
        .flex()
        .size_full()
        .items_center()
        .gap_2()
        .child(Icon::default().path("logo-app.svg").with_size(px(20.0)))
        .child(
          Button::new("window-sidebar-toggle")
            .ghost()
            .small()
            .icon(Icon::default().path("icons/panel-split-horizontal.svg"))
            .tooltip("Toggle sidebar")
            .accessibility_label("Toggle sidebar")
            .on_click(move |_, window, cx| {
              let _ = app.update(cx, |app, cx| {
                app.project_sidebar_visible = !app.project_sidebar_visible;
                let focus = if app.project_sidebar_visible && app.sidebar_view == super::sidebar::SidebarView::Project {
                  app.project_sidebar_focus(cx)
                } else {
                  app.document_panel_focus(cx)
                };
                window.focus(&focus, cx);
                cx.notify();
              });
            }),
        )
        .child(
          Button::new("window-command-search")
            .ghost()
            .small()
            .label("Commands")
            .on_click(move |_, window, cx| {
              let _ = search_app.update(cx, |app, cx| {
                if app.command_panel.toggle(window, cx) {
                  cx.notify();
                }
              });
            }),
        )
        .child(div().flex_1()),
    )
}
