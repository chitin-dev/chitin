use chitin_core::WorkspaceSummary;
use chitin_ui::themes::builtins;
use gpui::{Context, FontWeight, Render, Window, div, prelude::*};

use crate::components::{
  activity_bar::{ActiveActivity, render_activity_bar},
  window_bar::render_window_bar,
};

pub struct ChitinApp {
  summary: WorkspaceSummary,
  pub(crate) active_activity: ActiveActivity,
}

impl ChitinApp {
  pub fn new() -> Self {
    Self {
      summary: WorkspaceSummary::default(),
      active_activity: ActiveActivity::Files,
    }
  }
}

impl Render for ChitinApp {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
    let theme = builtins::dark();

    div()
      .flex()
      .flex_col()
      .size_full()
      .bg(theme.background.primary)
      .text_color(theme.text.primary)
      .child(render_window_bar(cx))
      .child(
        div()
          .flex()
          .flex_1()
          .min_h_0()
          .child(render_activity_bar(self.active_activity, cx))
          .child(
            div()
              .flex()
              .flex_col()
              .flex_1()
              .h_full()
              .p_8()
              .gap_4()
              .child(
                div()
                  .text_3xl()
                  .font_weight(FontWeight::SEMIBOLD)
                  .child(self.summary.product_name),
              )
              .child(
                div()
                  .text_lg()
                  .text_color(theme.text.secondary)
                  .child(self.summary.focus),
              )
              .child(
                div()
                  .mt_6()
                  .p_4()
                  .rounded_md()
                  .border_1()
                  .border_color(theme.border.primary)
                  .bg(theme.background.secondary)
                  .child(
                    div()
                      .flex()
                      .flex_col()
                      .gap_2()
                      .child(
                        div()
                          .text_lg()
                          .font_weight(FontWeight::SEMIBOLD)
                          .child(self.active_activity.title()),
                      )
                      .child(
                        div()
                          .text_sm()
                          .text_color(theme.text.secondary)
                          .child(self.active_activity.description()),
                      ),
                  ),
              ),
          ),
      )
  }
}
