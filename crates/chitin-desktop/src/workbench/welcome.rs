//! Empty document area's workspace launch surface.

use chitin_command::CommandId;
use chitin_ui::workbench::{WorkbenchStyle, panel_surface, viewport_container};
use gpui::{Context, InteractiveElement, IntoElement, ParentElement, Styled, div, px};
use gpui_kit::component::{Icon, Sizable as _, theme::ThemeColor};

use crate::{
  app::ChitinApp,
  workbench::sidebar::{SidebarView, function_button},
};

/// Renders the empty document area's workspace title and shared launch actions.
pub(crate) fn render_welcome(app: &ChitinApp, theme: ThemeColor, cx: &Context<ChitinApp>) -> impl IntoElement {
  let title = app
    .workspace
    .as_ref()
    .and_then(|workspace| workspace.root.file_name())
    .map(|name| name.to_string_lossy().into_owned())
    .unwrap_or_else(|| "Chitin workspace".into());
  let weak = cx.weak_entity();
  panel_surface(WorkbenchStyle::new(theme)).child(
    viewport_container(WorkbenchStyle::new(theme))
      .items_center()
      .justify_center()
      .p_6()
      .gap_3()
      .child(
        Icon::default()
          .path("logo-app.svg")
          .with_size(px(96.0))
          .text_color(theme.foreground),
      )
      .child(div().text_xl().child(title))
      .child(
        div()
          .text_sm()
          .text_color(theme.muted_foreground)
          .child("Open a structure to begin exploring."),
      )
      .child(
        div()
          .id("welcome-actions")
          .flex()
          .flex_col()
          .w_full()
          .max_w(gpui::px(420.0))
          .mt_4()
          .gap_2()
          .child(
            gpui_kit::component::button::Button::new("welcome-project")
              .label("Browse project files")
              .w_full()
              .on_click(move |_, window, cx| {
                let _ = weak.update(cx, |app, cx| app.select_sidebar_view(SidebarView::Project, window, cx));
              }),
          )
          .child(
            function_button(
              CommandId::DatabaseDownloadRcsbStructure,
              "Download RCSB structure",
              Some("icons/download.svg"),
              cx,
            )
            .w_full(),
          )
          .child(
            function_button(
              CommandId::ApplicationToggleCommandPanel,
              "All commands…",
              Some("icons/activity-search.svg"),
              cx,
            )
            .w_full(),
          ),
      ),
  )
}
