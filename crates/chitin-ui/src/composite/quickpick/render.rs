//! Rendering for the reusable quick-pick composite.

use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
  AnyElement, Div, ElementId, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, SharedString,
  Styled, UniformListScrollHandle, div, px, uniform_list,
};

use super::{QuickPickItem, QuickPickOverlay, QuickPickSearchInput, QuickPickSelectHandler};
use crate::{
  primitive::input::text::{TextInput, TextInputSize, TextInputVariant},
  themes::{UIThemes, builtins},
};

/// Maximum number of quick-pick rows visible before the body scrolls.
pub const DEFAULT_QUICK_PICK_VISIBLE_ROWS: usize = 8;
/// Default height of quick pick item
pub const DEFAULT_QUICK_PICK_ITEM_HEIGHT: Pixels = px(34.0);
/// Default height of the quick-pick query line.
pub const DEFAULT_QUICK_PICK_QUERY_HEIGHT: Pixels = px(34.0);
/// Default margin from top
pub const DEFAULT_QUICK_PICK_MARGIN_TOP: Pixels = px(30.0);
/// Default width of quick pick panel
pub const DEFAULT_QUICK_PICK_WIDTH: Pixels = px(520.0);
/// Default max height of quick pick panel
pub const DEFAULT_QUICK_PICK_MAX_HEIGHT: Pixels = px(560.0);
/// Default max height of the quick-pick result list.
pub const DEFAULT_QUICK_PICK_BODY_MAX_HEIGHT: Pixels = px(512.0);

/// Renders a floating quick-pick overlay.
///
/// # Parameters
///
/// * `overlay` is the query line and body data to render.
/// * `theme` supplies workbench colors.
/// * `on_select` is invoked with the row index and current GPUI window selected by pointer.
///
/// # Returns
///
/// A full-screen overlay element.
pub fn render_quick_pick_overlay(
  overlay: QuickPickOverlay,
  theme: UIThemes,
  on_select: Rc<QuickPickSelectHandler>,
) -> Div {
  let body_height = quick_pick_body_height(overlay.items.len());
  let panel_height = DEFAULT_QUICK_PICK_QUERY_HEIGHT + body_height;
  let panel_body = render_items(
    "quick-pick-result-list",
    overlay.items,
    overlay.selected_index,
    overlay.scroll_handle,
    overlay.empty_message,
    theme,
    on_select,
  );

  div()
    .absolute()
    .inset_0()
    .flex()
    .justify_center()
    .bg(builtins::TRANSPARENT)
    .occlude()
    .on_scroll_wheel(|_, _, cx| {
      cx.stop_propagation();
    })
    .child(
      div()
        .mt(DEFAULT_QUICK_PICK_MARGIN_TOP)
        .w(DEFAULT_QUICK_PICK_WIDTH)
        .max_w(DEFAULT_QUICK_PICK_WIDTH)
        .h(panel_height)
        .max_h(DEFAULT_QUICK_PICK_MAX_HEIGHT)
        .flex_none()
        .rounded_md()
        .border_1()
        .border_color(theme.border.primary)
        .bg(theme.background.secondary)
        .overflow_hidden()
        .on_scroll_wheel(|_, _, cx| {
          cx.stop_propagation();
        })
        .child(render_query_line(
          overlay.query,
          overlay.placeholder,
          overlay.search_input,
          theme,
        ))
        .child(panel_body),
    )
}

/// Renders the quick-pick query line.
///
/// # Parameters
///
/// * `query` is the current text.
/// * `placeholder` is shown when `query` is empty.
/// * `theme` supplies colors.
///
/// # Returns
///
/// A visual input row.
fn render_query_line(
  query: SharedString,
  placeholder: SharedString,
  search_input: Option<QuickPickSearchInput>,
  theme: UIThemes,
) -> Div {
  let query_is_empty = query.is_empty();
  let value = if query_is_empty { placeholder.clone() } else { query };

  div()
    .flex()
    .items_center()
    .gap_2()
    .h(DEFAULT_QUICK_PICK_QUERY_HEIGHT)
    .border_b_1()
    .border_color(theme.border.muted)
    .text_color(theme.text.primary)
    // Though it's called tree-collapse, it's used here as a prompt indicator
    .child(match search_input {
      Some(search_input) => TextInput::new(search_input.state)
        .theme(theme)
        .size(TextInputSize::Medium)
        .full_width(true)
        .variant(TextInputVariant::Transparent)
        .placeholder(placeholder)
        .into_any_element(),
      None => div()
        .flex_1()
        .text_size(px(15.0))
        .text_color(if query_is_empty {
          theme.text.disabled
        } else {
          theme.text.primary
        })
        .child(value)
        .into_any_element(),
    })
}

/// Renders quick-pick result rows.
///
/// # Parameters
///
/// * `items` are ranked rows.
/// * `selected_index` controls highlight state.
/// * `scroll_handle` optionally preserves and controls virtual-list scroll state.
/// * `empty_message` is shown when no rows exist.
/// * `theme` supplies colors.
/// * `on_select` handles pointer selection.
///
/// # Returns
///
/// A quick-pick body element.
fn render_items(
  id: impl Into<ElementId>,
  items: Vec<QuickPickItem>,
  selected_index: usize,
  scroll_handle: Option<UniformListScrollHandle>,
  empty_message: SharedString,
  theme: UIThemes,
  on_select: Rc<QuickPickSelectHandler>,
) -> AnyElement {
  if items.is_empty() {
    return div()
      .h(quick_pick_body_height(0))
      .flex()
      .items_center()
      .px_3()
      .text_color(theme.text.secondary)
      .text_xs()
      .child(empty_message)
      .into_any_element();
  }

  let body_height = quick_pick_body_height(items.len());
  let items = Rc::new(items);
  let item_count = items.len();
  let list = uniform_list(id, item_count, move |range, _window, _cx| {
    range
      .filter_map(|index| items.get(index).cloned().map(|item| (index, item)))
      .map(|(index, item)| {
        render_item(index, item, index == selected_index, theme, on_select.clone()).into_any_element()
      })
      .collect()
  });

  let list = match scroll_handle {
    Some(scroll_handle) => list.track_scroll(&scroll_handle),
    None => list,
  };

  div()
    .flex()
    .flex_col()
    .h(body_height)
    .max_h(DEFAULT_QUICK_PICK_BODY_MAX_HEIGHT)
    .flex_none()
    .min_h_0()
    .child(Styled::scrollbar_width(list.size_full(), px(8.0)))
    .into_any_element()
}

/// Calculates the result-list height without allowing a short list to reserve
/// the full scroll viewport.
fn quick_pick_body_height(item_count: usize) -> Pixels {
  let visible_rows = item_count.clamp(1, DEFAULT_QUICK_PICK_VISIBLE_ROWS);
  DEFAULT_QUICK_PICK_ITEM_HEIGHT * visible_rows
}

/// Renders one quick-pick row.
///
/// # Parameters
///
/// * `index` is the row index passed to selection callbacks.
/// * `item` is the row content.
/// * `selected` controls highlight state.
/// * `theme` supplies colors.
/// * `on_select` handles pointer selection.
///
/// # Returns
///
/// A quick-pick row element.
fn render_item(
  index: usize,
  item: QuickPickItem,
  selected: bool,
  theme: UIThemes,
  on_select: Rc<QuickPickSelectHandler>,
) -> Div {
  let shortcut = item.shortcut.unwrap_or_else(|| SharedString::from(""));

  div()
    .flex()
    .items_center()
    .h(DEFAULT_QUICK_PICK_ITEM_HEIGHT)
    .w_full()
    .px_1()
    .on_mouse_up(MouseButton::Left, move |_, window, cx| {
      on_select(index, window, cx);
    })
    .child(
      div()
        .flex()
        .items_center()
        .justify_between()
        .h_full()
        .w_full()
        .px_2()
        .rounded_sm()
        .bg(if selected {
          theme.background.hover
        } else {
          builtins::TRANSPARENT
        })
        .when(!selected, |style| style.hover(|style| style.bg(theme.background.hover)))
        .child(div().text_xs().text_color(theme.text.primary).child(item.title))
        .child(div().text_xs().text_color(theme.text.secondary).child(shortcut)),
    )
}
