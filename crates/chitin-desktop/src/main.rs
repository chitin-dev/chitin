#![forbid(unsafe_code)]

use chitin_core::WorkspaceSummary;
use gpui::{
  App, Application, Bounds, Context, FontWeight, Render, Window, WindowBounds, WindowOptions, div,
  prelude::*, px, rgb, size,
};

struct ChitinApp {
  summary: WorkspaceSummary,
}

impl Render for ChitinApp {
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl gpui::IntoElement {
    div()
      .flex()
      .flex_col()
      .size_full()
      .bg(rgb(0x101419))
      .text_color(rgb(0xe8eef5))
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
          .text_color(rgb(0xaebdcc))
          .child(self.summary.focus),
      )
      .child(
        div()
          .mt_6()
          .p_4()
          .rounded_md()
          .border_1()
          .border_color(rgb(0x2b3744))
          .bg(rgb(0x161d24))
          .child("Agent-native desktop shell initialized with GPUI."),
      )
  }
}

fn main() {
  Application::new().run(|cx: &mut App| {
    let bounds = Bounds::centered(None, size(px(1100.0), px(760.0)), cx);
    let result = cx.open_window(
      WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        ..Default::default()
      },
      |_, cx| {
        cx.new(|_| ChitinApp {
          summary: WorkspaceSummary::default(),
        })
      },
    );

    if let Err(error) = result {
      eprintln!("failed to open Chitin desktop window: {error}");
      cx.quit();
      return;
    }

    cx.activate(true);
  });
}
