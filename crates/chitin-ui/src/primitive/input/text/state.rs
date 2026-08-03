//! Persistent editing state and keyboard behavior for the text input primitive.

use std::time::{Duration, Instant};

use gpui::{Context, EventEmitter, FocusHandle, KeyDownEvent, SharedString};

use super::{TextInputEvent, TextSelection};

/// Duration of one complete caret blink cycle.
pub const CARET_BLINK_PERIOD: Duration = Duration::from_millis(1_000);
/// Duration for which the caret is visible within a blink cycle.
pub const CARET_VISIBLE_DURATION: Duration = Duration::from_millis(500);

/// Persistent state for a reusable single-line text input.
pub struct TextInputState {
  text: SharedString,
  selection: TextSelection,
  focus_handle: FocusHandle,
  disabled: bool,
  readonly: bool,
  focused: bool,
  caret_epoch: Instant,
}

impl TextInputState {
  /// Creates an empty input state with a focus handle allocated by GPUI.
  pub fn new(cx: &mut Context<Self>) -> Self {
    Self::with_text("", cx)
  }

  /// Creates an input state initialized with `text` and a caret at its end.
  pub fn with_text(text: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
    let text = normalize_single_line(text.into());
    let cursor = text.len();

    Self {
      text,
      selection: TextSelection::caret(cursor),
      focus_handle: cx.focus_handle(),
      disabled: false,
      readonly: false,
      focused: false,
      caret_epoch: Instant::now(),
    }
  }

  /// Returns the complete editable text.
  pub fn text(&self) -> &str {
    &self.text
  }

  /// Returns the current cursor and selection state.
  pub fn selection(&self) -> TextSelection {
    self.selection
  }

  /// Returns the focus handle tracked by this input.
  pub fn focus_handle(&self) -> &FocusHandle {
    &self.focus_handle
  }

  /// Returns whether editing and keyboard focus are disabled.
  pub fn is_disabled(&self) -> bool {
    self.disabled
  }

  /// Returns whether text mutation is disabled while selection remains available.
  pub fn is_readonly(&self) -> bool {
    self.readonly
  }

  /// Returns whether this input currently owns keyboard focus.
  pub fn is_focused(&self) -> bool {
    self.focused
  }

  /// Enables or disables the input and emits an availability event when changed.
  pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
    if self.disabled == disabled {
      return;
    }

    self.disabled = disabled;
    cx.emit(TextInputEvent::DisabledChange { disabled });
    cx.notify();
  }

  /// Enables or disables read-only mode and emits an availability event when changed.
  pub fn set_readonly(&mut self, readonly: bool, cx: &mut Context<Self>) {
    if self.readonly == readonly {
      return;
    }

    self.readonly = readonly;
    cx.emit(TextInputEvent::ReadOnlyChange { readonly });
    cx.notify();
  }

  /// Replaces the complete value and moves the caret to the end.
  pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) -> bool {
    let text = normalize_single_line(text.into());
    if self.text == text {
      return false;
    }

    self.text = text;
    self.set_selection_internal(TextSelection::caret(self.text.len()), cx);
    self.emit_change(cx);
    true
  }

  /// Removes the complete value.
  pub fn clear(&mut self, cx: &mut Context<Self>) -> bool {
    self.set_text("", cx)
  }

  /// Selects the complete current value.
  pub fn select_all(&mut self, cx: &mut Context<Self>) {
    self.set_selection_internal(TextSelection::new(0, self.text.len()), cx);
  }

  /// Moves the caret to `offset` when it is a valid UTF-8 boundary.
  pub fn set_cursor(&mut self, offset: usize, cx: &mut Context<Self>) -> bool {
    if self.disabled || !self.is_valid_offset(offset) {
      return false;
    }

    self.set_selection_internal(TextSelection::caret(offset), cx);
    true
  }

  /// Selects the half-open text interval defined by `anchor` and `head`.
  pub fn set_selection(&mut self, anchor: usize, head: usize, cx: &mut Context<Self>) -> bool {
    if self.disabled || !self.is_valid_offset(anchor) || !self.is_valid_offset(head) {
      return false;
    }

    self.set_selection_internal(TextSelection::new(anchor, head), cx);
    true
  }

  /// Replaces the current selection with one logical line of `inserted` text.
  pub fn insert_text(&mut self, inserted: &str, cx: &mut Context<Self>) -> bool {
    if self.disabled || self.readonly {
      return false;
    }

    let inserted = normalize_single_line(SharedString::from(inserted));
    let range = self.selection.range();
    if range.is_empty() && inserted.is_empty() {
      return false;
    }

    let mut text = self.text.to_string();
    text.replace_range(range.clone(), &inserted);
    self.text = SharedString::from(text);
    self.set_selection_internal(TextSelection::caret(range.start + inserted.len()), cx);
    self.emit_change(cx);
    true
  }

  /// Deletes the selection or one complete Unicode scalar before the caret.
  pub fn delete_backward(&mut self, cx: &mut Context<Self>) -> bool {
    if self.disabled || self.readonly {
      return false;
    }

    let Some(range) = backward_deletion_range(&self.text, self.selection) else {
      return false;
    };

    self.replace_range(range, "", cx)
  }

  /// Deletes the selection or one complete Unicode scalar after the caret.
  pub fn delete_forward(&mut self, cx: &mut Context<Self>) -> bool {
    if self.disabled || self.readonly {
      return false;
    }

    let range = self.selection.range();
    let range = if range.is_empty() {
      let Some(end) = next_char_boundary(&self.text, range.end) else {
        return false;
      };
      range.end..end
    } else {
      range
    };

    self.replace_range(range, "", cx)
  }

  /// Synchronizes focus state from the rendered GPUI focus handle.
  pub(crate) fn sync_focus(&mut self, focused: bool, cx: &mut Context<Self>) {
    if self.focused == focused {
      return;
    }

    self.focused = focused;
    self.reset_caret_blink();
    cx.emit(if focused {
      TextInputEvent::Focus
    } else {
      TextInputEvent::Blur
    });
    cx.notify();
  }

  /// Returns whether the caret is currently in its visible blink phase.
  pub(crate) fn caret_visible(&self, now: Instant) -> bool {
    let elapsed = now.saturating_duration_since(self.caret_epoch);
    elapsed.as_millis() % CARET_BLINK_PERIOD.as_millis() < CARET_VISIBLE_DURATION.as_millis()
  }

  /// Applies a focused key event and returns whether this input consumed it.
  pub(crate) fn handle_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
    if self.disabled {
      return false;
    }

    let handled = if let Some(forward) = cursor_direction(event.keystroke.key.as_str()) {
      self.move_cursor(forward, event.keystroke.modifiers.shift, cx)
    } else {
      match event.keystroke.key.as_str() {
        "backspace" => self.delete_backward(cx),
        "delete" => self.delete_forward(cx),
        "enter" => {
          cx.emit(TextInputEvent::Submit {
            value: self.text.clone(),
          });
          cx.notify();
          true
        }
        "escape" => {
          cx.emit(TextInputEvent::Cancel);
          cx.notify();
          true
        }
        "home" => self.move_to_boundary(0, event.keystroke.modifiers.shift, cx),
        "end" => self.move_to_boundary(self.text.len(), event.keystroke.modifiers.shift, cx),
        "a" if event.keystroke.modifiers.platform || event.keystroke.modifiers.control => {
          self.select_all(cx);
          true
        }
        _ => printable_character(event).is_some_and(|character| self.insert_text(character, cx)),
      }
    };

    if handled {
      self.reset_caret_blink();
    }
    handled
  }

  /// Moves or extends the selection by one complete Unicode scalar.
  ///
  /// # Parameters
  ///
  /// `forward` selects movement toward increasing byte offsets.
  ///
  /// `extend` selects whether the existing selection anchor is retained.
  ///
  /// `cx` emits a selection-change event when the cursor position changes.
  ///
  /// # Returns
  ///
  /// `true` after handling a valid horizontal cursor command.
  fn move_cursor(&mut self, forward: bool, extend: bool, cx: &mut Context<Self>) -> bool {
    let selection = self.selection;
    let target = if forward {
      if !extend && !selection.is_collapsed() {
        selection.range().end
      } else {
        next_char_boundary(&self.text, selection.head()).unwrap_or(selection.head())
      }
    } else if !extend && !selection.is_collapsed() {
      selection.range().start
    } else {
      previous_char_boundary(&self.text, selection.head()).unwrap_or(selection.head())
    };

    let anchor = if extend { selection.anchor() } else { target };
    self.set_selection_internal(TextSelection::new(anchor, target), cx);
    true
  }

  /// Moves or extends the selection to one validated text boundary.
  ///
  /// # Parameters
  ///
  /// `target` supplies the destination UTF-8 byte offset.
  ///
  /// `extend` selects whether the existing selection anchor is retained.
  ///
  /// `cx` emits a selection-change event when the cursor position changes.
  ///
  /// # Returns
  ///
  /// `true` after handling the boundary movement command.
  fn move_to_boundary(&mut self, target: usize, extend: bool, cx: &mut Context<Self>) -> bool {
    let anchor = if extend { self.selection.anchor() } else { target };
    self.set_selection_internal(TextSelection::new(anchor, target), cx);
    true
  }

  /// Replaces one selected byte range and moves the cursor after the replacement.
  ///
  /// # Parameters
  ///
  /// `range` identifies a valid UTF-8 range in the current text.
  ///
  /// `replacement` supplies normalized replacement text.
  ///
  /// `cx` emits selection and text-change events.
  ///
  /// # Returns
  ///
  /// `true` after replacing the requested text range.
  fn replace_range(&mut self, range: std::ops::Range<usize>, replacement: &str, cx: &mut Context<Self>) -> bool {
    let mut text = self.text.to_string();
    text.replace_range(range.clone(), replacement);
    self.text = SharedString::from(text);
    self.set_selection_internal(TextSelection::caret(range.start + replacement.len()), cx);
    self.emit_change(cx);
    true
  }

  /// Updates selection state and emits an event only when it changed.
  ///
  /// # Parameters
  ///
  /// `selection` supplies the next UTF-8 byte-offset selection.
  ///
  /// `cx` emits the selection-change event and refreshes observers.
  ///
  /// # Returns
  ///
  /// This function returns `()` after applying a changed selection.
  fn set_selection_internal(&mut self, selection: TextSelection, cx: &mut Context<Self>) {
    if self.selection == selection {
      return;
    }

    self.selection = selection;
    cx.emit(TextInputEvent::SelectionChange { selection });
    cx.notify();
  }

  /// Emits a semantic event containing the current complete text value.
  ///
  /// # Parameters
  ///
  /// `cx` emits the text-change event and refreshes observers.
  ///
  /// # Returns
  ///
  /// This function returns `()` after publishing the current text value.
  fn emit_change(&self, cx: &mut Context<Self>) {
    cx.emit(TextInputEvent::Change {
      value: self.text.clone(),
    });
    cx.notify();
  }

  /// Restarts the caret blink interval after a handled interaction.
  ///
  /// # Parameters
  ///
  /// This method mutates the input state's blink epoch.
  ///
  /// # Returns
  ///
  /// This function returns `()` after recording the current instant.
  fn reset_caret_blink(&mut self) {
    self.caret_epoch = Instant::now();
  }

  /// Reports whether one byte offset is within the current text at a UTF-8 boundary.
  ///
  /// # Parameters
  ///
  /// `offset` supplies the candidate UTF-8 byte offset.
  ///
  /// # Returns
  ///
  /// `true` when the offset is valid for cursor or selection placement.
  fn is_valid_offset(&self, offset: usize) -> bool {
    offset <= self.text.len() && self.text.is_char_boundary(offset)
  }
}

impl EventEmitter<TextInputEvent> for TextInputState {}

/// Replaces line-break characters so the control always stores one logical line.
///
/// # Parameters
///
/// `text` supplies raw text entered or assigned by a caller.
///
/// # Returns
///
/// A text value with carriage returns and line feeds replaced by spaces.
fn normalize_single_line(text: SharedString) -> SharedString {
  if !text.contains(['\n', '\r']) {
    return text;
  }
  SharedString::from(text.replace(['\n', '\r'], " "))
}

/// Extracts printable input from an unmodified GPUI key event.
///
/// # Parameters
///
/// `event` supplies the key event to inspect.
///
/// # Returns
///
/// The printable character sequence, or `None` when modifiers reserve the key.
fn printable_character(event: &KeyDownEvent) -> Option<&str> {
  let modifiers = event.keystroke.modifiers;
  if modifiers.control || modifiers.platform || modifiers.alt || modifiers.function {
    return None;
  }

  event
    .keystroke
    .key_char
    .as_deref()
    .or_else(|| (event.keystroke.key.len() == 1).then_some(event.keystroke.key.as_str()))
}

/// Maps GPUI's normalized horizontal cursor keys to movement direction.
///
/// # Parameters
///
/// `key` supplies the normalized GPUI key name.
///
/// # Returns
///
/// `Some(true)` for right, `Some(false)` for left, or `None` for other keys.
fn cursor_direction(key: &str) -> Option<bool> {
  match key {
    "left" => Some(false),
    "right" => Some(true),
    _ => None,
  }
}

/// Finds the UTF-8 boundary immediately before one valid byte offset.
///
/// # Parameters
///
/// `text` supplies the source string.
///
/// `offset` supplies the current valid UTF-8 byte offset.
///
/// # Returns
///
/// The preceding boundary, or `None` at the beginning of the text.
fn previous_char_boundary(text: &str, offset: usize) -> Option<usize> {
  text[..offset].char_indices().next_back().map(|(index, _)| index)
}

/// Returns the text range Backspace should remove for one selection state.
///
/// # Parameters
///
/// `text` supplies the source string.
///
/// `selection` supplies the current UTF-8 byte-offset selection.
///
/// # Returns
///
/// The selected range or preceding scalar range, or `None` at text start.
fn backward_deletion_range(text: &str, selection: TextSelection) -> Option<std::ops::Range<usize>> {
  let range = selection.range();
  if !range.is_empty() {
    return Some(range);
  }

  previous_char_boundary(text, range.start).map(|start| start..range.start)
}

/// Finds the UTF-8 boundary immediately after one valid byte offset.
///
/// # Parameters
///
/// `text` supplies the source string.
///
/// `offset` supplies the current valid UTF-8 byte offset.
///
/// # Returns
///
/// The following boundary, or `None` at the end of the text.
fn next_char_boundary(text: &str, offset: usize) -> Option<usize> {
  text[offset..]
    .chars()
    .next()
    .map(|character| offset + character.len_utf8())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn normalize_single_line_should_replace_line_breaks() {
    assert_eq!(normalize_single_line("a\nb\rc".into()), "a b c");
  }

  #[test]
  fn previous_char_boundary_should_preserve_unicode_scalars() {
    assert_eq!(previous_char_boundary("a水", "a水".len()), Some(1));
  }

  #[test]
  fn next_char_boundary_should_preserve_unicode_scalars() {
    assert_eq!(next_char_boundary("水a", 0), Some("水".len()));
  }

  #[test]
  fn cursor_direction_should_use_gpui_normalized_arrow_keys() {
    assert_eq!(cursor_direction("left"), Some(false));
    assert_eq!(cursor_direction("right"), Some(true));
    assert_eq!(cursor_direction("arrowleft"), None);
  }

  #[test]
  fn delete_backward_at_text_start_should_preserve_text() {
    let text = "workspace";
    let selection = TextSelection::caret(0);
    let mut updated = text.to_owned();

    if let Some(range) = backward_deletion_range(text, selection) {
      updated.replace_range(range, "");
    }

    assert_eq!(updated, text);
    assert_eq!(selection, TextSelection::caret(0));
  }
}
