//! An interactive gallery for Chitin primitive components.
//!
//! Run with `cargo run -p chitin-ui --example primitive-showcase`.

use chitin_ui::{
  primitive::input::text::{
    TextInput, TextInputEvent, TextInputSize, TextInputState, TextInputStyle, TextInputVariant,
  },
  themes::{UIThemes, builtins},
};
use gpui::{
  App, AppContext, Application, Bounds, Context, Div, Entity, IntoElement, ParentElement, Render, SharedString,
  Subscription, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgba, size,
};

/// State backing the primitive showcase text-input examples.
struct PrimitiveShowcase {
  default_input: Entity<TextInputState>,
  primary_input: Entity<TextInputState>,
  placeholder_input: Entity<TextInputState>,
  custom_style_input: Entity<TextInputState>,
  readonly_input: Entity<TextInputState>,
  disabled_input: Entity<TextInputState>,
  default_status: SharedString,
  primary_status: SharedString,
  placeholder_status: SharedString,
  custom_style_status: SharedString,
  readonly_status: SharedString,
  disabled_status: SharedString,
  _input_subscriptions: Vec<Subscription>,
}

/// Immutable display data for one text-input row in the primitive gallery.
struct TextInputExample {
  title: &'static str,
  description: &'static str,
  placeholder: Option<&'static str>,
  input: Entity<TextInputState>,
  status: SharedString,
  size: TextInputSize,
  variant: TextInputVariant,
  style: TextInputStyle,
}

impl PrimitiveShowcase {
  /// Creates independent text-input examples and retains semantic event subscriptions.
  ///
  /// # Parameters
  ///
  /// `cx` allocates the input state entities and subscriptions.
  ///
  /// # Returns
  ///
  /// A showcase with editable, placeholder, read-only, and disabled inputs.
  fn new(cx: &mut Context<Self>) -> Self {
    let default_input = cx.new(TextInputState::new);
    let primary_input = cx.new(|cx| TextInputState::with_text("Primary surface", cx));
    let placeholder_input = cx.new(TextInputState::new);
    let custom_style_input = cx.new(|cx| TextInputState::with_text("Styled by semantic tokens", cx));
    let readonly_input = cx.new(|cx| {
      let mut input = TextInputState::with_text("Read-only value", cx);
      input.set_readonly(true, cx);
      input
    });
    let disabled_input = cx.new(|cx| {
      let mut input = TextInputState::with_text("Disabled value", cx);
      input.set_disabled(true, cx);
      input
    });

    let default_subscription = cx.subscribe(&default_input, |this, _, event, cx| {
      this.default_status = text_input_event_summary(event);
      cx.notify();
    });
    let primary_subscription = cx.subscribe(&primary_input, |this, _, event, cx| {
      this.primary_status = text_input_event_summary(event);
      cx.notify();
    });
    let placeholder_subscription = cx.subscribe(&placeholder_input, |this, _, event, cx| {
      this.placeholder_status = text_input_event_summary(event);
      cx.notify();
    });
    let custom_style_subscription = cx.subscribe(&custom_style_input, |this, _, event, cx| {
      this.custom_style_status = text_input_event_summary(event);
      cx.notify();
    });
    let readonly_subscription = cx.subscribe(&readonly_input, |this, _, event, cx| {
      this.readonly_status = text_input_event_summary(event);
      cx.notify();
    });
    let disabled_subscription = cx.subscribe(&disabled_input, |this, _, event, cx| {
      this.disabled_status = text_input_event_summary(event);
      cx.notify();
    });

    Self {
      default_input,
      primary_input,
      placeholder_input,
      custom_style_input,
      readonly_input,
      disabled_input,
      default_status: "Ready".into(),
      primary_status: "Ready".into(),
      placeholder_status: "Ready".into(),
      custom_style_status: "Ready".into(),
      readonly_status: "Ready".into(),
      disabled_status: "Ready".into(),
      _input_subscriptions: vec![
        default_subscription,
        primary_subscription,
        placeholder_subscription,
        custom_style_subscription,
        readonly_subscription,
        disabled_subscription,
      ],
    }
  }

  /// Renders one labelled text-input example and its latest semantic event.
  ///
  /// # Parameters
  ///
  /// `example` provides the input state, explanatory copy, and latest event.
  /// `theme` supplies semantic colors.
  ///
  /// # Returns
  ///
  /// A vertically stacked GPUI row for the requested text-input example.
  fn text_input_example(example: TextInputExample, theme: UIThemes) -> Div {
    div()
      .flex()
      .flex_col()
      .gap_1()
      .child(div().text_sm().text_color(theme.text.primary).child(example.title))
      .child(
        div()
          .text_xs()
          .text_color(theme.text.secondary)
          .child(example.description),
      )
      .child(
        TextInput::new(example.input)
          .theme(theme)
          .size(example.size)
          .variant(example.variant)
          .style(example.style)
          .full_width(true)
          .when_some(example.placeholder, |input, placeholder| input.placeholder(placeholder)),
      )
      .child(div().text_xs().text_color(theme.text.secondary).child(example.status))
  }
}

impl Render for PrimitiveShowcase {
  /// Renders the text-input portion of the growing primitive component gallery.
  /// A scrollable, theme-based gallery layout.
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
    let theme = builtins::dark();

    div()
      .size_full()
      .overflow_y_scroll()
      .bg(theme.background.primary)
      .text_color(theme.text.primary)
      .overflow_y_scroll()
      .child(
        div()
          .w(px(640.0))
          .mx_auto()
          .p_6()
          .flex()
          .flex_col()
          .gap_5()
          .child(div().text_lg().child("Primitive Showcase"))
          .child(
            div()
              .text_sm()
              .text_color(theme.text.secondary)
              .child("TextInput owns editing, focus, and semantic events."),
          )
          .child(
            div()
              .flex()
              .flex_col()
              .gap_4()
              .p_4()
              .border_1()
              .border_color(theme.border.primary)
              .rounded_sm()
              .bg(theme.background.secondary)
              .child(div().text_sm().child("Text Input"))
              .child(Self::text_input_example(
                TextInputExample {
                  title: "Default",
                  description: "Editable, medium-sized input.",
                  placeholder: None,
                  input: self.default_input.clone(),
                  status: self.default_status.clone(),
                  size: TextInputSize::Medium,
                  variant: TextInputVariant::Secondary,
                  style: TextInputStyle::new(),
                },
                theme,
              ))
              .child(Self::text_input_example(
                TextInputExample {
                  title: "Primary variant",
                  description: "Uses the built-in primary input treatment.",
                  placeholder: None,
                  input: self.primary_input.clone(),
                  status: self.primary_status.clone(),
                  size: TextInputSize::Medium,
                  variant: TextInputVariant::Primary,
                  style: TextInputStyle::new(),
                },
                theme,
              ))
              .child(Self::text_input_example(
                TextInputExample {
                  title: "Placeholder",
                  description: "Shows guidance while its value is empty.",
                  placeholder: Some("Search structures"),
                  input: self.placeholder_input.clone(),
                  status: self.placeholder_status.clone(),
                  size: TextInputSize::Small,
                  variant: TextInputVariant::Secondary,
                  style: TextInputStyle::new(),
                },
                theme,
              ))
              .child(Self::text_input_example(
                TextInputExample {
                  title: "Custom style",
                  description: "Overrides visual tokens while keeping TextInput behavior.",
                  placeholder: None,
                  input: self.custom_style_input.clone(),
                  status: self.custom_style_status.clone(),
                  size: TextInputSize::Medium,
                  variant: TextInputVariant::Secondary,
                  style: TextInputStyle::new()
                    .background(rgba(0x11111bff))
                    .border(rgba(0xeff1f5ff))
                    .focus_border(rgba(0x89dcebff))
                    .foreground(rgba(0x9ca0b0ff))
                    .placeholder_foreground(theme.text.secondary)
                    .selection_background(theme.background.selection)
                    .caret(theme.accent.primary)
                    .height(px(34.0))
                    .horizontal_padding(px(12.0)),
                },
                theme,
              ))
              .child(Self::text_input_example(
                TextInputExample {
                  title: "Read-only",
                  description: "Allows selection without text mutation.",
                  placeholder: None,
                  input: self.readonly_input.clone(),
                  status: self.readonly_status.clone(),
                  size: TextInputSize::Large,
                  variant: TextInputVariant::Secondary,
                  style: TextInputStyle::new(),
                },
                theme,
              ))
              .child(Self::text_input_example(
                TextInputExample {
                  title: "Disabled",
                  description: "Does not receive focus or input events.",
                  placeholder: None,
                  input: self.disabled_input.clone(),
                  status: self.disabled_status.clone(),
                  size: TextInputSize::Medium,
                  variant: TextInputVariant::Secondary,
                  style: TextInputStyle::new(),
                },
                theme,
              )),
          ),
      )
  }
}

/// Converts one text-input event into concise status text for the showcase.
///
/// # Parameters
///
/// `event` is the semantic event emitted by a [`TextInputState`].
///
/// # Returns
///
/// A status string suitable for display beneath the input that emitted it.
fn text_input_event_summary(event: &TextInputEvent) -> SharedString {
  match event {
    TextInputEvent::Change { value } => format!("Changed: {value}").into(),
    TextInputEvent::Submit { value } => format!("Submitted: {value}").into(),
    TextInputEvent::SelectionChange { .. } => "Selection changed".into(),
    TextInputEvent::DisabledChange { disabled } => {
      if *disabled {
        "Disabled".into()
      } else {
        "Enabled".into()
      }
    }
    TextInputEvent::ReadOnlyChange { readonly } => {
      if *readonly {
        "Read-only".into()
      } else {
        "Editable".into()
      }
    }
    TextInputEvent::Cancel => "Cancelled".into(),
    TextInputEvent::Focus => "Focused".into(),
    TextInputEvent::Blur => "Blurred".into(),
  }
}

/// Opens the GPUI primitive component gallery.
///
/// # Parameters
///
/// This function takes no Rust parameters.
///
/// # Returns
///
/// This function returns after the GPUI application exits.
fn main() {
  Application::new().run(|cx: &mut App| {
    let bounds = Bounds::centered(None, size(px(720.0), px(640.0)), cx);
    let result = cx.open_window(
      WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        app_id: Some("dev.chitin.PrimitiveShowcase".to_string()),
        ..Default::default()
      },
      |_, cx| cx.new(PrimitiveShowcase::new),
    );

    if let Err(error) = result {
      eprintln!("failed to open primitive showcase: {error}");
      cx.quit();
      return;
    }

    cx.activate(true);
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn text_input_event_summary_should_include_current_value() {
    assert_eq!(
      text_input_event_summary(&TextInputEvent::Change { value: "query".into() }),
      "Changed: query"
    );
  }

  #[test]
  fn text_input_event_summary_should_include_submitted_value() {
    assert_eq!(
      text_input_event_summary(&TextInputEvent::Submit {
        value: "protein".into(),
      }),
      "Submitted: protein"
    );
  }
}
