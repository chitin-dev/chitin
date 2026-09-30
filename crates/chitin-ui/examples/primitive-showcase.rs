//! Interactive gallery for GPUI Kit controls used by Chitin.
//!
//! Run with `cargo run -p chitin-ui --example primitive-showcase`.

use gpui::{
  App, AppContext, Application, Bounds, Context, Entity, IntoElement, ParentElement, Render, SharedString,
  Subscription, Window, WindowBounds, WindowOptions, div, prelude::*, px, size,
};
use gpui_kit::component::{
  ActiveTheme, IndexPath,
  input::{Input, InputEvent, InputState, NumberInput},
  select::{Select, SelectEvent, SelectState},
};

use chitin_ui::assets::ChitinAssets;
use chitin_ui::primitive::progress::{Progress, ProgressLabel};

/// Retained control state; editing, navigation, and popups belong to GPUI Kit.
struct PrimitiveShowcase {
  input: Entity<InputState>,
  readonly: Entity<InputState>,
  disabled: Entity<InputState>,
  number: Entity<InputState>,
  format: Entity<SelectState<Vec<SharedString>>>,
  models: Entity<SelectState<Vec<SharedString>>>,
  status: SharedString,
  _subscriptions: Vec<Subscription>,
}

impl PrimitiveShowcase {
  /// Creates the gallery controls and subscribes to their semantic events.
  ///
  /// # Parameters
  ///
  /// * `window` supplies input and selection focus infrastructure.
  /// * `cx` owns the controls and event subscriptions.
  ///
  /// # Returns
  ///
  /// A gallery using the same editing and selection engines as the desktop.
  fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder("Enter PDB IDs, e.g. 4HHB, 1YTH"));
    let readonly = cx.new(|cx| InputState::new(window, cx).default_value("Read-only value"));
    let disabled = cx.new(|cx| InputState::new(window, cx).default_value("Disabled value"));
    let number = cx.new(|cx| InputState::new(window, cx).default_value("1.4").step(0.1).min(0.0));
    let format = cx.new(|cx| {
      SelectState::new(
        vec!["PDB".into(), "mmCIF".into()],
        Some(IndexPath::default()),
        window,
        cx,
      )
    });
    let models = cx.new(|cx| {
      SelectState::new(
        (1..=32)
          .map(|index| SharedString::from(format!("Model {index}")))
          .collect::<Vec<_>>(),
        None,
        window,
        cx,
      )
    });
    let input_subscription = cx.subscribe(&input, |this, input, event, cx| {
      this.status = match event {
        InputEvent::Change => format!("Changed: {}", input.read(cx).value()).into(),
        InputEvent::PressEnter { .. } => format!("Submitted: {}", input.read(cx).value()).into(),
        InputEvent::Focus => "Input focused".into(),
        InputEvent::Blur => "Input blurred".into(),
      };
      cx.notify();
    });
    let format_subscription = cx.subscribe(&format, |this, _, event, cx| {
      let SelectEvent::Confirm(value) = event;
      this.status = format!("Selected format: {}", value.as_deref().unwrap_or("none")).into();
      cx.notify();
    });
    let model_subscription = cx.subscribe(&models, |this, _, event, cx| {
      let SelectEvent::Confirm(value) = event;
      this.status = format!("Selected: {}", value.as_deref().unwrap_or("none")).into();
      cx.notify();
    });
    Self {
      input,
      readonly,
      disabled,
      number,
      format,
      models,
      status: "Edit a value or choose an option to inspect its events.".into(),
      _subscriptions: vec![input_subscription, format_subscription, model_subscription],
    }
  }
}

impl Render for PrimitiveShowcase {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div()
      .id("primitive-showcase")
      .size_full()
      .overflow_y_scroll()
      .bg(cx.theme().background)
      .text_color(cx.theme().foreground)
      .p_6()
      .child(
        div()
          .flex()
          .flex_col()
          .gap_4()
          .w_full()
          .max_w(px(640.0))
          .child(div().text_lg().child("GPUI Kit controls"))
          .child(
            div()
              .text_sm()
              .text_color(cx.theme().muted_foreground)
              .child(self.status.clone()),
          )
          .child(div().child("Text input").child(Input::new(&self.input).w_full()))
          .child(
            div()
              .child("Read-only input")
              .child(Input::new(&self.readonly).readonly(true).w_full()),
          )
          .child(
            div()
              .child("Disabled input")
              .child(Input::new(&self.disabled).disabled(true).w_full()),
          )
          .child(
            div()
              .child("Probe radius (Å)")
              .child(NumberInput::new(&self.number).w_full()),
          )
          .child(
            div().child("Structure format").child(
              Select::new(&self.format)
                .accessibility_label("Structure format")
                .w_full(),
            ),
          )
          .child(
            div().child("Scrollable model selector").child(
              Select::new(&self.models)
                .accessibility_label("Model")
                .placeholder("Choose a model")
                .menu_max_h(px(180.0))
                .w_full(),
            ),
          )
          .child(Progress::new(42.0).label(ProgressLabel::new("Download progress")))
          .child(
            Progress::new(0.0)
              .indeterminate()
              .label(ProgressLabel::new("Unknown download size")),
          ),
      )
  }
}

/// Opens the control gallery inside GPUI Kit's window root.
fn main() {
  env_logger::init();
  Application::new().with_assets(ChitinAssets).run(|cx: &mut App| {
    chitin_ui::init(cx);
    let bounds = Bounds::centered(None, size(px(720.0), px(760.0)), cx);
    if let Err(error) = gpui_kit::open_window(
      WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("dev.chitin.PrimitiveShowcase".into()),
        ..Default::default()
      },
      cx,
      |window, cx| cx.new(|cx| PrimitiveShowcase::new(window, cx)),
    ) {
      eprintln!("failed to open primitive showcase: {error}");
      cx.quit();
      return;
    }
    cx.activate(true);
  });
}
