// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Chitin Contributors

//! Rendering and composition for the select input primitive.

use gpui::{
  App, Bounds, CursorStyle, Div, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
  RenderOnce, ScrollHandle, SharedString, Size, Window, div, point, prelude::*, px,
};

use super::{SelectInputState, SelectOption};
use crate::{
  primitive::icon::Icon,
  primitive::popover::{Popover, PopoverPlacement, PopoverStyle},
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
/// Border inset between the popup surface and its scrollable option content.
const POPUP_BORDER_WIDTH: Pixels = px(1.0);

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

    let (selected_id, selected_label, highlighted_index, open, disabled, popup_scroll_handle, trigger_bounds) = {
      let state = self.state.read(cx);
      (
        state.selected_id().map(SharedString::from),
        state.selected_option().map(|option| SharedString::from(option.label())),
        state.highlighted_index(),
        state.is_open(),
        state.is_disabled(),
        state.popup_scroll_handle().clone(),
        state.trigger_bounds(),
      )
    };
    let viewport_size = window.viewport_size();
    let align_selected_on_open = open
      && self
        .state
        .update(cx, |state, _| state.take_selected_alignment_request(viewport_size));
    let highlight_scroll_request = open
      .then(|| self.state.update(cx, |state, _| state.take_highlight_scroll_request()))
      .flatten();
    let colors = SelectInputColors::new(self.theme, self.variant, disabled);
    let pointer_state = self.state.clone();
    let key_state = self.state.clone();
    let trigger_bounds_state = self.state.clone();
    let width = self.style.width.unwrap_or(DEFAULT_SELECT_WIDTH);
    let popup_layout = SelectPopupLayout {
      trigger: metrics,
      configured_width: width,
      trigger_bounds,
      viewport_size,
    };
    let value = self.trigger.value;

    div()
      .relative()
      .w(width)
      .when(self.full_width, |style| style.w_full())
      .child(
        div()
          .on_children_prepainted(move |children_bounds, _, cx| {
            if let Some(bounds) = children_bounds.first().copied() {
              trigger_bounds_state.update(cx, |state, cx| state.set_trigger_bounds(bounds, cx));
            }
          })
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
          ),
      )
      .when(open, |parent| {
        let dismiss_state = self.state.clone();
        let popover = render_content(
          self.content,
          selected_id.as_deref(),
          highlighted_index,
          self.style.menu_max_height.unwrap_or(DEFAULT_MENU_MAX_HEIGHT),
          popup_layout,
          self.theme,
          SelectPopupInteraction {
            state: self.state,
            scroll_handle: popup_scroll_handle,
            align_selected_on_open,
            highlight_scroll_request,
          },
        )
        .on_dismiss(move |window, cx| {
          dismiss_state.update(cx, |state, cx| state.dismiss_popup(cx));
          window.blur();
        });

        parent
          .child(popover.deferred_backdrop(viewport_size))
          .child(popover.deferred_content())
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

  /// Returns the total vertical extent of the popup's direct children.
  fn content_height(&self) -> Pixels {
    self.children.iter().fold(px(0.0), |height, child| match child {
      SelectContentChild::Group(group) => {
        height + if group.label.is_some() { LABEL_HEIGHT } else { px(0.0) } + ITEM_HEIGHT * group.items.len() as f32
      }
      SelectContentChild::Separator(_) => height + SEPARATOR_HEIGHT,
    })
  }

  /// Resolves the selected item's visible center within a height-constrained popup.
  ///
  /// # Parameters
  ///
  /// * `selected_id` identifies the selected item, when one exists.
  /// * `popup_height` is the viewport height resolved for this popup placement.
  ///
  /// # Returns
  ///
  /// Geometry required to align the selected item, or `None` when it is absent.
  fn item_alignment(&self, selected_id: Option<&str>, popup_height: Pixels) -> Option<ItemAlignment> {
    let selected_id = selected_id?;
    let mut content_height = px(0.0);
    let mut selected_item = None;
    for child in &self.children {
      match child {
        SelectContentChild::Group(group) => {
          if group.label.is_some() {
            content_height += LABEL_HEIGHT;
          }
          for item in &group.items {
            if item.id == selected_id {
              selected_item = Some(content_height);
            }
            content_height += ITEM_HEIGHT;
          }
        }
        SelectContentChild::Separator(_) => {
          content_height += SEPARATOR_HEIGHT;
        }
      }
    }
    let selected_top = selected_item?;
    let viewport_height = content_height.min(popup_height);
    let maximum_scroll_offset = (content_height - viewport_height).max(px(0.0));
    let desired_scroll_offset = (selected_top + ITEM_HEIGHT / 2.0 - viewport_height / 2.0).max(px(0.0));
    let scroll_offset = desired_scroll_offset.min(maximum_scroll_offset);

    Some(ItemAlignment {
      scroll_offset,
      content_center: selected_top + ITEM_HEIGHT / 2.0,
      visible_center: selected_top + ITEM_HEIGHT / 2.0 - scroll_offset,
    })
  }

  /// Returns the top edge of a flattened option in popup content coordinates.
  fn option_top(&self, option_index: usize) -> Option<Pixels> {
    let mut selectable_index = 0;
    let mut top = px(0.0);
    for child in &self.children {
      match child {
        SelectContentChild::Group(group) => {
          if group.label.is_some() {
            top += LABEL_HEIGHT;
          }
          for _ in &group.items {
            if selectable_index == option_index {
              return Some(top);
            }
            selectable_index += 1;
            top += ITEM_HEIGHT;
          }
        }
        SelectContentChild::Separator(_) => top += SEPARATOR_HEIGHT,
      }
    }
    None
  }

  /// Returns the minimum scroll offset that makes one option fully visible.
  fn scroll_offset_to_reveal_option(
    &self,
    option_index: usize,
    popup_height: Pixels,
    current_offset: Pixels,
  ) -> Option<Pixels> {
    let option_top = self.option_top(option_index)?;
    let option_bottom = option_top + ITEM_HEIGHT;
    let content_height = self.content_height();
    let maximum_offset = (content_height - popup_height).max(px(0.0));
    let offset = if option_top + current_offset < px(0.0) {
      -option_top
    } else if option_bottom + current_offset > popup_height {
      popup_height - option_bottom
    } else {
      current_offset
    };
    Some(offset.clamp(-maximum_offset, px(0.0)))
  }
}

#[derive(Clone)]
enum SelectContentChild {
  /// A labelled collection of selectable items.
  Group(SelectGroup),
  /// A non-selectable visual boundary between groups.
  Separator(SelectSeparator),
}

/// Geometry used to align one selected option with a trigger.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ItemAlignment {
  /// Downward content offset needed to reveal the selected option.
  scroll_offset: Pixels,
  /// Selected option center in the unscrolled popup content.
  content_center: Pixels,
  /// Selected option center after its scroll offset is clamped.
  visible_center: Pixels,
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
/// * `interaction` supplies popup state, scrolling, and one-time alignment behavior.
///
/// # Returns
///
/// A modal popover wrapped in deferred overlays by [`Select::render`].
fn render_content(
  content: SelectContent,
  selected_id: Option<&str>,
  highlighted_index: Option<usize>,
  menu_max_height: Pixels,
  layout: SelectPopupLayout,
  theme: UIThemes,
  interaction: SelectPopupInteraction,
) -> Popover {
  let configured_height = content.content_height().min(menu_max_height);
  let candidate_item_alignment = content.item_alignment(selected_id, configured_height);
  let placement = layout.resolve_placement(
    content.position,
    content.content_height(),
    menu_max_height,
    candidate_item_alignment.map(|alignment| alignment.visible_center),
  );
  let popup_height = placement.popup_height();
  let item_alignment = content.item_alignment(selected_id, popup_height);
  if interaction.align_selected_on_open
    && let Some(item_alignment) = item_alignment
  {
    interaction
      .scroll_handle
      .set_offset(point(px(0.0), -item_alignment.scroll_offset));
  }
  if let Some(item_index) = interaction.highlight_scroll_request
    && let Some(offset) =
      content.scroll_offset_to_reveal_option(item_index, popup_height, interaction.scroll_handle.offset().y)
  {
    interaction.scroll_handle.set_offset(point(px(0.0), offset));
  }
  let mut item_index = 0;
  // Popover owns the stable element ID and persistent scroll position.
  let mut popup = div().w(layout.popup_width()).text_size(layout.trigger.font_size);

  for child in content.children {
    match child {
      SelectContentChild::Group(group) => {
        if let Some(label) = group.label {
          popup = popup.child(render_label(label, theme));
        }
        for item in group.items {
          // State navigation sees a flattened option list. Increment only for
          // items so pointer and keyboard indices always refer to the same row.
          let selected = selected_id == Some(item.id.as_ref());
          popup = popup.child(render_item(
            item,
            item_index,
            highlighted_index == Some(item_index),
            selected,
            theme,
            interaction.state.clone(),
          ));
          item_index += 1;
        }
      }
      SelectContentChild::Separator(_) => popup = popup.child(render_separator(theme)),
    }
  }
  let popover = Popover::new(("select-popup", interaction.state.entity_id()), popup)
    .anchor_size(Size {
      width: layout.popup_width(),
      height: layout.popup_anchor_height(),
    })
    .style(
      PopoverStyle::new()
        .width(layout.popup_width())
        .max_height(popup_height)
        .background(theme.background.secondary)
        .border_color(theme.border.primary)
        .border_width(POPUP_BORDER_WIDTH),
    )
    .theme(theme)
    .scroll_handle(interaction.scroll_handle);
  let popover = if let Some(trigger_bounds) = layout.trigger_bounds {
    popover.anchor_position(trigger_bounds.origin)
  } else {
    popover
  };

  match placement {
    SelectPopupPlacement::ItemAligned { .. } => {
      // ItemAligned establishes selected-item/trigger alignment when opening.
      // Later wheel input moves only the list's scroll viewport, never this surface.
      let top = item_alignment.map_or(layout.trigger.height + px(2.0), |item_alignment| {
        layout.item_aligned_top(item_alignment.visible_center)
      });
      popover.offset(point(px(0.0), top))
    }
    SelectPopupPlacement::Popover { placement, .. } => popover.placement(placement),
  }
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
/// * `selected` controls the selected-item checkmark.
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
  selected: bool,
  theme: UIThemes,
  state: Entity<SelectInputState>,
) -> Div {
  let disabled = item.disabled;
  let selection_state = state.clone();
  let item = div()
    .flex()
    .items_center()
    .justify_between()
    .h(ITEM_HEIGHT)
    .px(DEFAULT_PADDING_X)
    .bg(if highlighted {
      theme.background.hover
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
        if selection_state.update(cx, |state, cx| state.select_index(index, cx)) {
          cx.stop_propagation();
        }
      })
    })
    .child(item.label)
    .when(selected, |style| {
      style.child(Icon::new("icons/check.svg").size(px(14.0)).color(if disabled {
        theme.text.disabled
      } else {
        theme.text.primary
      }))
    });

  if !selected {
    return item;
  }

  div()
    .on_children_prepainted(move |children_bounds, _, cx| {
      let Some(selected_bounds) = children_bounds.first().copied() else {
        return;
      };
      let trigger_bounds = state.read(cx).trigger_bounds();
      let Some(trigger_bounds) = trigger_bounds else {
        return;
      };
      let selected_center = (selected_bounds.top() + selected_bounds.bottom()) / 2.0;
      let trigger_center = (trigger_bounds.top() + trigger_bounds.bottom()) / 2.0;
      log::debug!(
        "Select ItemAligned measured: selected_bounds={:?}, trigger_bounds={:?}, selected_center={:?}, trigger_center={:?}, center_error={:?}",
        selected_bounds,
        trigger_bounds,
        selected_center,
        trigger_center,
        selected_center - trigger_center,
      );
    })
    .child(item)
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
  /// Configured width used until the trigger has been measured.
  configured_width: Pixels,
  /// Trigger bounds measured during the previous prepaint pass.
  trigger_bounds: Option<Bounds<Pixels>>,
  /// Current window size used to constrain item-aligned content.
  viewport_size: Size<Pixels>,
}

/// Placement selected after applying select-specific item-alignment fallback rules.
#[derive(Clone, Copy, Debug, PartialEq)]
enum SelectPopupPlacement {
  /// The selected option can share the trigger's vertical center without clipping the popup.
  ItemAligned { height: Pixels },
  /// The popup uses a conventional anchor edge after item alignment cannot fit.
  Popover {
    /// Built-in edge placement chosen from available window space.
    placement: PopoverPlacement,
    /// Visible popup height constrained to that edge's available space.
    height: Pixels,
  },
}

impl SelectPopupPlacement {
  /// Returns the resolved visible popup height.
  fn popup_height(self) -> Pixels {
    match self {
      Self::ItemAligned { height } | Self::Popover { height, .. } => height,
    }
  }
}

/// Persistent interaction values consumed by one popup render pass.
struct SelectPopupInteraction {
  /// Entity that receives option selection events and identifies popup element state.
  state: Entity<SelectInputState>,
  /// Shared viewport position for the scrollable option list.
  scroll_handle: ScrollHandle,
  /// Whether this render must reveal and align the selected option.
  align_selected_on_open: bool,
  /// Flattened option index that keyboard navigation must reveal.
  highlight_scroll_request: Option<usize>,
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

impl SelectPopupLayout {
  /// Resolves item alignment when it fits, otherwise chooses the roomier popup edge.
  fn resolve_placement(
    self,
    requested_position: SelectContentPosition,
    content_height: Pixels,
    menu_max_height: Pixels,
    selected_visible_center: Option<Pixels>,
  ) -> SelectPopupPlacement {
    let configured_height = content_height.min(menu_max_height);
    if requested_position == SelectContentPosition::Popper {
      return self.edge_placement(configured_height);
    }

    let Some(trigger_bounds) = self.trigger_bounds else {
      return self.edge_placement(configured_height);
    };
    let Some(selected_visible_center) = selected_visible_center else {
      return self.edge_placement(configured_height);
    };
    let popup_top =
      (trigger_bounds.top() + trigger_bounds.bottom()) / 2.0 - selected_visible_center - POPUP_BORDER_WIDTH;
    let popup_bottom = popup_top + configured_height;

    if popup_top >= px(0.0) && popup_bottom <= self.viewport_size.height {
      SelectPopupPlacement::ItemAligned {
        height: configured_height,
      }
    } else {
      self.edge_placement(configured_height)
    }
  }

  /// Chooses an edge and constrains the popup to the available space on that edge.
  fn edge_placement(self, configured_height: Pixels) -> SelectPopupPlacement {
    let Some(trigger_bounds) = self.trigger_bounds else {
      return SelectPopupPlacement::Popover {
        placement: PopoverPlacement::Below,
        height: configured_height,
      };
    };
    let above = trigger_bounds.top().max(px(0.0));
    let below = (self.viewport_size.height - trigger_bounds.bottom()).max(px(0.0));
    let placement = if above > below {
      PopoverPlacement::Above
    } else {
      PopoverPlacement::Below
    };
    let height = configured_height.min(if placement == PopoverPlacement::Above {
      above
    } else {
      below
    });

    SelectPopupPlacement::Popover { placement, height }
  }

  /// Converts an option-content center into a trigger-local popup surface offset.
  fn item_aligned_top(self, visible_center: Pixels) -> Pixels {
    let top = self
      .trigger_bounds
      .map_or(self.trigger.height / 2.0 - visible_center, |trigger_bounds| {
        (trigger_bounds.top() + trigger_bounds.bottom()) / 2.0 - visible_center - trigger_bounds.top()
      })
      - POPUP_BORDER_WIDTH;
    log::debug!(
      "Select ItemAligned geometry: trigger_bounds={:?}, trigger_height={:?}, visible_center={:?}, border_inset={:?}, popup_top={:?}",
      self.trigger_bounds,
      self.trigger.height,
      visible_center,
      POPUP_BORDER_WIDTH,
      top,
    );
    top
  }

  /// Returns the measured trigger width, falling back before the first prepaint pass.
  fn popup_width(self) -> Pixels {
    self
      .trigger_bounds
      .map_or(self.configured_width, |trigger_bounds| trigger_bounds.size.width)
  }

  /// Returns the measured trigger height used by conventional anchor placement.
  fn popup_anchor_height(self) -> Pixels {
    self
      .trigger_bounds
      .map_or(self.trigger.height, |trigger_bounds| trigger_bounds.size.height)
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
  fn item_alignment_should_account_for_labels_separators_and_scroll_limits() {
    let content = SelectContent::new()
      .group(
        SelectGroup::new()
          .label(SelectLabel::new("Molecular mechanics"))
          .item(SelectItem::new("amber", "Amber")),
      )
      .separator(SelectSeparator::new())
      .group(
        SelectGroup::new()
          .label(SelectLabel::new("No force field"))
          .item(SelectItem::new("none", "None")),
      );

    assert_eq!(
      content.item_alignment(Some("none"), px(60.0)),
      Some(ItemAlignment {
        scroll_offset: px(57.0),
        content_center: px(102.0),
        visible_center: px(45.0),
      })
    );
  }

  #[test]
  fn item_alignment_should_center_selected_item_when_content_allows_it() {
    let content = SelectContent::new().group(
      SelectGroup::new()
        .item(SelectItem::new("one", "One"))
        .item(SelectItem::new("two", "Two"))
        .item(SelectItem::new("three", "Three"))
        .item(SelectItem::new("four", "Four")),
    );

    assert_eq!(
      content.item_alignment(Some("three"), px(60.0)),
      Some(ItemAlignment {
        scroll_offset: px(45.0),
        content_center: px(75.0),
        visible_center: px(30.0),
      })
    );
  }

  #[test]
  fn keyboard_scroll_should_include_labels_and_separators() {
    let content = SelectContent::new()
      .group(
        SelectGroup::new()
          .label(SelectLabel::new("Force fields"))
          .item(SelectItem::new("amber", "Amber")),
      )
      .separator(SelectSeparator::new())
      .group(
        SelectGroup::new()
          .label(SelectLabel::new("No force field"))
          .item(SelectItem::new("none", "None")),
      );

    assert_eq!(
      content.scroll_offset_to_reveal_option(1, px(60.0), px(0.0)),
      Some(px(-57.0))
    );
  }

  #[test]
  fn item_aligned_position_should_fall_back_below_near_the_window_top() {
    let layout = SelectPopupLayout {
      trigger: SelectInputMetrics {
        height: px(30.0),
        font_size: px(13.0),
      },
      configured_width: px(240.0),
      trigger_bounds: Some(Bounds {
        origin: point(px(0.0), px(20.0)),
        size: Size {
          width: px(240.0),
          height: px(30.0),
        },
      }),
      viewport_size: Size {
        width: px(800.0),
        height: px(700.0),
      },
    };

    assert_eq!(
      layout.resolve_placement(
        SelectContentPosition::ItemAligned,
        px(240.0),
        px(240.0),
        Some(px(120.0)),
      ),
      SelectPopupPlacement::Popover {
        placement: PopoverPlacement::Below,
        height: px(240.0),
      }
    );
  }

  #[test]
  fn item_aligned_position_should_fall_back_above_near_the_window_bottom() {
    let layout = SelectPopupLayout {
      trigger: SelectInputMetrics {
        height: px(30.0),
        font_size: px(13.0),
      },
      configured_width: px(240.0),
      trigger_bounds: Some(Bounds {
        origin: point(px(0.0), px(650.0)),
        size: Size {
          width: px(240.0),
          height: px(30.0),
        },
      }),
      viewport_size: Size {
        width: px(800.0),
        height: px(700.0),
      },
    };

    assert_eq!(
      layout.resolve_placement(
        SelectContentPosition::ItemAligned,
        px(240.0),
        px(240.0),
        Some(px(120.0)),
      ),
      SelectPopupPlacement::Popover {
        placement: PopoverPlacement::Above,
        height: px(240.0),
      }
    );
  }

  #[test]
  fn item_aligned_position_should_remain_centered_when_it_fits() {
    let layout = SelectPopupLayout {
      trigger: SelectInputMetrics {
        height: px(30.0),
        font_size: px(13.0),
      },
      configured_width: px(240.0),
      trigger_bounds: Some(Bounds {
        origin: point(px(0.0), px(300.0)),
        size: Size {
          width: px(240.0),
          height: px(30.0),
        },
      }),
      viewport_size: Size {
        width: px(800.0),
        height: px(700.0),
      },
    };

    assert_eq!(
      layout.resolve_placement(
        SelectContentPosition::ItemAligned,
        px(240.0),
        px(240.0),
        Some(px(120.0)),
      ),
      SelectPopupPlacement::ItemAligned { height: px(240.0) }
    );
  }

  #[test]
  fn item_aligned_position_should_fall_back_before_window_snapping_changes_the_alignment() {
    let layout = SelectPopupLayout {
      trigger: SelectInputMetrics {
        height: px(30.0),
        font_size: px(13.0),
      },
      configured_width: px(240.0),
      trigger_bounds: Some(Bounds {
        origin: point(px(0.0), px(580.0)),
        size: Size {
          width: px(560.0),
          height: px(30.0),
        },
      }),
      viewport_size: Size {
        width: px(800.0),
        height: px(768.0),
      },
    };

    assert_eq!(
      layout.resolve_placement(SelectContentPosition::ItemAligned, px(240.0), px(240.0), Some(px(39.0)),),
      SelectPopupPlacement::Popover {
        placement: PopoverPlacement::Above,
        height: px(240.0),
      }
    );
  }

  #[test]
  fn popup_width_should_prefer_the_measured_trigger_width() {
    let layout = SelectPopupLayout {
      trigger: SelectInputMetrics {
        height: px(30.0),
        font_size: px(13.0),
      },
      configured_width: px(240.0),
      trigger_bounds: Some(Bounds {
        origin: point(px(0.0), px(0.0)),
        size: Size {
          width: px(560.0),
          height: px(30.0),
        },
      }),
      viewport_size: Size {
        width: px(800.0),
        height: px(768.0),
      },
    };

    assert_eq!(layout.popup_width(), px(560.0));
  }

  #[test]
  fn item_aligned_popup_top_should_use_the_initial_selected_item_center() {
    let layout = SelectPopupLayout {
      trigger: SelectInputMetrics {
        height: px(30.0),
        font_size: px(13.0),
      },
      configured_width: px(240.0),
      trigger_bounds: Some(Bounds {
        origin: point(px(0.0), px(300.0)),
        size: Size {
          width: px(240.0),
          height: px(30.0),
        },
      }),
      viewport_size: Size {
        width: px(800.0),
        height: px(700.0),
      },
    };
    let alignment = ItemAlignment {
      scroll_offset: px(45.0),
      content_center: px(75.0),
      visible_center: px(30.0),
    };

    assert_eq!(layout.item_aligned_top(alignment.visible_center), px(-16.0));
  }
}
