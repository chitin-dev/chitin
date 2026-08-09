//! Persistent numeric-editing state, validation, formatting, and stepping behavior.

use gpui::{AppContext, Context, Entity, EventEmitter, KeyDownEvent, SharedString, Subscription};

use super::NumberInputEvent;
use crate::primitive::input::text::{TextInputEvent, TextInputState};

/// The semantic state of a numeric editing draft.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NumberDraftState {
  /// The editor contains no value.
  Empty,
  /// The draft can become numeric with further user input.
  Incomplete,
  /// The draft is a finite number within configured bounds.
  Valid(f64),
  /// The draft cannot become numeric without replacing existing text.
  Invalid,
  /// The draft is finite but outside configured bounds.
  OutOfRange {
    /// The parsed finite value outside the configured range.
    value: f64,
  },
}

impl NumberDraftState {
  /// Returns the finite value represented by this draft when one exists.
  ///
  /// # Parameters
  ///
  /// This method reads the draft state.
  ///
  /// # Returns
  ///
  /// The parsed value for valid and out-of-range drafts, or `None` otherwise.
  pub fn value(self) -> Option<f64> {
    match self {
      Self::Valid(value) | Self::OutOfRange { value } => Some(value),
      Self::Empty | Self::Incomplete | Self::Invalid => None,
    }
  }

  /// Returns whether this draft may replace the committed numeric value.
  ///
  /// # Parameters
  ///
  /// This method reads the draft state.
  ///
  /// # Returns
  ///
  /// `true` when the draft is empty or a valid in-range finite number.
  fn is_committable(self) -> bool {
    matches!(self, Self::Empty | Self::Valid(_))
  }

  /// Returns whether this draft should receive invalid-value visual treatment.
  ///
  /// # Parameters
  ///
  /// This method reads the draft state.
  ///
  /// # Returns
  ///
  /// `true` for invalid and out-of-range drafts.
  pub(crate) fn has_validation_error(self) -> bool {
    matches!(self, Self::Invalid | Self::OutOfRange { .. })
  }
}

/// A validated inclusive range for numeric input values.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NumberBounds {
  minimum: Option<f64>,
  maximum: Option<f64>,
}

impl NumberBounds {
  /// Creates a finite inclusive numeric range.
  ///
  /// # Parameters
  ///
  /// * `minimum` supplies an optional lower limit.
  /// * `maximum` supplies an optional upper limit.
  ///
  /// # Returns
  ///
  /// The validated range, or `None` when a bound is non-finite or inverted.
  pub fn new(minimum: Option<f64>, maximum: Option<f64>) -> Option<Self> {
    if minimum.is_some_and(|value| !value.is_finite()) || maximum.is_some_and(|value| !value.is_finite()) {
      return None;
    }
    if minimum.zip(maximum).is_some_and(|(minimum, maximum)| minimum > maximum) {
      return None;
    }

    Some(Self { minimum, maximum })
  }

  /// Returns the optional inclusive lower limit.
  ///
  /// # Parameters
  ///
  /// This method reads the configured bounds.
  ///
  /// # Returns
  ///
  /// The configured finite lower limit, when present.
  pub fn minimum(self) -> Option<f64> {
    self.minimum
  }

  /// Returns the optional inclusive upper limit.
  ///
  /// # Parameters
  ///
  /// This method reads the configured bounds.
  ///
  /// # Returns
  ///
  /// The configured finite upper limit, when present.
  pub fn maximum(self) -> Option<f64> {
    self.maximum
  }

  /// Clamps one finite value to this range.
  ///
  /// # Parameters
  ///
  /// * `value` supplies the finite value to constrain.
  ///
  /// # Returns
  ///
  /// The constrained finite value.
  fn clamp(self, value: f64) -> f64 {
    let value = self.minimum.map_or(value, |minimum| value.max(minimum));
    self.maximum.map_or(value, |maximum| value.min(maximum))
  }

  /// Returns whether a finite value is within this inclusive range.
  ///
  /// # Parameters
  ///
  /// * `value` supplies the finite value to validate.
  ///
  /// # Returns
  ///
  /// `true` when the value satisfies both configured limits.
  fn contains(self, value: f64) -> bool {
    self.minimum.is_none_or(|minimum| value >= minimum) && self.maximum.is_none_or(|maximum| value <= maximum)
  }
}

/// Formatting applied after numeric values are committed or stepped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NumberFormat {
  /// Preserve Rust's concise finite-number representation.
  #[default]
  Auto,
  /// Render exactly the requested decimal places.
  Fixed {
    /// Number of digits after the decimal point.
    decimals: usize,
  },
  /// Render scientific notation with the requested decimal places.
  Scientific {
    /// Number of digits after the decimal point.
    decimals: usize,
  },
}

/// Persistent state for a reusable numeric input.
pub struct NumberInputState {
  input: Entity<TextInputState>,
  draft: SharedString,
  draft_state: NumberDraftState,
  committed_value: Option<f64>,
  bounds: NumberBounds,
  step: f64,
  format: NumberFormat,
  disabled: bool,
  readonly: bool,
  _input_subscription: Subscription,
}

impl NumberInputState {
  /// Creates an empty numeric input with a nested text-editing primitive.
  ///
  /// # Parameters
  ///
  /// * `cx` allocates the nested input entity and event subscription.
  ///
  /// # Returns
  ///
  /// A state with no draft or committed numeric value.
  pub fn new(cx: &mut Context<Self>) -> Self {
    let input = cx.new(TextInputState::new);
    let subscription = cx.subscribe(&input, |this, _, event, cx| this.handle_text_event(event, cx));

    Self {
      input,
      draft: "".into(),
      draft_state: NumberDraftState::Empty,
      committed_value: None,
      bounds: NumberBounds::default(),
      step: 1.0,
      format: NumberFormat::default(),
      disabled: false,
      readonly: false,
      _input_subscription: subscription,
    }
  }

  /// Returns the nested input state used internally for editing and focus management.
  ///
  /// # Parameters
  ///
  /// This method reads the numeric input state.
  ///
  /// # Returns
  ///
  /// A clone of the internal text-input entity.
  pub(crate) fn text_input(&self) -> Entity<TextInputState> {
    self.input.clone()
  }

  /// Returns the current editable text, including intermediate numeric drafts.
  ///
  /// # Parameters
  ///
  /// This method reads the numeric input state.
  ///
  /// # Returns
  ///
  /// The current raw editing draft.
  pub fn draft(&self) -> &str {
    &self.draft
  }

  /// Returns the semantic classification of the current editing draft.
  ///
  /// # Parameters
  ///
  /// This method reads the numeric input state.
  ///
  /// # Returns
  ///
  /// The current numeric draft classification.
  pub fn draft_state(&self) -> NumberDraftState {
    self.draft_state
  }

  /// Returns the finite parsed value, including values currently outside configured bounds.
  ///
  /// # Parameters
  ///
  /// This method reads the numeric input state.
  ///
  /// # Returns
  ///
  /// The parsed finite value, or `None` for empty, incomplete, and invalid drafts.
  pub fn value(&self) -> Option<f64> {
    self.draft_state.value()
  }

  /// Returns the most recently committed finite value, if any.
  ///
  /// # Parameters
  ///
  /// This method reads the numeric input state.
  ///
  /// # Returns
  ///
  /// The last committed finite value, or `None` when none has been committed.
  pub fn committed_value(&self) -> Option<f64> {
    self.committed_value
  }

  /// Returns the validated numeric bounds used for direct-edit validation and stepping.
  ///
  /// # Parameters
  ///
  /// This method reads the numeric input state.
  ///
  /// # Returns
  ///
  /// The current inclusive numeric bounds.
  pub fn bounds(&self) -> NumberBounds {
    self.bounds
  }

  /// Returns the output formatting applied after commits and steps.
  ///
  /// # Parameters
  ///
  /// This method reads the numeric input state.
  ///
  /// # Returns
  ///
  /// The active numeric format.
  pub fn format(&self) -> NumberFormat {
    self.format
  }

  /// Returns whether numeric editing is disabled.
  ///
  /// # Parameters
  ///
  /// This method reads the numeric input state.
  ///
  /// # Returns
  ///
  /// `true` when the input cannot receive editing interaction.
  pub fn is_disabled(&self) -> bool {
    self.disabled
  }

  /// Returns whether numeric mutation is disabled while selection remains available.
  ///
  /// # Parameters
  ///
  /// This method reads the numeric input state.
  ///
  /// # Returns
  ///
  /// `true` when the input is read-only.
  pub fn is_readonly(&self) -> bool {
    self.readonly
  }

  /// Enables or disables numeric editing.
  ///
  /// # Parameters
  ///
  /// * `disabled` selects the editing availability.
  /// * `cx` forwards the state update to the nested text input.
  ///
  /// # Returns
  ///
  /// This function returns `()` after forwarding the availability update.
  pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
    if self.disabled == disabled {
      return;
    }

    self.disabled = disabled;
    self.input.update(cx, |input, cx| input.set_disabled(disabled, cx));
    cx.notify();
  }

  /// Enables or disables numeric text mutation while keeping selection available.
  ///
  /// # Parameters
  ///
  /// * `readonly` selects whether mutation is permitted.
  /// * `cx` forwards the state update to the nested text input.
  ///
  /// # Returns
  ///
  /// This function returns `()` after forwarding the availability update.
  pub fn set_readonly(&mut self, readonly: bool, cx: &mut Context<Self>) {
    if self.readonly == readonly {
      return;
    }

    self.readonly = readonly;
    self.input.update(cx, |input, cx| input.set_readonly(readonly, cx));
    cx.notify();
  }

  /// Replaces both numeric bounds after validating their relationship.
  ///
  /// # Parameters
  ///
  /// * `bounds` supplies a previously validated inclusive range.
  /// * `cx` emits validation changes caused by the new range.
  ///
  /// # Returns
  ///
  /// `true` when the bounds changed.
  pub fn set_bounds(&mut self, bounds: NumberBounds, cx: &mut Context<Self>) -> bool {
    if self.bounds == bounds {
      return false;
    }

    self.bounds = bounds;
    self.refresh_draft_state(cx);
    cx.notify();
    true
  }

  /// Sets an optional inclusive lower bound without allowing an inverted range.
  ///
  /// # Parameters
  ///
  /// * `minimum` supplies a finite lower bound, or clears the lower bound.
  /// * `cx` emits validation changes caused by the new range.
  ///
  /// # Returns
  ///
  /// `true` when a valid changed bound was applied.
  pub fn set_minimum(&mut self, minimum: Option<f64>, cx: &mut Context<Self>) -> bool {
    let Some(bounds) = NumberBounds::new(minimum, self.bounds.maximum()) else {
      return false;
    };

    self.set_bounds(bounds, cx)
  }

  /// Sets an optional inclusive upper bound without allowing an inverted range.
  ///
  /// # Parameters
  ///
  /// * `maximum` supplies a finite upper bound, or clears the upper bound.
  /// * `cx` emits validation changes caused by the new range.
  ///
  /// # Returns
  ///
  /// `true` when a valid changed bound was applied.
  pub fn set_maximum(&mut self, maximum: Option<f64>, cx: &mut Context<Self>) -> bool {
    let Some(bounds) = NumberBounds::new(self.bounds.minimum(), maximum) else {
      return false;
    };

    self.set_bounds(bounds, cx)
  }

  /// Sets the positive increment used by [`Self::step_by`].
  ///
  /// # Parameters
  ///
  /// * `step` supplies the new finite positive increment.
  ///
  /// # Returns
  ///
  /// `true` when a new increment was applied.
  pub fn set_step(&mut self, step: f64) -> bool {
    if !step.is_finite() || step <= 0.0 || self.step == step {
      return false;
    }

    self.step = step;
    true
  }

  /// Sets formatting used after programmatic updates, commits, and stepping.
  ///
  /// # Parameters
  ///
  /// * `format` supplies the next output format.
  ///
  /// # Returns
  ///
  /// `true` when a new format was applied.
  pub fn set_format(&mut self, format: NumberFormat) -> bool {
    if self.format == format {
      return false;
    }

    self.format = format;
    true
  }

  /// Replaces the editor with a bounded programmatic value and records it as committed.
  ///
  /// # Parameters
  ///
  /// * `value` supplies the programmatic finite value, or clears the input.
  /// * `cx` updates the nested text input and emits semantic events.
  ///
  /// # Returns
  ///
  /// `true` when the committed value or visible draft changed.
  pub fn set_value(&mut self, value: Option<f64>, cx: &mut Context<Self>) -> bool {
    let value = value
      .filter(|value| value.is_finite())
      .map(|value| self.bounds.clamp(value));
    self.set_committed_value(value, cx)
  }

  /// Steps the current value by a signed count and clamps the result to configured bounds.
  ///
  /// # Parameters
  ///
  /// * `count` supplies the signed number of configured increments to apply.
  /// * `cx` updates the nested text input and emits semantic events.
  ///
  /// # Returns
  ///
  /// `true` when a non-zero increment updated the committed numeric value.
  pub fn step_by(&mut self, count: i32, cx: &mut Context<Self>) -> bool {
    if count == 0 || self.disabled || self.readonly {
      return false;
    }

    let base = self.value().or(self.committed_value).unwrap_or(0.0);
    let value = self.bounds.clamp(base + self.step * f64::from(count));
    let value = self.normalize_stepped_value(value);
    let changed = self.set_committed_value(Some(value), cx);
    if changed {
      cx.emit(NumberInputEvent::Commit {
        draft: self.draft.clone(),
        state: self.draft_state,
        accepted: true,
      });
      cx.notify();
    }
    changed
  }

  /// Handles numeric-specific focused keyboard actions.
  ///
  /// # Parameters
  ///
  /// * `event` supplies the focused GPUI key event.
  /// * `cx` updates stepped values and emits semantic events.
  ///
  /// # Returns
  ///
  /// `true` when the number input consumed the key event.
  pub(crate) fn handle_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
    match event.keystroke.key.as_str() {
      "up" => self.step_by(1, cx),
      "down" => self.step_by(-1, cx),
      _ => false,
    }
  }

  /// Maps nested text-input events to numeric draft and commit events.
  ///
  /// # Parameters
  ///
  /// * `event` is the semantic event emitted by the nested text input.
  /// * `cx` updates numeric state and emits numeric events.
  ///
  /// # Returns
  ///
  /// This function returns `()` after handling the supported text-input event.
  fn handle_text_event(&mut self, event: &TextInputEvent, cx: &mut Context<Self>) {
    match event {
      TextInputEvent::Change { value } => self.set_draft(value.clone(), cx),
      TextInputEvent::Submit { .. } => self.commit(cx),
      TextInputEvent::Blur => {
        self.commit(cx);
        cx.emit(NumberInputEvent::Blur);
      }
      TextInputEvent::DisabledChange { disabled } => {
        cx.emit(NumberInputEvent::DisabledChange { disabled: *disabled });
      }
      TextInputEvent::ReadOnlyChange { readonly } => {
        cx.emit(NumberInputEvent::ReadOnlyChange { readonly: *readonly });
      }
      TextInputEvent::Focus => cx.emit(NumberInputEvent::Focus),
      TextInputEvent::Cancel => self.cancel(cx),
      TextInputEvent::SelectionChange { .. } => {}
    }
  }

  /// Replaces the editable draft and synchronizes its numeric classification.
  ///
  /// # Parameters
  ///
  /// * `draft` supplies the new raw numeric editing text.
  /// * `cx` emits draft and parsed-value events when their values change.
  ///
  /// # Returns
  ///
  /// This function returns `()` after synchronizing numeric draft state.
  fn set_draft(&mut self, draft: SharedString, cx: &mut Context<Self>) {
    if self.draft == draft {
      return;
    }

    self.draft = draft;
    cx.emit(NumberInputEvent::DraftChange {
      draft: self.draft.clone(),
    });
    self.refresh_draft_state(cx);
    cx.notify();
  }

  /// Reclassifies the current raw draft after text or bound changes.
  ///
  /// # Parameters
  ///
  /// * `cx` emits draft-state and parsed-value events when they change.
  ///
  /// # Returns
  ///
  /// This function returns `()` after synchronizing draft classification.
  fn refresh_draft_state(&mut self, cx: &mut Context<Self>) {
    let draft_state = classify_number_draft(&self.draft, self.bounds);
    if self.draft_state == draft_state {
      return;
    }

    let previous_value = self.draft_state.value();
    self.draft_state = draft_state;
    cx.emit(NumberInputEvent::DraftStateChange { state: draft_state });
    let value = draft_state.value();
    if previous_value != value {
      cx.emit(NumberInputEvent::ValueChange { value });
    }
  }

  /// Commits only empty or in-range finite drafts and formats accepted numeric values.
  ///
  /// # Parameters
  ///
  /// * `cx` updates the nested editor and emits the completion event.
  ///
  /// # Returns
  ///
  /// This function returns `()` after publishing commit acceptance.
  fn commit(&mut self, cx: &mut Context<Self>) {
    let accepted = if let Some(value) = committed_value_for_draft(self.draft_state) {
      self.set_committed_value(value, cx);
      true
    } else {
      false
    };

    cx.emit(NumberInputEvent::Commit {
      draft: self.draft.clone(),
      state: self.draft_state,
      accepted,
    });
    cx.notify();
  }

  /// Restores the most recently committed value and emits a cancellation event.
  ///
  /// # Parameters
  ///
  /// * `cx` updates the nested editor and emits the cancellation event.
  ///
  /// # Returns
  ///
  /// This function returns `()` after restoring the committed draft.
  fn cancel(&mut self, cx: &mut Context<Self>) {
    self.set_committed_value(self.committed_value, cx);
    cx.emit(NumberInputEvent::Cancel);
    cx.notify();
  }

  /// Replaces the committed value and synchronizes the visible formatted editor text.
  ///
  /// # Parameters
  ///
  /// * `value` supplies the next already-bounded committed value, or clears it.
  /// * `cx` updates the nested text input and emits draft-state changes.
  ///
  /// # Returns
  ///
  /// `true` when the committed value or visible draft changed.
  fn set_committed_value(&mut self, value: Option<f64>, cx: &mut Context<Self>) -> bool {
    let committed_changed = self.committed_value != value;
    self.committed_value = value;
    let draft = value.map(|value| self.format_value(value)).unwrap_or_default();
    let draft_changed = self.input.update(cx, |input, cx| input.set_text(draft, cx));
    if committed_changed && !draft_changed {
      cx.notify();
    }
    committed_changed || draft_changed
  }

  /// Normalizes stepped values to the display precision when fixed formatting is active.
  ///
  /// # Parameters
  ///
  /// * `value` supplies the bounded stepped value.
  ///
  /// # Returns
  ///
  /// The normalized value used for the next committed step.
  fn normalize_stepped_value(&self, value: f64) -> f64 {
    normalize_stepped_value(value, self.format)
  }

  /// Formats one finite value using the configured output format.
  ///
  /// # Parameters
  ///
  /// * `value` supplies the finite value to display.
  ///
  /// # Returns
  ///
  /// The formatted editor text used after commits and steps.
  fn format_value(&self, value: f64) -> SharedString {
    format_number(value, self.format)
  }
}

impl EventEmitter<NumberInputEvent> for NumberInputState {}

/// Classifies raw numeric editing text without rejecting legitimate intermediate drafts.
///
/// # Parameters
///
/// * `draft` supplies the raw editor text.
/// * `bounds` supplies the range used to classify parsed values.
///
/// # Returns
///
/// The semantic numeric state for the supplied raw draft.
fn classify_number_draft(draft: &str, bounds: NumberBounds) -> NumberDraftState {
  if draft.is_empty() {
    return NumberDraftState::Empty;
  }
  if is_incomplete_number_draft(draft) {
    return NumberDraftState::Incomplete;
  }

  let Some(value) = draft.parse::<f64>().ok().filter(|value| value.is_finite()) else {
    return NumberDraftState::Invalid;
  };
  if bounds.contains(value) {
    NumberDraftState::Valid(value)
  } else {
    NumberDraftState::OutOfRange { value }
  }
}

/// Returns whether a draft is a valid prefix of a finite decimal scientific number.
///
/// # Parameters
///
/// * `draft` supplies the raw editor text.
///
/// # Returns
///
/// `true` when additional text can complete the current numeric draft.
fn is_incomplete_number_draft(draft: &str) -> bool {
  if matches!(draft, "+" | "-" | "." | "+." | "-.") || draft.ends_with('.') {
    return true;
  }

  for suffix in ["e", "E", "e+", "e-", "E+", "E-"] {
    if let Some(mantissa) = draft.strip_suffix(suffix) {
      return !mantissa.is_empty()
        && (mantissa.parse::<f64>().ok().is_some_and(f64::is_finite) || mantissa.ends_with('.'));
    }
  }

  false
}

/// Normalizes a stepped value to fixed display precision when one is configured.
///
/// # Parameters
///
/// * `value` supplies the bounded stepped value.
/// * `format` selects whether fixed decimal normalization is required.
///
/// # Returns
///
/// The normalized stepped value.
fn normalize_stepped_value(value: f64, format: NumberFormat) -> f64 {
  let NumberFormat::Fixed { decimals } = format else {
    return value;
  };
  if decimals > 15 {
    return value;
  }

  let scale = 10_f64.powi(decimals as i32);
  (value * scale).round() / scale
}

/// Formats one finite numeric value according to a selected output format.
///
/// # Parameters
///
/// * `value` supplies the finite value to format.
/// * `format` selects the output representation.
///
/// # Returns
///
/// The formatted numeric text.
fn format_number(value: f64, format: NumberFormat) -> SharedString {
  match format {
    NumberFormat::Auto => value.to_string().into(),
    NumberFormat::Fixed { decimals } => format!("{value:.decimals$}").into(),
    NumberFormat::Scientific { decimals } => format!("{value:.decimals$e}").into(),
  }
}

/// Selects the committed value transition permitted by one numeric draft state.
///
/// # Parameters
///
/// * `state` supplies the current semantic numeric draft state.
///
/// # Returns
///
/// `Some` with the next committed value for empty and valid drafts, or `None` when rejected.
fn committed_value_for_draft(state: NumberDraftState) -> Option<Option<f64>> {
  state.is_committable().then(|| state.value())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn classify_number_draft_should_preserve_incomplete_scientific_drafts() {
    assert_eq!(
      classify_number_draft("1e-", NumberBounds::default()),
      NumberDraftState::Incomplete
    );
  }

  #[test]
  fn classify_number_draft_should_treat_trailing_decimal_as_incomplete() {
    assert_eq!(
      classify_number_draft("1.", NumberBounds::default()),
      NumberDraftState::Incomplete
    );
  }

  #[test]
  fn classify_number_draft_should_reject_non_numeric_text() {
    assert_eq!(
      classify_number_draft("hello", NumberBounds::default()),
      NumberDraftState::Invalid
    );
  }

  #[test]
  fn classify_number_draft_should_identify_out_of_range_values() {
    let bounds = NumberBounds::new(Some(0.0), Some(100.0));

    assert_eq!(
      classify_number_draft("500", bounds.unwrap_or_default()),
      NumberDraftState::OutOfRange { value: 500.0 }
    );
  }

  #[test]
  fn number_bounds_should_reject_an_inverted_range() {
    assert_eq!(NumberBounds::new(Some(100.0), Some(10.0)), None);
  }

  #[test]
  fn number_format_should_use_fixed_precision_for_stepped_values() {
    let format = NumberFormat::Fixed { decimals: 2 };
    let value = normalize_stepped_value(0.1 + 0.2, format);

    assert_eq!(value, 0.3);
  }

  #[test]
  fn format_number_should_use_scientific_notation() {
    let formatted = format_number(6.022e23, NumberFormat::Scientific { decimals: 3 });

    assert_eq!(formatted, "6.022e23");
  }

  #[test]
  fn committed_value_for_draft_should_reject_out_of_range_values() {
    assert_eq!(
      committed_value_for_draft(NumberDraftState::OutOfRange { value: 500.0 }),
      None
    );
  }

  #[test]
  fn committed_value_for_draft_should_clear_an_empty_value() {
    assert_eq!(committed_value_for_draft(NumberDraftState::Empty), Some(None));
  }
}
