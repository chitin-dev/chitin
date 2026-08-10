// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Chitin Contributors

//! Rendering and composition for the select input primitive.

use gpui::{
  Anchored, AnchoredPositionMode, App, Corner, CursorStyle, Div, Entity, InteractiveElement, IntoElement, MouseButton,
  ParentElement, Pixels, RenderOnce, SharedString, Size, Window, anchored, deferred, div, point, prelude::*, px,
};

use super::{SelectInputState, SelectOption};
use crate::{
  primitive::icon::Icon,
  themes::{UIThemes, builtins},
};

/// Default width of selector popup
const DEFAULT_SELECT_WIDTH: Pixels = px(240.0);
/// Default horizontal padding
const DEFAULT_PADDING_X: Pixels = px(8.0);
/// Default max height of selector popup menu, when the height is greater than
/// this value, the menu should be scrollable
const DEFAULT_MENU_MAX_HEIGHT: Pixels = px(240.0);
/// Default height of each item
const ITEM_HEIGHT: Pixels = px(30.0);
/// Default height of each label in the items, notice it's different from the
/// font size of label in the items.
const LABEL_HEIGHT: Pixels = px(24.0);
/// Default height of separator
const SEPARATOR_HEIGHT: Pixels = px(9.0);

/// Built-in theme-based appearance variants for a [`Select`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectInputVariant {
  /// A bordered selector for standard forms and property panels.
  #[default]
  Secondary,
  /// A low-chrome selector for dense toolbars and inline controls.
  Transparent,
}

/// Visual-only overrides for a [`Select`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SelectInputStyle {
  /// Explicit trigger width, when visual layout requires one.
  width: Option<Pixels>,
  /// Maximum popup height before its content scrolls.
  menu_max_height: Option<Pixels>,
}

impl SelectInputStyle {
  /// Creates a selector style with no visual overrides.
  pub fn new() -> Self {
    Self::default()
  }

  /// Sets an explicit visual width for the selector control.
  pub fn width(mut self, width: Pixels) -> Self {
    self.width = Some(width);
    self
  }

  /// Sets the maximum visible popup height before its option list scrolls.
  pub fn menu_max_height(mut self, height: Pixels) -> Self {
    self.menu_max_height = Some(height);
    self
  }
}

/// Visual size for a [`Select`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectInputSize {
  /// Compact selector for dense toolbars.
  Small,
  /// Default selector for sidebars and property panels.
  #[default]
  Medium,
  /// Taller selector for prominent forms.
  Large,
}

/// Positions a [`SelectContent`] relative to its [`SelectTrigger`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectContentPosition {
  /// Vertically aligns the selected item with the trigger.
  #[default]
  ItemAligned,
  /// Places the content below the trigger.
  Popper,
}

/// Root select component that owns trigger, content, and interaction state.
#[derive(IntoElement)]
pub struct Select {
  /// Persistent selection, focus, and popup interaction state.
  state: Entity<SelectInputState>,
  /// Trigger subtree that owns the selected-value display.
  trigger: SelectTrigger,
  /// Declarative popup hierarchy that supplies groups and selectable items.
  content: SelectContent,
  /// Semantic color palette used by trigger and content.
  theme: UIThemes,
  /// Built-in trigger appearance treatment.
  variant: SelectInputVariant,
  /// Component-specific visual overrides.
  style: SelectInputStyle,
  /// Trigger height and text-size selection.
  size: SelectInputSize,
  /// Whether the trigger adopts its parent's available width.
  full_width: bool,
}

impl Select {
  /// Creates a select with an empty trigger and content hierarchy.
  pub fn new(state: Entity<SelectInputState>) -> Self {
    Self {
      state,
      trigger: SelectTrigger::new(),
      content: SelectContent::new(),
      theme: builtins::dark(),
      variant: SelectInputVariant::default(),
      style: SelectInputStyle::default(),
      size: SelectInputSize::default(),
      full_width: false,
    }
  }

  /// Sets the trigger subtree.
  pub fn trigger(mut self, trigger: SelectTrigger) -> Self {
    self.trigger = trigger;
    self
  }

  /// Sets the content subtree.
  pub fn content(mut self, content: SelectContent) -> Self {
    self.content = content;
    self
  }

  /// Sets the semantic theme used for this select.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Sets one built-in theme-based appearance variant.
  pub fn variant(mut self, variant: SelectInputVariant) -> Self {
    self.variant = variant;
    self
  }

  /// Applies visual-only select appearance overrides.
  pub fn style(mut self, style: SelectInputStyle) -> Self {
    self.style = style;
    self
  }

  /// Sets the visual size.
  pub fn size(mut self, size: SelectInputSize) -> Self {
    self.size = size;
    self
  }

  /// Makes the trigger fill its parent width when enabled.
  pub fn full_width(mut self, full_width: bool) -> Self {
    self.full_width = full_width;
    self
  }
}

impl RenderOnce for Select {
  /// Renders the trigger and, when open, its deferred content overlay.
  ///
  /// # Parameters
  ///
  /// * `window` reports the trigger's focus state and receives focus requests.
  /// * `cx` synchronizes selection state and registers interaction handlers.
  ///
  /// # Returns
  ///
  /// A trigger plus an optional content overlay that paints above the main scene.
  fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    // The declarative content tree is the source of truth for display order.
    // `set_options` is a no-op when this render observes the same item list.
    let options = self.content.options();
    self.state.update(cx, |state, cx| state.set_options(options, cx));

    let metrics = self.size.metrics();
    let focus_handle = self.state.read(cx).focus_handle().clone();
    let focused = focus_handle.is_focused(window);
    self.state.update(cx, |state, cx| state.sync_focus(focused, cx));

    let (selected_id, selected_label, highlighted_index, open, disabled) = {
      let state = self.state.read(cx);
      (
        state.selected_id().map(SharedString::from),
        state.selected_option().map(|option| SharedString::from(option.label())),
        state.highlighted_index(),
        state.is_open(),
        state.is_disabled(),
      )
    };
    let colors = SelectInputColors::new(self.theme, self.variant, disabled);
    let pointer_state = self.state.clone();
    let key_state = self.state.clone();
    let backdrop_state = self.state.clone();
    let width = self.style.width.unwrap_or(DEFAULT_SELECT_WIDTH);
    let popup_layout = SelectPopupLayout {
      trigger: metrics,
      width,
    };
    let value = self.trigger.value;
    let viewport_size = window.viewport_size();

    div()
      .relative()
      .w(width)
      .when(self.full_width, |style| style.w_full())
      .child(
        div()
          .flex()
          .items_center()
          .justify_between()
          .h(metrics.height)
          .px(DEFAULT_PADDING_X)
          .rounded_sm()
          .border_1()
          .border_color(colors.border)
          .bg(colors.background)
          .text_size(metrics.font_size)
          .cursor(if disabled {
            CursorStyle::Arrow
          } else {
            CursorStyle::PointingHand
          })
          .when(!disabled, |style| {
            style
              .hover(move |style| style.bg(colors.hover_background))
              .focus(move |style| style.border_color(colors.focus_border))
          })
          .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            let focus_handle = pointer_state.read(cx).focus_handle().clone();
            if pointer_state.update(cx, |state, cx| state.toggle_open(cx)) {
              window.focus(&focus_handle, cx);
              cx.stop_propagation();
            }
          })
          .on_key_down(move |event, _, cx| {
            if key_state.update(cx, |state, cx| state.handle_key_down(event, cx)) {
              cx.stop_propagation();
            }
          })
          .track_focus(&focus_handle)
          .child(render_value(
            value,
            selected_label,
            colors.foreground,
            self.theme.text.disabled,
          ))
          .child(Icon::new("icons/tree-expand.svg").theme(self.theme)),
      )
      .when(open, |parent| {
        // An item-aligned popup can extend above its trigger. Deferred drawing
        // keeps it above sibling content and outside normal paint ordering.
        parent
          .child(deferred(render_popup_backdrop(viewport_size, backdrop_state)).with_priority(0))
          .child(
            deferred(render_content(
              self.content,
              selected_id.as_deref(),
              highlighted_index,
              self.style.menu_max_height.unwrap_or(DEFAULT_MENU_MAX_HEIGHT),
              popup_layout,
              self.theme,
              self.state,
            ))
            .with_priority(1),
          )
      })
  }
}

/// Trigger popup panel for a [`Select`].
#[derive(Clone, Default)]
pub struct SelectTrigger {
  /// Value subtree displayed before the trigger icon.
  value: SelectValue,
}

impl SelectTrigger {
  /// Creates a trigger with its default value display.
  pub fn new() -> Self {
    Self::default()
  }

  /// Sets the value display nested inside this trigger.
  pub fn value(mut self, value: SelectValue) -> Self {
    self.value = value;
    self
  }
}

/// Displays the selected label or a placeholder inside a [`SelectTrigger`].
#[derive(Clone)]
pub struct SelectValue {
  /// Text displayed when the selection state is empty.
  placeholder: SharedString,
}

impl Default for SelectValue {
  /// Creates the default empty-selection display.
  fn default() -> Self {
    Self {
      placeholder: "Select an option".into(),
    }
  }
}

impl SelectValue {
  /// Creates a value display with the default placeholder.
  pub fn new() -> Self {
    Self::default()
  }

  /// Sets placeholder text shown while no item is selected.
  pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
    self.placeholder = placeholder.into();
    self
  }
}

/// Popup content subtree for a [`Select`].
#[derive(Clone, Default)]
pub struct SelectContent {
  /// Popup placement relative to the trigger.
  position: SelectContentPosition,
  /// Ordered content nodes used for rendering and selection-state flattening.
  children: Vec<SelectContentChild>,
}

impl SelectContent {
  /// Creates empty content with item-aligned positioning.
  pub fn new() -> Self {
    Self::default()
  }

  /// Selects how the content is positioned relative to the trigger.
  pub fn position(mut self, position: SelectContentPosition) -> Self {
    self.position = position;
    self
  }

  /// Adds one grouped section to this content.
  pub fn group(mut self, group: SelectGroup) -> Self {
    self.children.push(SelectContentChild::Group(group));
    self
  }

  /// Adds a visual separator between content groups.
  pub fn separator(mut self, separator: SelectSeparator) -> Self {
    self.children.push(SelectContentChild::Separator(separator));
    self
  }

  /// Flattens grouped content into the display order required by selection state.
  ///
  /// # Parameters
  ///
  /// This method reads the content tree.
  ///
  /// # Returns
  ///
  /// Selectable options in visual order, excluding labels and separators.
  fn options(&self) -> Vec<SelectOption> {
    self
      .children
      .iter()
      .filter_map(|child| match child {
        SelectContentChild::Group(group) => Some(group.items.iter().map(SelectItem::option)),
        SelectContentChild::Separator(_) => None,
      })
      .flatten()
      .collect()
  }

  /// Finds the selected item's vertical center within popup content.
  ///
  /// # Parameters
  ///
  /// * `selected_id` identifies the selected item, when one exists.
  ///
  /// # Returns
  ///
  /// The item's center offset from the content top, or `None` when it is absent.
  fn selected_item_offset(&self, selected_id: Option<&str>) -> Option<Pixels> {
    let selected_id = selected_id?;
    let mut offset = px(0.0);
    for child in &self.children {
      match child {
        SelectContentChild::Group(group) => {
          // Labels and separators occupy real popup rows, so they must be
          // included before aligning a later selected item with the trigger.
          if group.label.is_some() {
            offset += LABEL_HEIGHT;
          }
          for item in &group.items {
            if item.id == selected_id {
              return Some(offset + ITEM_HEIGHT / 2.0);
            }
            offset += ITEM_HEIGHT;
          }
        }
        SelectContentChild::Separator(_) => offset += SEPARATOR_HEIGHT,
      }
    }
    None
  }
}

#[derive(Clone)]
enum SelectContentChild {
  /// A labelled collection of selectable items.
  Group(SelectGroup),
  /// A non-selectable visual boundary between groups.
  Separator(SelectSeparator),
}

/// A labelled set of selectable [`SelectItem`] values.
#[derive(Clone, Default)]
pub struct SelectGroup {
  /// Optional heading rendered before this group's items.
  label: Option<SelectLabel>,
  /// Selectable rows preserved in content display order.
  items: Vec<SelectItem>,
}

impl SelectGroup {
  /// Creates an empty select group.
  pub fn new() -> Self {
    Self::default()
  }

  /// Adds the group's label.
  pub fn label(mut self, label: SelectLabel) -> Self {
    self.label = Some(label);
    self
  }

  /// Adds one selectable item.
  pub fn item(mut self, item: SelectItem) -> Self {
    self.items.push(item);
    self
  }
}

/// Non-selectable text heading for a [`SelectGroup`].
#[derive(Clone)]
pub struct SelectLabel(SharedString);

impl SelectLabel {
  /// Creates a group label.
  pub fn new(label: impl Into<SharedString>) -> Self {
    Self(label.into())
  }
}

/// One selectable row nested in a [`SelectGroup`].
#[derive(Clone)]
pub struct SelectItem {
  /// Stable identifier synchronized with [`SelectInputState`].
  id: SharedString,
  /// Text shown in the popup row and trigger after selection.
  label: SharedString,
  /// Whether this row remains visible but cannot be selected.
  disabled: bool,
}

impl SelectItem {
  /// Creates an enabled selectable item.
  pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
    Self {
      id: id.into(),
      label: label.into(),
      disabled: false,
    }
  }

  /// Sets whether this item can be selected.
  pub fn disabled(mut self, disabled: bool) -> Self {
    self.disabled = disabled;
    self
  }

  /// Converts this rendered item into state-owned option data.
  fn option(&self) -> SelectOption {
    SelectOption::new(self.id.clone(), self.label.clone()).disabled(self.disabled)
  }
}

/// Visual divider nested in [`SelectContent`].
#[derive(Clone, Copy, Debug, Default)]
pub struct SelectSeparator;

impl SelectSeparator {
  /// Creates a visual content separator.
  pub fn new() -> Self {
    Self
  }
}

/// Renders the trigger's selected value or its placeholder.
///
/// # Parameters
///
/// * `value` configures the placeholder displayed for an empty selection.
/// * `selected_label` is the current selected item label, when available.
/// * `foreground` colors selected text.
/// * `placeholder` colors the empty-state text.
///
/// # Returns
///
/// A flexible value element that leaves room for the trigger icon.
fn render_value(
  value: SelectValue,
  selected_label: Option<SharedString>,
  foreground: gpui::Rgba,
  placeholder: gpui::Rgba,
) -> Div {
  let selected = selected_label.is_some();
  div()
    .min_w_0()
    .flex_1()
    .text_color(if selected { foreground } else { placeholder })
    .child(selected_label.unwrap_or(value.placeholder))
}

/// Renders one popup content tree at its selected positioning mode.
///
/// # Parameters
///
/// * `content` supplies groups, labels, items, and separators in visual order.
/// * `selected_id` identifies the item used by item-aligned positioning.
/// * `highlighted_index` identifies the keyboard-highlighted flattened item.
/// * `menu_max_height` limits the visible scrollable popup height.
/// * `layout` provides the trigger dimensions, popup width, and matching option font size.
/// * `theme` supplies the popup color tokens.
/// * `state` receives pointer selection events from rendered items.
///
/// # Returns
///
/// An anchored popup that is wrapped in a deferred overlay by [`Select::render`].
fn render_content(
  content: SelectContent,
  selected_id: Option<&str>,
  highlighted_index: Option<usize>,
  menu_max_height: Pixels,
  layout: SelectPopupLayout,
  theme: UIThemes,
  state: Entity<SelectInputState>,
) -> Anchored {
  let top = match content.position {
    // Align centers rather than row tops so every trigger size has the same
    // visual relationship to its selected item.
    SelectContentPosition::ItemAligned => content
      .selected_item_offset(selected_id)
      .map_or(layout.trigger.height + px(2.0), |offset| {
        layout.trigger.height / 2.0 - offset
      }),
    SelectContentPosition::Popper => layout.trigger.height + px(2.0),
  };
  let mut item_index = 0;
  // GPUI stores the scroll offset in element state keyed by this stable ID.
  let mut popup = div()
    .id(("select-popup", state.entity_id()))
    .w(layout.width)
    .max_h(menu_max_height)
    .overflow_y_scroll()
    .text_size(layout.trigger.font_size)
    .rounded_sm()
    .border_1()
    .border_color(theme.border.primary)
    .bg(theme.background.secondary)
    .occlude();

  for child in content.children {
    match child {
      SelectContentChild::Group(group) => {
        if let Some(label) = group.label {
          popup = popup.child(render_label(label, theme));
        }
        for item in group.items {
          // State navigation sees a flattened option list. Increment only for
          // items so pointer and keyboard indices always refer to the same row.
          popup = popup.child(render_item(
            item,
            item_index,
            highlighted_index == Some(item_index),
            theme,
            state.clone(),
          ));
          item_index += 1;
        }
      }
      SelectContentChild::Separator(_) => popup = popup.child(render_separator(theme)),
    }
  }
  // `anchored` flips the popup above the trigger when the requested lower
  // position exceeds the window viewport, then snaps it into view if needed.
  anchored()
    .anchor(Corner::TopLeft)
    .position_mode(AnchoredPositionMode::Local)
    .offset(point(px(0.0), top))
    .child(popup)
}

/// Renders the transparent modal surface behind an open popup.
///
/// # Parameters
///
/// * `viewport_size` supplies the window area that must block background interaction.
/// * `state` closes the popup after an outside interaction.
///
/// # Returns
///
/// A window-anchored backdrop that blocks background scrolling and clears focus on outside clicks.
fn render_popup_backdrop(viewport_size: Size<Pixels>, state: Entity<SelectInputState>) -> Anchored {
  anchored()
    .anchor(Corner::TopLeft)
    .position(point(px(0.0), px(0.0)))
    .position_mode(AnchoredPositionMode::Window)
    .child(
      div()
        .w(viewport_size.width)
        .h(viewport_size.height)
        .occlude()
        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
        .on_any_mouse_down(move |_, window, cx| {
          state.update(cx, |state, cx| state.dismiss_popup(cx));
          window.blur();
          cx.stop_propagation();
        }),
    )
}

/// Renders a non-selectable group heading.
fn render_label(label: SelectLabel, theme: UIThemes) -> Div {
  div()
    .flex()
    .items_center()
    .h(LABEL_HEIGHT)
    .px(DEFAULT_PADDING_X)
    .text_size(px(12.0))
    .text_color(theme.text.secondary)
    .child(label.0)
}

/// Renders a visual boundary between adjacent content groups.
fn render_separator(theme: UIThemes) -> Div {
  div()
    .h(SEPARATOR_HEIGHT)
    .px(DEFAULT_PADDING_X)
    .child(div().mt(px(4.0)).border_t_1().border_color(theme.border.muted))
}

/// Renders one selectable popup row.
///
/// # Parameters
///
/// * `item` supplies the row label, id, and disabled state.
/// * `index` identifies the item in the flattened selection state.
/// * `highlighted` selects the keyboard-navigation background.
/// * `theme` supplies semantic colors.
/// * `state` receives pointer selection interaction.
///
/// # Returns
///
/// One item row with pointer interaction when it is enabled.
fn render_item(
  item: SelectItem,
  index: usize,
  highlighted: bool,
  theme: UIThemes,
  state: Entity<SelectInputState>,
) -> Div {
  let disabled = item.disabled;
  div()
    .flex()
    .items_center()
    .h(ITEM_HEIGHT)
    .px(DEFAULT_PADDING_X)
    .bg(if highlighted {
      theme.background.selection
    } else {
      theme.background.secondary
    })
    .text_color(if disabled {
      theme.text.disabled
    } else {
      theme.text.primary
    })
    .cursor(if disabled {
      CursorStyle::Arrow
    } else {
      CursorStyle::PointingHand
    })
    .when(!disabled, |style| {
      style.hover(move |style| style.bg(theme.background.hover))
    })
    .when(!disabled, |style| {
      style.on_mouse_down(MouseButton::Left, move |_, _, cx| {
        // Selection closes the popup in state before propagation is stopped.
        if state.update(cx, |state, cx| state.select_index(index, cx)) {
          cx.stop_propagation();
        }
      })
    })
    .child(item.label)
}

/// Resolved dimensions shared by the trigger and item-alignment calculation.
#[derive(Clone, Copy)]
struct SelectInputMetrics {
  /// Trigger height for the current size variant.
  height: Pixels,
  /// Trigger text size for the current size variant.
  font_size: Pixels,
}

/// Layout values shared by the trigger and its anchored popup.
#[derive(Clone, Copy)]
struct SelectPopupLayout {
  /// Trigger height and option font size for the selected size variant.
  trigger: SelectInputMetrics,
  /// Popup width kept equal to the rendered trigger width.
  width: Pixels,
}

impl SelectInputSize {
  /// Returns layout metrics for one select size variant.
  fn metrics(self) -> SelectInputMetrics {
    match self {
      Self::Small => SelectInputMetrics {
        height: px(24.0),
        font_size: px(12.0),
      },
      Self::Medium => SelectInputMetrics {
        height: px(30.0),
        font_size: px(13.0),
      },
      Self::Large => SelectInputMetrics {
        height: px(36.0),
        font_size: px(14.0),
      },
    }
  }
}

/// Theme colors resolved once for one rendered trigger state.
#[derive(Clone, Copy)]
struct SelectInputColors {
  /// Default trigger surface color.
  background: gpui::Rgba,
  /// Trigger surface color while hovered.
  hover_background: gpui::Rgba,
  /// Trigger text color for a selected value.
  foreground: gpui::Rgba,
  /// Default trigger border color.
  border: gpui::Rgba,
  /// Trigger border color while focused.
  focus_border: gpui::Rgba,
}

impl SelectInputColors {
  /// Resolves visual tokens for the select's variant and availability state.
  ///
  /// # Parameters
  ///
  /// * `theme` supplies semantic UI colors.
  /// * `variant` selects the control treatment.
  /// * `disabled` selects the unavailable visual state.
  ///
  /// # Returns
  ///
  /// Colors used by the trigger in its current state.
  fn new(theme: UIThemes, variant: SelectInputVariant, disabled: bool) -> Self {
    if disabled {
      return Self {
        background: theme.background.secondary,
        hover_background: theme.background.secondary,
        foreground: theme.text.disabled,
        border: theme.border.muted,
        focus_border: theme.border.muted,
      };
    }
    match variant {
      SelectInputVariant::Secondary => Self {
        background: theme.background.tertiary,
        hover_background: theme.background.hover,
        foreground: theme.text.primary,
        border: theme.border.primary,
        focus_border: theme.border.focus,
      },
      SelectInputVariant::Transparent => Self {
        background: theme.background.primary,
        hover_background: theme.background.hover,
        foreground: theme.text.primary,
        border: theme.border.muted,
        focus_border: theme.border.focus,
      },
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn select_content_should_place_selected_item_after_group_label() {
    let content = SelectContent::new().group(
      SelectGroup::new()
        .label(SelectLabel::new("Force field"))
        .item(SelectItem::new("amber", "Amber")),
    );

    assert_eq!(
      content.selected_item_offset(Some("amber")),
      Some(LABEL_HEIGHT + ITEM_HEIGHT / 2.0)
    );
  }
}
