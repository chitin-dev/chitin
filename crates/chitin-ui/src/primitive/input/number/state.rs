//! Persistent numeric-editing state and draft parsing helpers.

use gpui::{AppContext, Context, Entity, EventEmitter, SharedString, Subscription};

use super::NumberInputEvent;
use crate::primitive::input::text::{TextInputEvent, TextInputState};

/// Persistent state for a reusable numeric input.
pub struct NumberInputState {
  input: Entity<TextInputState>,
  draft: SharedString,
  value: Option<f64>,
  committed_value: Option<f64>,
  minimum: Option<f64>,
  maximum: Option<f64>,
  step: f64,
  precision: Option<usize>,
  _input_subscription: Subscription,
}

impl NumberInputState {
  /// Creates an empty numeric input with a nested text-editing primitive.
  ///
  /// # Parameters
  ///
  /// `cx` allocates the nested input entity and event subscription.
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
      value: None,
      committed_value: None,
      minimum: None,
      maximum: None,
      step: 1.0,
      precision: None,
      _input_subscription: subscription,
    }
  }

  /// Returns the nested input state used internally for text editing and focus management.
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

  /// Returns the current editable text, including incomplete numeric drafts.
  ///
  /// # Parameters
  ///
  /// This method reads the numeric input state.
  ///
  /// # Returns
  ///
  /// The current text draft.
  pub fn draft(&self) -> &str {
    &self.draft
  }

  /// Returns the finite value parsed from the current draft, if any.
  ///
  /// # Parameters
  ///
  /// This method reads the numeric input state.
  ///
  /// # Returns
  ///
  /// The parsed finite value, or `None` for an incomplete draft.
  pub fn value(&self) -> Option<f64> {
    self.value
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

  /// Enables or disables numeric editing.
  ///
  /// # Parameters
  ///
  /// `disabled` selects the editing availability.
  ///
  /// `cx` forwards the state update to the nested text input.
  ///
  /// # Returns
  ///
  /// This function returns `()` after forwarding the availability update.
  pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
    self.input.update(cx, |input, cx| input.set_disabled(disabled, cx));
  }

  /// Enables or disables numeric text mutation while keeping selection available.
  ///
  /// # Parameters
  ///
  /// `readonly` selects whether mutation is permitted.
  ///
  /// `cx` forwards the state update to the nested text input.
  ///
  /// # Returns
  ///
  /// This function returns `()` after forwarding the availability update.
  pub fn set_readonly(&mut self, readonly: bool, cx: &mut Context<Self>) {
    self.input.update(cx, |input, cx| input.set_readonly(readonly, cx));
  }

  /// Sets an optional inclusive minimum used when stepping values.
  ///
  /// # Parameters
  ///
  /// `minimum` supplies a finite lower bound, or clears the lower bound.
  ///
  /// # Returns
  ///
  /// This function returns `()` after replacing the configured lower bound.
  pub fn set_minimum(&mut self, minimum: Option<f64>) {
    self.minimum = minimum.filter(|value| value.is_finite());
  }

  /// Sets an optional inclusive maximum used when stepping values.
  ///
  /// # Parameters
  ///
  /// `maximum` supplies a finite upper bound, or clears the upper bound.
  ///
  /// # Returns
  ///
  /// This function returns `()` after replacing the configured upper bound.
  pub fn set_maximum(&mut self, maximum: Option<f64>) {
    self.maximum = maximum.filter(|value| value.is_finite());
  }

  /// Sets the positive increment used by [`Self::step_by`].
  ///
  /// # Parameters
  ///
  /// `step` supplies the new finite positive increment.
  ///
  /// # Returns
  ///
  /// This function returns `()` and ignores non-positive or non-finite values.
  pub fn set_step(&mut self, step: f64) {
    if step.is_finite() && step > 0.0 {
      self.step = step;
    }
  }

  /// Sets the optional fixed decimal precision used for programmatic and stepped values.
  ///
  /// # Parameters
  ///
  /// `precision` supplies the fixed decimal places, or disables fixed formatting.
  ///
  /// # Returns
  ///
  /// This function returns `()` after replacing the formatting precision.
  pub fn set_precision(&mut self, precision: Option<usize>) {
    self.precision = precision;
  }

  /// Replaces the draft with a formatted finite value or clears the input.
  ///
  /// # Parameters
  ///
  /// `value` supplies the programmatic numeric value, or clears the draft.
  ///
  /// `cx` updates the nested text input and emits semantic events.
  ///
  /// # Returns
  ///
  /// This function returns `()` after updating the editable draft.
  pub fn set_value(&mut self, value: Option<f64>, cx: &mut Context<Self>) {
    let value = value.filter(|value| value.is_finite()).map(|value| self.clamp(value));
    let draft = value.map(|value| self.format_value(value)).unwrap_or_default();
    self.input.update(cx, |input, cx| {
      input.set_text(draft, cx);
    });
  }

  /// Steps the current parsed value by a signed count, clamped to configured bounds.
  ///
  /// # Parameters
  ///
  /// `count` supplies the signed number of configured increments to apply.
  ///
  /// `cx` updates the nested text input and emits semantic events.
  ///
  /// # Returns
  ///
  /// `true` when a non-zero increment updated the draft.
  pub fn step_by(&mut self, count: i32, cx: &mut Context<Self>) -> bool {
    if count == 0 {
      return false;
    }

    let base = self.value.or(self.committed_value).unwrap_or(0.0);
    self.set_value(Some(self.clamp(base + self.step * f64::from(count))), cx);
    true
  }

  /// Maps nested text-input events to numeric draft and commit events.
  ///
  /// # Parameters
  ///
  /// `event` is the semantic event emitted by the nested text input.
  ///
  /// `cx` updates numeric state and emits numeric events.
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
      TextInputEvent::Cancel | TextInputEvent::SelectionChange { .. } => {}
    }
  }

  /// Replaces the editable draft and synchronizes its parsed value.
  ///
  /// # Parameters
  ///
  /// `draft` supplies the new raw numeric editing text.
  ///
  /// `cx` emits draft and parsed-value events when their values change.
  ///
  /// # Returns
  ///
  /// This function returns `()` after synchronizing the numeric draft state.
  fn set_draft(&mut self, draft: SharedString, cx: &mut Context<Self>) {
    if self.draft == draft {
      return;
    }

    self.draft = draft;
    cx.emit(NumberInputEvent::DraftChange {
      draft: self.draft.clone(),
    });

    let value = parse_number(&self.draft);
    if self.value != value {
      self.value = value;
      cx.emit(NumberInputEvent::ValueChange { value });
    }
    cx.notify();
  }

  /// Commits the current parsed value and emits a completion event.
  ///
  /// # Parameters
  ///
  /// `cx` emits the numeric completion event and refreshes observers.
  ///
  /// # Returns
  ///
  /// This function returns `()` after retaining a valid parsed value.
  fn commit(&mut self, cx: &mut Context<Self>) {
    if let Some(value) = self.value {
      self.committed_value = Some(value);
    }
    cx.emit(NumberInputEvent::Commit {
      draft: self.draft.clone(),
      value: self.value,
    });
    cx.notify();
  }

  /// Clamps one finite value to the configured inclusive numeric bounds.
  ///
  /// # Parameters
  ///
  /// `value` supplies the finite numeric value to constrain.
  ///
  /// # Returns
  ///
  /// The value constrained by any configured minimum and maximum.
  fn clamp(&self, value: f64) -> f64 {
    let value = self.minimum.map_or(value, |minimum| value.max(minimum));
    self.maximum.map_or(value, |maximum| value.min(maximum))
  }

  /// Formats one finite value using the configured optional decimal precision.
  ///
  /// # Parameters
  ///
  /// `value` supplies the finite numeric value to format.
  ///
  /// # Returns
  ///
  /// The formatted text used as the next editable draft.
  fn format_value(&self, value: f64) -> SharedString {
    match self.precision {
      Some(precision) => format!("{value:.precision$}").into(),
      None => value.to_string().into(),
    }
  }
}

impl EventEmitter<NumberInputEvent> for NumberInputState {}

/// Parses finite numeric input while preserving incomplete drafts for the editor.
///
/// # Parameters
///
/// `draft` supplies the raw numeric editing text.
///
/// # Returns
///
/// The finite parsed value, or `None` when the draft is incomplete or invalid.
fn parse_number(draft: &str) -> Option<f64> {
  draft.parse::<f64>().ok().filter(|value| value.is_finite())
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Verifies incomplete numeric drafts remain unparsed.
  #[test]
  fn parse_number_should_allow_incomplete_drafts() {
    assert_eq!(parse_number("-"), None);
    assert_eq!(parse_number("1."), Some(1.0));
    assert_eq!(parse_number("1e"), None);
  }

  /// Verifies non-finite numeric text is rejected.
  #[test]
  fn parse_number_should_reject_non_finite_values() {
    assert_eq!(parse_number("NaN"), None);
    assert_eq!(parse_number("inf"), None);
  }
}
