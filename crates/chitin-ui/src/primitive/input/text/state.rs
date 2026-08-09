//! Persistent editing state and keyboard behavior for the text input primitive.

use std::{
  ops::Range,
  time::{Duration, Instant},
};

use gpui::{
  Bounds, Context, EntityInputHandler, EventEmitter, FocusHandle, KeyDownEvent, Pixels, Point, ShapedLine,
  SharedString, UTF16Selection, Window, point,
};

use super::{TextInputEvent, TextSelection};

/// Duration of one complete caret blink cycle.
pub const CARET_BLINK_PERIOD: Duration = Duration::from_millis(1_000);
/// Duration for which the caret is visible within a blink cycle.
pub const CARET_VISIBLE_DURATION: Duration = Duration::from_millis(500);

/// Transient shaped-text data needed by the platform text-input bridge.
#[derive(Default)]
struct TextInputLayoutCache {
  line: Option<ShapedLine>,
  bounds: Option<Bounds<Pixels>>,
}

/// Persistent state for a reusable single-line text input.
pub struct TextInputState {
  text: SharedString,
  // Selection (blue background) in text input, it has an anchor from the start
  // of our selection movement
  selection: TextSelection,
  focus_handle: FocusHandle,
  // When disabled, text input cannot be written and copied
  disabled: bool,
  // When readonly, text input cannot be written, but can be copied
  readonly: bool,
  pointer_selection_anchor: Option<usize>,
  marked_range: Option<Range<usize>>,
  layout: TextInputLayoutCache,
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
    // Because our text input can only accepts single line
    let text = normalize_single_line(text.into());
    let cursor = text.len();

    Self {
      text,
      selection: TextSelection::caret(cursor),
      focus_handle: cx.focus_handle(),
      disabled: false,
      readonly: false,
      pointer_selection_anchor: None,
      marked_range: None,
      layout: TextInputLayoutCache::default(),
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

  /// Returns whether platform text input currently has marked composition text.
  pub fn is_composing(&self) -> bool {
    self.marked_range.is_some()
  }

  /// Returns the UTF-8 byte range currently owned by the platform IME.
  pub(crate) fn marked_range(&self) -> Option<Range<usize>> {
    self.marked_range.clone()
  }

  /// Returns the selected text when selection is not empty.
  pub fn selected_text(&self) -> Option<SharedString> {
    selected_text_from(&self.text, self.selection, self.disabled)
  }

  /// Enables or disables the input and emits an availability event when changed.
  pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
    if self.disabled == disabled {
      return;
    }

    self.disabled = disabled;
    if disabled {
      self.pointer_selection_anchor = None;
      self.marked_range = None;
    }
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
    self.marked_range = None;
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

  /// Starts a pointer selection at `offset`, optionally extending the current selection.
  ///
  /// # Parameters
  ///
  /// * `offset` supplies the UTF-8 byte offset under the pointer.
  /// * `extend` retains the current selection anchor when true.
  /// * `cx` emits selection-change events and refreshes observers.
  ///
  /// # Returns
  ///
  /// `true` when the pointer selection began.
  pub(crate) fn begin_pointer_selection(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) -> bool {
    if self.disabled || !self.is_valid_offset(offset) {
      return false;
    }

    let anchor = pointer_selection_anchor(self.selection, offset, extend);
    self.pointer_selection_anchor = Some(anchor);
    self.set_selection_internal(selection_from_pointer_anchor(anchor, offset), cx);
    self.reset_caret_blink();
    true
  }

  /// Extends the active pointer selection to `offset`.
  ///
  /// # Parameters
  ///
  /// * `offset` supplies the UTF-8 byte offset under the pointer.
  /// * `cx` emits selection-change events and refreshes observers.
  ///
  /// # Returns
  ///
  /// `true` when an active pointer selection was updated.
  pub(crate) fn update_pointer_selection(&mut self, offset: usize, cx: &mut Context<Self>) -> bool {
    let Some(anchor) = self.pointer_selection_anchor else {
      return false;
    };

    if self.disabled || !self.is_valid_offset(offset) {
      return false;
    }

    self.set_selection_internal(selection_from_pointer_anchor(anchor, offset), cx);
    self.reset_caret_blink();
    true
  }

  /// Ends the active pointer-selection gesture.
  ///
  /// # Parameters
  ///
  /// This method clears the stored pointer anchor.
  ///
  /// # Returns
  ///
  /// `true` when a pointer-selection gesture was active.
  pub(crate) fn end_pointer_selection(&mut self) -> bool {
    self.pointer_selection_anchor.take().is_some()
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
    if !focused {
      self.pointer_selection_anchor = None;
      self.marked_range = None;
    }
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
        _ => key_text(event).is_some_and(|text| self.insert_text(text, cx)),
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
  /// * `forward` selects movement toward increasing byte offsets.
  /// * `extend` selects whether the existing selection anchor is retained.
  /// * `cx` emits a selection-change event when the cursor position changes.
  ///
  /// # Returns
  ///
  /// `true` after handling a valid horizontal cursor command.
  fn move_cursor(&mut self, forward: bool, extend: bool, cx: &mut Context<Self>) -> bool {
    // when we press shift, the parameter `extend` should be true, that's because
    // we need to retain the anchor and move our caret
    let selection = self.selection;
    let target = if forward {
      // function `is_collapsed()` will check whether the selection contains
      // no text
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
  /// * `target` supplies the destination UTF-8 byte offset.
  /// * `extend` selects whether the existing selection anchor is retained.
  /// * `cx` emits a selection-change event when the cursor position changes.
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
  /// * `range` identifies a valid UTF-8 range in the current text.
  /// * `replacement` supplies normalized replacement text.
  /// * `cx` emits selection and text-change events.
  ///
  /// # Returns
  ///
  /// `true` after replacing the requested text range.
  fn replace_range(&mut self, range: std::ops::Range<usize>, replacement: &str, cx: &mut Context<Self>) -> bool {
    let cursor = range.start + replacement.len();
    self.replace_text_range(range, replacement, TextSelection::caret(cursor), None, cx);
    true
  }

  /// Stores the shaped line and bounds used by the current platform input handler.
  ///
  /// # Parameters
  ///
  /// * `line` is the complete shaped text line from the current render pass.
  /// * `bounds` is the text viewport in window coordinates.
  ///
  /// # Returns
  ///
  /// This function returns `()` after replacing the transient layout cache.
  pub(crate) fn update_layout_cache(&mut self, line: ShapedLine, bounds: Bounds<Pixels>) {
    self.layout = TextInputLayoutCache {
      line: Some(line),
      bounds: Some(bounds),
    };
  }

  /// Replaces an IME-specified UTF-16 range with committed text.
  ///
  /// # Parameters
  ///
  /// * `range_utf16` optionally identifies the platform replacement range.
  /// * `text` is the committed platform text.
  /// * `cx` emits semantic text and selection changes.
  ///
  /// # Returns
  ///
  /// This function returns `()` after applying the committed replacement when editing is enabled.
  fn replace_committed_text(&mut self, range_utf16: Option<Range<usize>>, text: &str, cx: &mut Context<Self>) {
    if self.disabled || self.readonly {
      return;
    }

    let range = self.ime_replacement_range(range_utf16);
    let text = normalize_single_line(SharedString::from(text));
    let cursor = range.start + text.len();
    self.replace_text_range(range, &text, TextSelection::caret(cursor), None, cx);
  }

  /// Replaces an IME-specified UTF-16 range with marked composition text.
  ///
  /// # Parameters
  ///
  /// * `range_utf16` optionally identifies the platform replacement range.
  /// * `text` is the current composition text.
  /// * `selected_range_utf16` is the composition-local selection supplied by the platform.
  /// * `cx` emits semantic text and selection changes.
  ///
  /// # Returns
  ///
  /// This function returns `()` after applying the marked replacement when editing is enabled.
  fn replace_marked_text(
    &mut self,
    range_utf16: Option<Range<usize>>,
    text: &str,
    selected_range_utf16: Option<Range<usize>>,
    cx: &mut Context<Self>,
  ) {
    if self.disabled || self.readonly {
      return;
    }

    let range = self.ime_replacement_range(range_utf16);
    let text = normalize_single_line(SharedString::from(text));
    let selection = selected_range_utf16
      .map(|selected_range| range_from_utf16(&text, selected_range))
      .map(|selected_range| TextSelection::new(range.start + selected_range.start, range.start + selected_range.end))
      .unwrap_or_else(|| TextSelection::caret(range.start + text.len()));
    let marked_range = (!text.is_empty()).then_some(range.start..range.start + text.len());
    self.replace_text_range(range, &text, selection, marked_range, cx);
  }

  /// Resolves the text range replaced by the next platform text-input operation.
  ///
  /// # Parameters
  ///
  /// * `range_utf16` optionally identifies a platform-provided replacement range.
  ///
  /// # Returns
  ///
  /// A valid UTF-8 byte range from the explicit range, active composition, or selection.
  fn ime_replacement_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
    range_utf16
      .map(|range| range_from_utf16(&self.text, range))
      .or_else(|| self.marked_range.clone())
      .unwrap_or_else(|| self.selection.range())
  }

  /// Replaces one valid UTF-8 range while synchronizing selection and composition state.
  ///
  /// # Parameters
  ///
  /// * `range` identifies the text to replace.
  /// * `replacement` is the normalized replacement text.
  /// * `selection` is the post-replacement directional selection.
  /// * `marked_range` identifies active composition text, when present.
  /// * `cx` emits semantic updates.
  ///
  /// # Returns
  ///
  /// This function returns `()` after updating the visible text state.
  fn replace_text_range(
    &mut self,
    range: Range<usize>,
    replacement: &str,
    selection: TextSelection,
    marked_range: Option<Range<usize>>,
    cx: &mut Context<Self>,
  ) {
    let mut text = self.text.to_string();
    text.replace_range(range, replacement);
    self.text = SharedString::from(text);
    self.marked_range = marked_range;
    self.set_selection_internal(selection, cx);
    self.emit_change(cx);
  }

  /// Updates selection state and emits an event only when it changed.
  ///
  /// # Parameters
  ///
  /// * `selection` supplies the next UTF-8 byte-offset selection.
  /// * `cx` emits the selection-change event and refreshes observers.
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
  /// * `cx` emits the text-change event and refreshes observers.
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
  /// * `offset` supplies the candidate UTF-8 byte offset.
  ///
  /// # Returns
  ///
  /// `true` when the offset is valid for cursor or selection placement.
  fn is_valid_offset(&self, offset: usize) -> bool {
    offset <= self.text.len() && self.text.is_char_boundary(offset)
  }
}

impl EntityInputHandler for TextInputState {
  fn text_for_range(
    &mut self,
    range_utf16: Range<usize>,
    adjusted_range: &mut Option<Range<usize>>,
    _: &mut Window,
    _: &mut Context<Self>,
  ) -> Option<String> {
    let range = range_from_utf16(&self.text, range_utf16);
    adjusted_range.replace(range_to_utf16(&self.text, range.clone()));
    Some(self.text[range].to_string())
  }

  fn selected_text_range(
    &mut self,
    ignore_disabled_input: bool,
    _: &mut Window,
    _: &mut Context<Self>,
  ) -> Option<UTF16Selection> {
    if self.disabled && !ignore_disabled_input {
      return None;
    }

    Some(UTF16Selection {
      range: range_to_utf16(&self.text, self.selection.range()),
      reversed: self.selection.anchor() > self.selection.head(),
    })
  }

  fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
    self.marked_range.clone().map(|range| range_to_utf16(&self.text, range))
  }

  fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if self.marked_range.take().is_some() {
      cx.notify();
      window.invalidate_character_coordinates();
    }
  }

  fn replace_text_in_range(
    &mut self,
    range_utf16: Option<Range<usize>>,
    text: &str,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    self.replace_committed_text(range_utf16, text, cx);
    window.invalidate_character_coordinates();
  }

  fn replace_and_mark_text_in_range(
    &mut self,
    range_utf16: Option<Range<usize>>,
    text: &str,
    selected_range_utf16: Option<Range<usize>>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    self.replace_marked_text(range_utf16, text, selected_range_utf16, cx);
    window.invalidate_character_coordinates();
  }

  fn bounds_for_range(
    &mut self,
    range_utf16: Range<usize>,
    element_bounds: Bounds<Pixels>,
    _: &mut Window,
    _: &mut Context<Self>,
  ) -> Option<Bounds<Pixels>> {
    let line = self.layout.line.as_ref()?;
    let range = range_from_utf16(&self.text, range_utf16);
    Some(Bounds::from_corners(
      point(
        element_bounds.left() + line.x_for_index(range.start),
        element_bounds.top(),
      ),
      point(
        element_bounds.left() + line.x_for_index(range.end),
        element_bounds.bottom(),
      ),
    ))
  }

  fn character_index_for_point(
    &mut self,
    point: Point<Pixels>,
    _: &mut Window,
    _: &mut Context<Self>,
  ) -> Option<usize> {
    let bounds = self.layout.bounds?;
    let line = self.layout.line.as_ref()?;
    let point = bounds.localize(&point)?;
    Some(offset_to_utf16(&self.text, line.closest_index_for_x(point.x)))
  }

  fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
    !self.disabled && !self.readonly
  }
}

impl EventEmitter<TextInputEvent> for TextInputState {}

/// Replaces line-break characters so the control always stores one logical line.
fn normalize_single_line(text: SharedString) -> SharedString {
  if !text.contains(['\n', '\r']) {
    return text;
  }
  SharedString::from(text.replace(['\n', '\r'], " "))
}

/// Returns the selected text for one text/selection/disabled snapshot.
///
/// # Parameters
///
/// * `text` supplies the current input contents.
/// * `selection` supplies the byte-offset selection to extract.
/// * `disabled` controls whether selection should be copyable.
///
/// # Returns
///
/// The selected text, or an empty string when selection is empty or disabled.
fn selected_text_from(text: &str, selection: TextSelection, disabled: bool) -> Option<SharedString> {
  let range = selection.range();
  if !disabled && !range.is_empty() {
    Some(text[range].into())
  } else {
    Some(SharedString::from(""))
  }
}

/// Maps GPUI's normalized horizontal cursor keys to movement direction.
///
/// # Parameters
///
/// * `key` supplies the normalized GPUI key name.
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

/// Extracts text committed by an unmodified normal keyboard event.
///
/// # Parameters
///
/// * `event` supplies the focused keyboard event from GPUI.
///
/// # Returns
///
/// The committed text for direct insertion, or `None` for shortcuts and non-text keys.
fn key_text(event: &KeyDownEvent) -> Option<&str> {
  let modifiers = event.keystroke.modifiers;
  if modifiers.platform || modifiers.control || modifiers.function || modifiers.alt {
    return None;
  }

  event.keystroke.key_char.as_deref().filter(|text| !text.is_empty())
}

/// Finds the UTF-8 boundary immediately before one valid byte offset.
///
/// # Parameters
///
/// * `text` supplies the source string.
/// * `offset` supplies the current valid UTF-8 byte offset.
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
/// * `text` supplies the source string.
/// * `selection` supplies the current UTF-8 byte-offset selection.
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
/// * `text` supplies the source string.
/// * `offset` supplies the current valid UTF-8 byte offset.
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

/// Chooses the persistent anchor for a new pointer-selection gesture.
///
/// # Parameters
///
/// * `selection` supplies the existing cursor or selected range.
/// * `offset` supplies the UTF-8 byte offset under the pressed pointer.
/// * `extend` retains the existing anchor when true.
///
/// # Returns
///
/// The UTF-8 byte offset that remains fixed while the pointer moves.
fn pointer_selection_anchor(selection: TextSelection, offset: usize, extend: bool) -> usize {
  if extend { selection.anchor() } else { offset }
}

/// Builds the selection range for one pointer anchor and current pointer offset.
///
/// # Parameters
///
/// * `anchor` supplies the persistent UTF-8 byte offset from the press event.
/// * `head` supplies the current UTF-8 byte offset under the pointer.
///
/// # Returns
///
/// A directional selection that preserves the pointer gesture's anchor.
fn selection_from_pointer_anchor(anchor: usize, head: usize) -> TextSelection {
  TextSelection::new(anchor, head)
}

/// Converts one UTF-8 byte offset into a UTF-16 code-unit offset.
///
/// # Parameters
///
/// * `text` supplies the current input text.
/// * `offset` identifies a UTF-8 byte boundary in `text`.
///
/// # Returns
///
/// The matching UTF-16 code-unit offset, clamped to the text end.
fn offset_to_utf16(text: &str, offset: usize) -> usize {
  text
    .char_indices()
    .take_while(|(index, _)| *index < offset)
    .map(|(_, character)| character.len_utf16())
    .sum()
}

/// Converts one UTF-16 code-unit offset into a UTF-8 byte offset.
///
/// # Parameters
///
/// * `text` supplies the current input text.
/// * `offset` identifies a UTF-16 code-unit position in `text`.
///
/// # Returns
///
/// The first valid UTF-8 byte boundary at or after the requested position.
fn offset_from_utf16(text: &str, offset: usize) -> usize {
  let mut utf16_offset = 0;

  for (utf8_offset, character) in text.char_indices() {
    if utf16_offset >= offset {
      return utf8_offset;
    }
    utf16_offset += character.len_utf16();
  }

  text.len()
}

/// Converts one UTF-8 byte range into a UTF-16 code-unit range.
///
/// # Parameters
///
/// * `text` supplies the current input text.
/// * `range` identifies valid UTF-8 byte boundaries in `text`.
///
/// # Returns
///
/// The corresponding UTF-16 code-unit range.
fn range_to_utf16(text: &str, range: Range<usize>) -> Range<usize> {
  offset_to_utf16(text, range.start)..offset_to_utf16(text, range.end)
}

/// Converts one UTF-16 code-unit range into a UTF-8 byte range.
///
/// # Parameters
///
/// * `text` supplies the current input text.
/// * `range` identifies UTF-16 code-unit positions in `text`.
///
/// # Returns
///
/// A valid UTF-8 byte range, clamped to the input text.
fn range_from_utf16(text: &str, range: Range<usize>) -> Range<usize> {
  offset_from_utf16(text, range.start)..offset_from_utf16(text, range.end)
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
  fn key_text_should_return_unmodified_english_text() {
    let event = key_down("a", Some("a"), gpui::Modifiers::default());

    assert_eq!(key_text(&event), Some("a"));
  }

  #[test]
  fn key_text_should_reject_control_shortcuts() {
    let event = key_down(
      "c",
      Some("c"),
      gpui::Modifiers {
        control: true,
        ..Default::default()
      },
    );

    assert_eq!(key_text(&event), None);
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

  #[test]
  fn selected_text_from_should_return_selected_range() {
    assert_eq!(
      selected_text_from("workspace", TextSelection::new(0, 4), false),
      Some("work".into())
    );
  }

  #[test]
  fn selected_text_from_should_return_empty_text_when_selection_is_empty() {
    assert_eq!(
      selected_text_from("workspace", TextSelection::caret(4), false),
      Some(SharedString::from(""))
    );
  }

  #[test]
  fn selected_text_from_should_return_empty_text_when_input_is_disabled() {
    assert_eq!(
      selected_text_from("workspace", TextSelection::new(0, 4), true),
      Some(SharedString::from(""))
    );
  }

  #[test]
  fn pointer_selection_anchor_should_use_the_press_offset_without_extension() {
    assert_eq!(pointer_selection_anchor(TextSelection::new(2, 6), 4, false), 4);
  }

  #[test]
  fn pointer_selection_anchor_should_preserve_the_existing_anchor_when_extended() {
    assert_eq!(pointer_selection_anchor(TextSelection::new(2, 6), 4, true), 2);
  }

  #[test]
  fn pointer_selection_should_keep_its_anchor_while_the_head_moves() {
    assert_eq!(selection_from_pointer_anchor(2, 8), TextSelection::new(2, 8));
  }

  #[test]
  fn offset_to_utf16_should_count_surrogate_pairs() {
    assert_eq!(offset_to_utf16("a😀水", "a😀".len()), 3);
  }

  #[test]
  fn offset_from_utf16_should_preserve_utf8_character_boundaries() {
    assert_eq!(offset_from_utf16("a😀水", 2), "a😀".len());
  }

  #[test]
  fn range_from_utf16_should_convert_non_ascii_composition_ranges() {
    assert_eq!(range_from_utf16("a😀水", 1..4), 1.."a😀水".len());
  }

  fn key_down(key: &str, key_char: Option<&str>, modifiers: gpui::Modifiers) -> KeyDownEvent {
    KeyDownEvent {
      keystroke: gpui::Keystroke {
        modifiers,
        key: key.to_owned(),
        key_char: key_char.map(str::to_owned),
      },
      is_held: false,
      prefer_character_input: false,
    }
  }
}
