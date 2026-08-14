// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Chitin Contributors

//! Persistent selection and keyboard behavior for the selector input primitive.

use gpui::{Bounds, Context, EventEmitter, FocusHandle, KeyDownEvent, Pixels, ScrollHandle, SharedString, Size};

use super::SelectInputEvent;

/// One application-neutral option shown by a [`SelectInputState`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectOption {
  /// Stable application-owned option identifier.
  id: SharedString,
  /// Visible label rendered for the option.
  label: SharedString,
  /// Whether keyboard and pointer selection skip this option.
  disabled: bool,
}

impl SelectOption {
  /// Creates an enabled selector option.
  ///
  /// # Parameters
  ///
  /// * `id` supplies the stable application-owned identifier.
  /// * `label` supplies the visible option text.
  ///
  /// # Returns
  ///
  /// An enabled application-neutral selector option.
  pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
    Self {
      id: id.into(),
      label: label.into(),
      disabled: false,
    }
  }

  /// Sets whether the option can be highlighted or selected.
  pub fn disabled(mut self, disabled: bool) -> Self {
    self.disabled = disabled;
    self
  }

  /// Returns the stable application-owned identifier.
  pub fn id(&self) -> &str {
    &self.id
  }

  /// Returns the visible option text.
  pub fn label(&self) -> &str {
    &self.label
  }

  /// Reports whether the option is unavailable for selection.
  pub fn is_disabled(&self) -> bool {
    self.disabled
  }
}

/// Persistent interaction state for a reusable selector input.
pub struct SelectInputState {
  /// Flattened selectable options in visual content order.
  options: Vec<SelectOption>,
  /// Stable identifier of the current selection.
  selected_id: Option<SharedString>,
  /// Flattened option index used by keyboard navigation.
  highlighted_index: Option<usize>,
  /// Keyboard-highlighted option that must be scrolled into the popup viewport.
  pending_highlight_scroll: Option<usize>,
  /// Focus ownership shared by the trigger and popup interaction.
  focus_handle: FocusHandle,
  /// Persistent scroll position for the popup option viewport.
  popup_scroll_handle: ScrollHandle,
  /// Last measured window bounds of the rendered trigger.
  trigger_bounds: Option<Bounds<Pixels>>,
  /// Whether the content popup is currently visible.
  open: bool,
  /// Whether the next item-aligned render must reveal the selected option.
  align_selected_on_open: bool,
  /// Viewport used for the most recent selected-item alignment.
  alignment_viewport: Option<Size<Pixels>>,
  /// Whether all selection interaction is unavailable.
  disabled: bool,
  /// Last focus state observed by the rendered trigger.
  focused: bool,
}

impl SelectInputState {
  /// Creates a selector state from application-neutral option data.
  ///
  /// # Parameters
  ///
  /// * `options` supplies the initial option list.
  /// * `cx` allocates the selector focus handle.
  ///
  /// # Returns
  ///
  /// A closed enabled selector with no selected option.
  pub fn new(options: impl IntoIterator<Item = SelectOption>, cx: &mut Context<Self>) -> Self {
    let options = options.into_iter().collect::<Vec<_>>();
    let highlighted_index = first_enabled_option_index(&options);

    Self {
      options,
      selected_id: None,
      highlighted_index,
      pending_highlight_scroll: None,
      focus_handle: cx.focus_handle(),
      popup_scroll_handle: ScrollHandle::new(),
      trigger_bounds: None,
      open: false,
      align_selected_on_open: false,
      alignment_viewport: None,
      disabled: false,
      focused: false,
    }
  }

  /// Returns the focus handle tracked by this selector.
  pub fn focus_handle(&self) -> &FocusHandle {
    &self.focus_handle
  }

  /// Returns the scroll handle shared by popup render passes.
  pub(crate) fn popup_scroll_handle(&self) -> &ScrollHandle {
    &self.popup_scroll_handle
  }

  /// Returns the trigger bounds measured during the previous prepaint pass.
  pub(crate) fn trigger_bounds(&self) -> Option<Bounds<Pixels>> {
    self.trigger_bounds
  }

  /// Returns all current application-neutral options.
  pub fn options(&self) -> &[SelectOption] {
    &self.options
  }

  /// Returns the selected option identifier, if any.
  pub fn selected_id(&self) -> Option<&str> {
    self.selected_id.as_deref()
  }

  /// Returns the selected option, if its id remains in the option list.
  pub fn selected_option(&self) -> Option<&SelectOption> {
    self
      .selected_id
      .as_deref()
      .and_then(|id| self.options.iter().find(|option| option.id == id))
  }

  /// Reports whether the options popup is visible.
  pub fn is_open(&self) -> bool {
    self.open
  }

  /// Returns the option currently highlighted for keyboard navigation.
  pub(crate) fn highlighted_index(&self) -> Option<usize> {
    self.highlighted_index
  }

  /// Takes the keyboard-highlighted option that must be revealed in the popup.
  pub(crate) fn take_highlight_scroll_request(&mut self) -> Option<usize> {
    self.pending_highlight_scroll.take()
  }

  /// Takes the pending selected-item alignment request for one viewport.
  pub(crate) fn take_selected_alignment_request(&mut self, viewport_size: Size<Pixels>) -> bool {
    let viewport_changed = self.alignment_viewport != Some(viewport_size);
    let explicit_request = std::mem::take(&mut self.align_selected_on_open);
    let requested = explicit_request || (self.open && viewport_changed);
    if requested {
      self.alignment_viewport = Some(viewport_size);
    }
    requested
  }

  /// Stores trigger bounds when layout changes.
  pub(crate) fn set_trigger_bounds(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
    if self.trigger_bounds == Some(bounds) {
      return;
    }

    self.trigger_bounds = Some(bounds);
    if self.open {
      self.align_selected_on_open = true;
    }
    cx.notify();
  }

  /// Reports whether selector interaction is disabled.
  pub fn is_disabled(&self) -> bool {
    self.disabled
  }

  /// Replaces the displayed options while preserving a still-valid selection.
  pub fn set_options(&mut self, options: impl IntoIterator<Item = SelectOption>, cx: &mut Context<Self>) {
    let options = options.into_iter().collect::<Vec<_>>();
    // when the new replaced options are same with original options, there is no
    // need to replace and we should early return
    if self.options == options {
      return;
    }

    self.options = options;
    // first we will check whether the selected id is still valid
    let selected_is_valid = self
      .selected_id
      .as_deref()
      .is_some_and(|id| self.options.iter().any(|option| option.id == id && !option.disabled));

    if !selected_is_valid {
      // when previous selected id is invalid, set current selected id to none
      // otherwise the selected_id will keep the original value
      self.set_selected_id(None, cx);
    }
    self.highlighted_index = self
      .selected_option_index()
      .or_else(|| first_enabled_option_index(&self.options));
    cx.notify();
  }

  /// Selects an enabled option by its stable application-owned identifier.
  ///
  /// # Parameters
  ///
  /// * `id` supplies an option identifier in the current option list.
  /// * `cx` emits a selection event and refreshes observers when selection changes.
  ///
  /// # Returns
  ///
  /// `true` when the enabled option exists and becomes selected.
  pub fn select(&mut self, id: impl AsRef<str>, cx: &mut Context<Self>) -> bool {
    let Some(index) = self
      .options
      .iter()
      .position(|option| option.id == id.as_ref() && !option.disabled)
    else {
      return false;
    };

    self.highlighted_index = Some(index);
    self.set_selected_id(Some(self.options[index].id.clone()), cx);
    true
  }

  /// Updates selector availability, closing the popup and emitting semantic events when it changes.
  pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
    if self.disabled == disabled {
      return;
    }

    self.disabled = disabled;
    if disabled {
      self.set_open(false, cx);
    }
    cx.emit(SelectInputEvent::DisabledChange { disabled });
    cx.notify();
  }

  /// Synchronizes selector focus, closing the popup and emitting focus events when it changes.
  ///
  /// # Parameters
  ///
  /// * `focused` supplies the focus state observed by the rendered trigger.
  /// * `cx` closes an open popup, emits focus events, and refreshes observers.
  ///
  /// # Returns
  ///
  /// This function returns `()` after applying a changed focus state.
  pub(crate) fn sync_focus(&mut self, focused: bool, cx: &mut Context<Self>) {
    if self.focused == focused {
      return;
    }

    self.focused = focused;
    if !focused {
      self.set_open(false, cx);
    }
    cx.emit(if focused {
      SelectInputEvent::Focus
    } else {
      SelectInputEvent::Blur
    });
    cx.notify();
  }

  /// Toggles the popup and synchronizes its keyboard highlight.
  ///
  /// # Parameters
  ///
  /// * `cx` updates popup visibility and emits semantic events.
  ///
  /// # Returns
  ///
  /// `true` when interaction is enabled and the popup was toggled.
  pub(crate) fn toggle_open(&mut self, cx: &mut Context<Self>) -> bool {
    if self.disabled {
      return false;
    }

    self.set_open(!self.open, cx);
    true
  }

  /// Closes the popup when an interaction occurs outside the select.
  ///
  /// # Parameters
  ///
  /// * `cx` updates popup visibility and emits its semantic event.
  ///
  /// # Returns
  ///
  /// This function returns `()` after closing an open popup.
  pub(crate) fn dismiss_popup(&mut self, cx: &mut Context<Self>) {
    self.set_open(false, cx);
  }

  /// Handles selector keyboard interaction and reports whether it consumed the key.
  ///
  /// # Parameters
  ///
  /// * `event` supplies the normalized GPUI key event.
  /// * `cx` updates selector state and emits semantic events.
  ///
  /// # Returns
  ///
  /// `true` when the selector consumed the keyboard event.
  pub(crate) fn handle_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
    if self.disabled {
      return false;
    }

    let modifiers = &event.keystroke.modifiers;
    if modifiers.control || modifiers.alt || modifiers.platform {
      // ignore control- alt- and platform-modified keystrokes. Return false before
      // matching event.keystroke.key when any of these modifier fields is set.
      // This prevents the select from consuming application shortcuts.
      return false;
    }

    match event.keystroke.key.as_str() {
      "enter" | "space" => {
        if self.open {
          self.select_highlighted(cx);
          self.set_open(false, cx);
        } else {
          self.set_open(true, cx);
        }
        true
      }
      "escape" if self.open => {
        self.set_open(false, cx);
        true
      }
      "up" => self.move_highlight(-1, cx),
      "down" => self.move_highlight(1, cx),
      "home" if self.open => self.highlight_boundary(false, cx),
      "end" if self.open => self.highlight_boundary(true, cx),
      _ => false,
    }
  }

  /// Selects one enabled popup row and closes the popup.
  ///
  /// # Parameters
  ///
  /// * `index` supplies the clicked option index in display order.
  /// * `cx` updates selection, visibility, and emits semantic events.
  ///
  /// # Returns
  ///
  /// `true` when the clicked option is enabled and becomes selected.
  pub(crate) fn select_index(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
    let Some(option) = self.options.get(index) else {
      return false;
    };
    if option.disabled {
      return false;
    }

    self.highlighted_index = Some(index);
    self.set_selected_id(Some(option.id.clone()), cx);
    self.set_open(false, cx);
    true
  }

  /// Updates popup visibility and emits an event only when it changed.
  fn set_open(&mut self, open: bool, cx: &mut Context<Self>) {
    if self.open == open {
      return;
    }

    self.open = open;
    if open {
      self.highlighted_index = self
        .selected_option_index()
        .or_else(|| first_enabled_option_index(&self.options));
      self.align_selected_on_open = self.selected_option_index().is_some();
    } else {
      self.align_selected_on_open = false;
      self.alignment_viewport = None;
    }
    cx.emit(SelectInputEvent::OpenChange { open });
    cx.notify();
  }

  /// Updates the selected identifier and emits an event only when it changed.
  fn set_selected_id(&mut self, selected_id: Option<SharedString>, cx: &mut Context<Self>) {
    if self.selected_id == selected_id {
      return;
    }

    self.selected_id = selected_id;
    cx.emit(SelectInputEvent::SelectionChange {
      selected_id: self.selected_id.clone(),
    });
    cx.notify();
  }

  /// Moves the highlighted enabled option in one direction, opening first if needed.
  ///
  /// # Parameters
  ///
  /// * `direction` supplies `-1` for previous or `1` for next option.
  /// * `cx` updates visibility or highlight state and refreshes observers.
  ///
  /// # Returns
  ///
  /// `true` when the selector handles the directional key.
  fn move_highlight(&mut self, direction: i8, cx: &mut Context<Self>) -> bool {
    if !self.open {
      self.set_open(true, cx);
      return true;
    }

    let Some(index) = next_enabled_option_index(&self.options, self.highlighted_index, direction) else {
      return true;
    };
    if self.highlighted_index != Some(index) {
      self.highlighted_index = Some(index);
      self.pending_highlight_scroll = Some(index);
      cx.notify();
    }
    true
  }

  /// Highlights the first or last enabled popup option.
  ///
  /// # Parameters
  ///
  /// * `last` selects the last enabled option when `true`.
  /// * `cx` refreshes observers when the highlight changes.
  ///
  /// # Returns
  ///
  /// `true` after handling a popup boundary command.
  fn highlight_boundary(&mut self, last: bool, cx: &mut Context<Self>) -> bool {
    let index = if last {
      self.options.iter().rposition(|option| !option.disabled)
    } else {
      first_enabled_option_index(&self.options)
    };
    if self.highlighted_index != index {
      self.highlighted_index = index;
      self.pending_highlight_scroll = index;
      cx.notify();
    }
    true
  }

  /// Selects the current highlighted enabled option, if any.
  ///
  /// # Parameters
  ///
  /// * `cx` updates selection and emits its semantic event.
  ///
  /// # Returns
  ///
  /// `true` when a highlighted enabled option becomes selected.
  fn select_highlighted(&mut self, cx: &mut Context<Self>) -> bool {
    let Some(index) = self.highlighted_index else {
      return false;
    };
    let Some(option) = self.options.get(index) else {
      return false;
    };
    if option.disabled {
      return false;
    }

    self.set_selected_id(Some(option.id.clone()), cx);
    true
  }

  /// Returns the selected enabled option index in display order.
  fn selected_option_index(&self) -> Option<usize> {
    self.selected_id.as_deref().and_then(|id| {
      self
        .options
        .iter()
        .position(|option| option.id == id && !option.disabled)
    })
  }
}

impl EventEmitter<SelectInputEvent> for SelectInputState {}

/// Finds the first enabled option in one display-ordered option list.
///
/// # Parameters
///
/// * `options` supplies the display-ordered selector options.
///
/// # Returns
///
/// The first enabled option index, or `None` when every option is disabled.
fn first_enabled_option_index(options: &[SelectOption]) -> Option<usize> {
  options.iter().position(|option| !option.disabled)
}

/// Finds the next enabled option, wrapping around the display-ordered list.
///
/// # Parameters
///
/// * `options` supplies the display-ordered selector options.
/// * `current` supplies the optional current highlighted option index.
/// * `direction` supplies `-1` for previous or `1` for next navigation.
///
/// # Returns
///
/// The next enabled option index, or `None` when none are enabled.
fn next_enabled_option_index(options: &[SelectOption], current: Option<usize>, direction: i8) -> Option<usize> {
  let enabled_count = options.iter().filter(|option| !option.disabled).count();
  if enabled_count == 0 {
    return None;
  }

  let start = current.unwrap_or_else(|| {
    if direction < 0 {
      0
    } else {
      options.len().saturating_sub(1)
    }
  });
  for offset in 1..=options.len() {
    let index = if direction < 0 {
      (start + options.len() - offset % options.len()) % options.len()
    } else {
      (start + offset) % options.len()
    };
    if !options[index].disabled {
      return Some(index);
    }
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;

  fn options() -> Vec<SelectOption> {
    vec![
      SelectOption::new("amber", "Amber"),
      SelectOption::new("charmm", "CHARMM").disabled(true),
      SelectOption::new("opls", "OPLS"),
    ]
  }

  /// Verifies option navigation skips disabled entries and wraps.
  #[test]
  fn next_enabled_option_index_should_skip_disabled_options_and_wrap() {
    let options = options();

    assert_eq!(next_enabled_option_index(&options, Some(0), 1), Some(2));
    assert_eq!(next_enabled_option_index(&options, Some(0), -1), Some(2));
  }

  /// Verifies selector navigation has no destination when all options are disabled.
  #[test]
  fn next_enabled_option_index_should_return_none_when_all_options_are_disabled() {
    let options = vec![SelectOption::new("disabled", "Disabled").disabled(true)];

    assert_eq!(next_enabled_option_index(&options, None, 1), None);
  }
}
