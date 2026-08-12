//! An interactive gallery for Chitin primitive components.
//!
//! Run with `cargo run -p chitin-ui --example primitive-showcase`.

use std::{borrow::Cow, fs, io, path::PathBuf};

use chitin_ui::{
  primitive::input::{
    number::{
      NumberDraftState, NumberFormat, NumberInput, NumberInputEvent, NumberInputSize, NumberInputState,
      NumberInputStyle, NumberInputVariant,
    },
    select::{
      Select, SelectContent, SelectContentPosition, SelectGroup, SelectInputEvent, SelectInputState, SelectInputStyle,
      SelectItem, SelectLabel, SelectOption, SelectSeparator, SelectTrigger, SelectValue,
    },
    text::{TextInput, TextInputEvent, TextInputSize, TextInputState, TextInputStyle, TextInputVariant},
  },
  themes::{UIThemes, builtins},
};
use gpui::{
  App, AppContext, Application, AssetSource, Bounds, Context, Div, Entity, IntoElement, ParentElement, Render, Result,
  SharedString, Subscription, Window, WindowBounds, WindowOptions, div, prelude::*, px, size,
};

macro_rules! subscribe_status {
  ($cx:expr, $input:expr, $status:ident, $summary:path) => {
    $cx.subscribe(&$input, |this, _, event, cx| {
      this.$status = $summary(event);
      cx.notify();
    })
  };
}

/// Creates the option data shared by the long scrollable select example.
///
/// # Parameters
///
/// This function takes no parameters.
///
/// # Returns
///
/// Twelve application-neutral options in display order.
fn long_select_options() -> Vec<SelectOption> {
  (1..=12)
    .map(|index| SelectOption::new(format!("model-{index}"), format!("Model {index}")))
    .collect()
}

/// Creates content that exceeds the showcase select popup's maximum height.
///
/// # Parameters
///
/// This function takes no parameters.
///
/// # Returns
///
/// One labelled group containing the long option list.
fn long_select_content() -> SelectContent {
  let mut group = SelectGroup::new().label(SelectLabel::new("Available models"));
  for option in long_select_options() {
    group = group.item(SelectItem::new(option.id(), option.label()));
  }
  SelectContent::new()
    .position(SelectContentPosition::ItemAligned)
    .group(group)
}

/// GPUI asset source backed by the workspace icon directory for this standalone example.
struct PrimitiveShowcaseAssets {
  base: PathBuf,
}

impl AssetSource for PrimitiveShowcaseAssets {
  /// Loads one showcase asset from the workspace asset directory.
  ///
  /// # Parameters
  ///
  /// * `path` is the asset-relative path requested by a primitive.
  ///
  /// # Returns
  ///
  /// `Ok(Some(bytes))` when the requested asset is readable, or `Ok(None)` when it is absent.
  fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
    match fs::read(self.base.join(path)) {
      Ok(data) => Ok(Some(Cow::Owned(data))),
      Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
      Err(error) => Err(error.into()),
    }
  }

  /// Lists child asset names for GPUI asset enumeration.
  ///
  /// # Parameters
  ///
  /// * `path` is the asset-relative directory path requested by GPUI.
  ///
  /// # Returns
  ///
  /// `Ok(Vec<SharedString>)` containing UTF-8 child asset names.
  fn list(&self, path: &str) -> Result<Vec<SharedString>> {
    fs::read_dir(self.base.join(path))
      .map(|entries| {
        entries
          .filter_map(|entry| {
            entry
              .ok()
              .and_then(|entry| entry.file_name().into_string().ok())
              .map(SharedString::from)
          })
          .collect()
      })
      .map_err(Into::into)
  }
}

/// State backing the primitive showcase text-input examples.
struct PrimitiveShowcase {
  // text inputs state
  default_input: Entity<TextInputState>,
  primary_input: Entity<TextInputState>,
  placeholder_input: Entity<TextInputState>,
  custom_style_input: Entity<TextInputState>,
  readonly_input: Entity<TextInputState>,
  disabled_input: Entity<TextInputState>,

  // number inputs state
  numeric_input: Entity<NumberInputState>,
  mass_input: Entity<NumberInputState>,
  precision_bounds_input: Entity<NumberInputState>,
  select_input: Entity<SelectInputState>,
  long_select_input: Entity<SelectInputState>,

  // text inputs status string representation
  default_status: SharedString,
  primary_status: SharedString,
  placeholder_status: SharedString,
  custom_style_status: SharedString,
  readonly_status: SharedString,
  disabled_status: SharedString,

  // number inputs status string representation
  numeric_status: SharedString,
  mass_status: SharedString,
  precision_bounds_status: SharedString,

  // select inputs status
  select_status: SharedString,
  long_select_status: SharedString,

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

/// Immutable display data for one numeric-input row in the primitive gallery.
struct NumberInputExample {
  title: &'static str,
  description: &'static str,
  placeholder: Option<&'static str>,
  suffix: Option<&'static str>,
  input: Entity<NumberInputState>,
  status: SharedString,
  variant: NumberInputVariant,
  size: NumberInputSize,
  style: NumberInputStyle,
}

impl PrimitiveShowcase {
  /// Creates independent text-input examples and retains semantic event subscriptions.
  ///
  /// # Parameters
  ///
  /// * `cx` allocates the input state entities and subscriptions.
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
    let numeric_input = cx.new(NumberInputState::new);
    let mass_input = cx.new(|cx| {
      let mut input = NumberInputState::new(cx);
      input.set_format(NumberFormat::Fixed { decimals: 3 });
      input.set_value(Some(58.44), cx);
      input
    });
    let precision_bounds_input = cx.new(|cx| {
      let mut input = NumberInputState::new(cx);
      input.set_format(NumberFormat::Fixed { decimals: 1 });
      input.set_maximum(Some(0.95), cx);
      input.set_step(0.1);
      input.set_value(Some(0.9), cx);
      input
    });
    let select_input = cx.new(|cx| {
      let mut input = SelectInputState::new(
        [
          SelectOption::new("amber", "Amber"),
          SelectOption::new("charmm", "CHARMM"),
          SelectOption::new("opls", "OPLS"),
          SelectOption::new("none", "None"),
        ],
        cx,
      );
      input.select("amber", cx);
      input
    });
    let long_select_input = cx.new(|cx| {
      let mut input = SelectInputState::new(long_select_options(), cx);
      input.select("model-1", cx);
      input
    });
    let input_subscriptions = vec![
      subscribe_status!(cx, default_input, default_status, text_input_event_summary),
      subscribe_status!(cx, primary_input, primary_status, text_input_event_summary),
      subscribe_status!(cx, placeholder_input, placeholder_status, text_input_event_summary),
      subscribe_status!(cx, custom_style_input, custom_style_status, text_input_event_summary),
      subscribe_status!(cx, readonly_input, readonly_status, text_input_event_summary),
      subscribe_status!(cx, disabled_input, disabled_status, text_input_event_summary),
      subscribe_status!(cx, numeric_input, numeric_status, number_input_event_summary),
      subscribe_status!(cx, mass_input, mass_status, number_input_event_summary),
      subscribe_status!(
        cx,
        precision_bounds_input,
        precision_bounds_status,
        number_input_event_summary
      ),
      subscribe_status!(cx, select_input, select_status, select_input_event_summary),
      subscribe_status!(cx, long_select_input, long_select_status, select_input_event_summary),
    ];

    Self {
      default_input,
      primary_input,
      placeholder_input,
      custom_style_input,
      readonly_input,
      disabled_input,
      numeric_input,
      mass_input,
      precision_bounds_input,
      select_input,
      long_select_input,
      default_status: "Ready".into(),
      primary_status: "Ready".into(),
      placeholder_status: "Ready".into(),
      custom_style_status: "Ready".into(),
      readonly_status: "Ready".into(),
      disabled_status: "Ready".into(),
      numeric_status: "Ready".into(),
      mass_status: "Ready".into(),
      precision_bounds_status: "Ready".into(),
      select_status: "Ready".into(),
      long_select_status: "Ready".into(),
      _input_subscriptions: input_subscriptions,
    }
  }

  /// Renders one labelled text-input example and its latest semantic event.
  ///
  /// # Parameters
  ///
  /// * `example` provides the input state, explanatory copy, and latest event.
  /// * `theme` supplies semantic colors.
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
          .variant(example.variant)
          .size(example.size)
          .style(example.style)
          .full_width(true)
          .when_some(example.placeholder, |input, placeholder| input.placeholder(placeholder)),
      )
      .child(div().text_xs().text_color(theme.text.secondary).child(example.status))
  }

  /// Renders one labelled numeric-input example and its latest semantic event.
  ///
  /// # Parameters
  ///
  /// * `example` provides numeric input state, display copy, and the latest event.
  /// * `theme` supplies semantic colors.
  ///
  /// # Returns
  ///
  /// A vertically stacked GPUI row for the requested numeric-input example.
  fn number_input_example(example: NumberInputExample, theme: UIThemes) -> Div {
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
        NumberInput::new(example.input)
          .theme(theme)
          .variant(example.variant)
          .size(example.size)
          .style(example.style)
          .full_width(true)
          .when_some(example.placeholder, |input, placeholder| input.placeholder(placeholder))
          .when_some(example.suffix, |input, suffix| input.suffix(suffix)),
      )
      .child(div().text_xs().text_color(theme.text.secondary).child(example.status))
  }

  /// Renders the text-input examples panel.
  ///
  /// # Parameters
  ///
  /// * `theme` supplies the showcase color tokens.
  ///
  /// # Returns
  ///
  /// A bordered panel containing all text-input examples.
  fn text_input_panel(&self, theme: UIThemes) -> Div {
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
            .background(theme.background.primary)
            .border(theme.border.primary)
            .focus_border(theme.accent.primary)
            .foreground(theme.text.primary)
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
      ))
  }

  /// Renders the number-input examples panel.
  ///
  /// # Parameters
  ///
  /// * `theme` supplies the showcase color tokens.
  ///
  /// # Returns
  ///
  /// A bordered panel containing all number-input examples.
  fn number_input_panel(&self, theme: UIThemes) -> Div {
    div()
      .flex()
      .flex_col()
      .gap_4()
      .p_4()
      .border_1()
      .border_color(theme.border.primary)
      .rounded_sm()
      .bg(theme.background.secondary)
      .child(div().text_sm().child("Number Input"))
      .child(Self::number_input_example(
        NumberInputExample {
          title: "Numeric draft",
          description: "Preserves incomplete numeric text while it is being edited.",
          placeholder: Some("Enter a value"),
          suffix: None,
          input: self.numeric_input.clone(),
          status: self.numeric_status.clone(),
          variant: NumberInputVariant::Secondary,
          size: NumberInputSize::Medium,
          style: NumberInputStyle::new(),
        },
        theme,
      ))
      .child(Self::number_input_example(
        NumberInputExample {
          title: "Molecular mass",
          description: "Uses fixed precision, a unit suffix, and semantic visual overrides.",
          placeholder: None,
          suffix: Some("g/mol"),
          input: self.mass_input.clone(),
          status: self.mass_status.clone(),
          variant: NumberInputVariant::Primary,
          size: NumberInputSize::Small,
          style: NumberInputStyle::new()
            .background(theme.background.primary)
            .border(theme.border.muted)
            .focus_border(theme.accent.primary)
            .suffix_foreground(theme.text.primary)
            .stepper_border(theme.border.muted)
            .stepper_hover_background(theme.background.active)
            .stepper_foreground(theme.text.primary),
        },
        theme,
      ))
      .child(Self::number_input_example(
        NumberInputExample {
          title: "Fixed precision boundary",
          description: "Starts at 0.9 with maximum 0.95, one decimal place, and a 0.1 step. \
            Increment to verify normalization occurs before the final clamp.",
          placeholder: None,
          suffix: None,
          input: self.precision_bounds_input.clone(),
          status: self.precision_bounds_status.clone(),
          variant: NumberInputVariant::Secondary,
          size: NumberInputSize::Small,
          style: NumberInputStyle::new(),
        },
        theme,
      ))
  }

  /// Renders the grouped select-input example panel.
  ///
  /// # Parameters
  ///
  /// * `theme` supplies the showcase color tokens.
  ///
  /// # Returns
  ///
  /// A bordered panel demonstrating groups, labels, separators, and popper positioning.
  fn select_input_panel(&self, theme: UIThemes) -> Div {
    div()
      .flex()
      .flex_col()
      .gap_4()
      .p_4()
      .border_1()
      .border_color(theme.border.primary)
      .rounded_sm()
      .bg(theme.background.secondary)
      .child(div().text_sm().child("Select Input"))
      .child(
        div()
          .text_xs()
          .text_color(theme.text.secondary)
          .child("ItemAligned keeps the selected item aligned with the trigger; Popper places content below it."),
      )
      .child(
        Select::new(self.select_input.clone())
          .theme(theme)
          .trigger(SelectTrigger::new().value(SelectValue::new().placeholder("Choose force field")))
          .content(
            SelectContent::new()
              .position(SelectContentPosition::ItemAligned)
              .group(
                SelectGroup::new()
                  .label(SelectLabel::new("Molecular mechanics"))
                  .item(SelectItem::new("amber", "Amber"))
                  .item(SelectItem::new("charmm", "CHARMM"))
                  .item(SelectItem::new("opls", "OPLS")),
              )
              .separator(SelectSeparator::new())
              .group(
                SelectGroup::new()
                  .label(SelectLabel::new("No force field"))
                  .item(SelectItem::new("none", "None")),
              ),
          ),
      )
      .child(
        div()
          .text_xs()
          .text_color(theme.text.secondary)
          .child(self.select_status.clone()),
      )
      .child(
        div()
          .text_sm()
          .text_color(theme.text.primary)
          .child("Scrollable options"),
      )
      .child(
        div()
          .text_xs()
          .text_color(theme.text.secondary)
          .child("Twelve options are constrained to a 240px popup to verify vertical scrolling."),
      )
      .child(
        Select::new(self.long_select_input.clone())
          .theme(theme)
          .style(SelectInputStyle::new().menu_max_height(px(240.0)))
          .trigger(SelectTrigger::new().value(SelectValue::new().placeholder("Choose model")))
          .content(long_select_content())
          .full_width(true),
      )
      .child(
        div()
          .text_xs()
          .text_color(theme.text.secondary)
          .child(self.long_select_status.clone()),
      )
  }
}

impl Render for PrimitiveShowcase {
  /// Renders the scrollable primitive component gallery.
  ///
  /// # Parameters
  ///
  /// * `_window` supplies the current GPUI window.
  /// * `_cx` supplies the showcase context.
  ///
  /// # Returns
  ///
  /// The root showcase layout containing independently rendered primitive panels.
  fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
    let theme = builtins::dark();

    div()
      .size_full()
      .flex()
      .flex_col()
      .bg(theme.background.primary)
      .text_color(theme.text.primary)
      .child(
        div()
          .id("primitive-showcase-scroll")
          .flex()
          .flex_col()
          .flex_1()
          .min_h_0()
          .overflow_y_scroll()
          .child(
            div()
              .w(px(640.0))
              .mx_auto()
              .px_6()
              .pt_6()
              .pb_48()
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
              .child(self.text_input_panel(theme))
              .child(
                div()
                  .text_sm()
                  .text_color(theme.text.secondary)
                  .child("NumberInput owns numeric drafts, parsing, and commits."),
              )
              .child(self.number_input_panel(theme))
              .child(self.select_input_panel(theme)),
          ),
      )
  }
}

/// Converts one text-input event into concise status text for the showcase.
///
/// # Parameters
///
/// * `event` is the semantic event emitted by a [`TextInputState`].
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

/// Converts one numeric-input event into concise status text for the showcase.
///
/// # Parameters
///
/// * `event` is the semantic event emitted by a [`NumberInputState`].
///
/// # Returns
///
/// A status string suitable for display beneath the input that emitted it.
fn number_input_event_summary(event: &NumberInputEvent) -> SharedString {
  match event {
    NumberInputEvent::DraftChange { draft } => format!("Draft: {draft}").into(),
    NumberInputEvent::DraftStateChange { state } => number_draft_state_summary(*state),
    NumberInputEvent::ValueChange { value } => match value {
      Some(value) => format!("Parsed: {value}").into(),
      None => "Waiting for a finite number".into(),
    },
    NumberInputEvent::Commit {
      accepted: true, draft, ..
    } => format!("Committed: {draft}").into(),
    NumberInputEvent::Commit {
      accepted: false, state, ..
    } => number_draft_state_summary(*state),
    NumberInputEvent::Cancel => "Restored the committed value".into(),
    NumberInputEvent::DisabledChange { disabled } => {
      if *disabled {
        "Disabled".into()
      } else {
        "Enabled".into()
      }
    }
    NumberInputEvent::ReadOnlyChange { readonly } => {
      if *readonly {
        "Read-only".into()
      } else {
        "Editable".into()
      }
    }
    NumberInputEvent::Focus => "Focused".into(),
    NumberInputEvent::Blur => "Blurred".into(),
  }
}

/// Converts one select event into concise status text for the showcase.
///
/// # Parameters
///
/// * `event` is the semantic event emitted by a [`SelectInputState`].
///
/// # Returns
///
/// A status string suitable for display beneath the select that emitted it.
fn select_input_event_summary(event: &SelectInputEvent) -> SharedString {
  match event {
    SelectInputEvent::SelectionChange { selected_id } => selected_id
      .as_ref()
      .map_or_else(|| "Selection cleared".into(), |id| format!("Selected: {id}").into()),
    SelectInputEvent::OpenChange { open } => {
      if *open {
        "Opened".into()
      } else {
        "Closed".into()
      }
    }
    SelectInputEvent::DisabledChange { disabled } => {
      if *disabled {
        "Disabled".into()
      } else {
        "Enabled".into()
      }
    }
    SelectInputEvent::Focus => "Focused".into(),
    SelectInputEvent::Blur => "Blurred".into(),
  }
}

/// Converts one numeric draft state into concise validation status text.
///
/// # Parameters
///
/// * `state` is the current semantic numeric draft state.
///
/// # Returns
///
/// A status string suitable for display beneath the numeric input.
fn number_draft_state_summary(state: NumberDraftState) -> SharedString {
  match state {
    NumberDraftState::Empty => "Empty".into(),
    NumberDraftState::Incomplete => "Continue entering the number".into(),
    NumberDraftState::Valid(value) => format!("Valid: {value}").into(),
    NumberDraftState::Invalid => "Enter a finite number".into(),
    NumberDraftState::OutOfRange { value } => format!("Out of range: {value}").into(),
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
  env_logger::init();
  Application::new()
    .with_assets(PrimitiveShowcaseAssets {
      base: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    })
    .run(|cx: &mut App| {
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

  #[test]
  fn number_input_event_summary_should_include_parsed_value() {
    assert_eq!(
      number_input_event_summary(&NumberInputEvent::ValueChange { value: Some(58.44) }),
      "Parsed: 58.44"
    );
  }
}
