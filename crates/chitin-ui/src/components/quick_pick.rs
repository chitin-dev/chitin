//! Reusable quick-pick overlay components.

use std::rc::Rc;

use gpui::{
  AnyElement, App, Div, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, SharedString, Styled,
  Window, div, prelude::*, px,
};

use crate::themes::{UIThemes, builtins};

/// Maximum number of quick-pick rows rendered in one overlay.
pub const DEFAULT_QUICK_PICK_VISIBLE_ROWS: usize = 10;
/// Default height of quick pick item
pub const DEFAULT_QUICK_PICK_ITEM_HEIGHT: Pixels = px(60.0);
/// Default margin from top
pub const DEFAULT_QUICK_PICK_MARGIN_TOP: Pixels = px(30.0);
/// Default width of quick pick panel
pub const DEFAULT_QUICK_PICK_WIDTH: Pixels = px(660.0);
/// Default max height of quick pick panel
pub const DEFAULT_QUICK_PICK_MAX_HEIGHT: Pixels = px(560.0);

/// Callback invoked when a quick-pick row is selected.
type QuickPickSelectHandler = dyn for<'a, 'b> Fn(usize, &'a mut Window, &'b mut App);

/// One visible quick-pick result row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickPickItem {
  /// Main row label.
  pub title: SharedString,
  /// Secondary row metadata.
  pub subtitle: SharedString,
  /// Optional shortcut text shown on the trailing edge.
  pub shortcut: Option<SharedString>,
}

/// Content for a compact form shown inside the quick-pick shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickPickForm {
  /// Form title.
  pub title: SharedString,
  /// Current form value or placeholder.
  pub value: SharedString,
}

/// Body content rendered below the query line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuickPickContent {
  /// Ranked selectable rows.
  Items {
    /// Rows to render.
    items: Vec<QuickPickItem>,
    /// Currently selected row index.
    selected_index: usize,
    /// Empty-state text.
    empty_message: SharedString,
  },
  /// A command-specific form.
  Form(QuickPickForm),
}

/// Data needed to render one quick-pick overlay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickPickOverlay {
  /// Leading prompt label.
  pub prompt: SharedString,
  /// Current query text.
  pub query: SharedString,
  /// Placeholder shown when `query` is empty.
  pub placeholder: SharedString,
  /// Overlay body content.
  pub content: QuickPickContent,
}

impl QuickPickItem {
  /// Creates a quick-pick row.
  ///
  /// # Parameters
  ///
  /// `title` is the primary row label.
  ///
  /// `subtitle` is the secondary row metadata.
  ///
  /// `shortcut` is optional trailing shortcut text.
  ///
  /// # Returns
  ///
  /// A reusable quick-pick row model.
  pub fn new(
    title: impl Into<SharedString>,
    subtitle: impl Into<SharedString>,
    shortcut: Option<impl Into<SharedString>>,
  ) -> Self {
    Self {
      title: title.into(),
      subtitle: subtitle.into(),
      shortcut: shortcut.map(Into::into),
    }
  }
}

/// Renders a floating quick-pick overlay.
///
/// # Parameters
///
/// `overlay` is the query line and body data to render.
///
/// `theme` supplies workbench colors.
///
/// `on_select` is invoked with the row index and current GPUI window selected by pointer.
///
/// # Returns
///
/// A full-screen overlay element.
pub fn render_quick_pick_overlay(
  overlay: QuickPickOverlay,
  theme: UIThemes,
  on_select: Rc<QuickPickSelectHandler>,
) -> Div {
  let is_form = matches!(overlay.content, QuickPickContent::Form(_));
  let panel_body = match overlay.content {
    QuickPickContent::Items {
      items,
      selected_index,
      empty_message,
    } => render_items(items, selected_index, empty_message, theme, on_select),
    QuickPickContent::Form(form) => render_form(form, theme),
  };

  div()
    .absolute()
    .inset_0()
    .flex()
    .justify_center()
    .bg(builtins::TRANSPARENT)
    .child(
      div()
        .mt(DEFAULT_QUICK_PICK_MARGIN_TOP)
        .w(DEFAULT_QUICK_PICK_WIDTH)
        .max_w(DEFAULT_QUICK_PICK_WIDTH)
        .max_h(DEFAULT_QUICK_PICK_MAX_HEIGHT)
        .rounded_md()
        .border_1()
        .border_color(theme.border.primary)
        .bg(theme.background.secondary)
        .overflow_hidden()
        .when(!is_form, |parent| {
          parent.child(render_query_line(
            overlay.prompt,
            overlay.query,
            overlay.placeholder,
            theme,
          ))
        })
        .child(panel_body),
    )
}

/// Renders the quick-pick query line.
///
/// # Parameters
///
/// `prompt` is shown before the query text.
///
/// `query` is the current text.
///
/// `placeholder` is shown when `query` is empty.
///
/// `theme` supplies colors.
///
/// # Returns
///
/// A visual input row.
fn render_query_line(prompt: SharedString, query: SharedString, placeholder: SharedString, theme: UIThemes) -> Div {
  let query_is_empty = query.is_empty();
  let value = if query_is_empty { placeholder } else { query };

  div()
    .flex()
    .items_center()
    .gap_2()
    .h(px(48.0))
    .px_3()
    .border_b_1()
    .border_color(theme.border.muted)
    .text_color(theme.text.primary)
    .child(div().text_color(theme.text.secondary).child(prompt))
    .child(
      div()
        .flex_1()
        .text_size(px(15.0))
        .text_color(if query_is_empty {
          theme.text.disabled
        } else {
          theme.text.primary
        })
        .child(value),
    )
}

/// Renders quick-pick result rows.
///
/// # Parameters
///
/// `items` are ranked rows.
///
/// `selected_index` controls highlight state.
///
/// `empty_message` is shown when no rows exist.
///
/// `theme` supplies colors.
///
/// `on_select` handles pointer selection.
///
/// # Returns
///
/// A quick-pick body element.
fn render_items(
  items: Vec<QuickPickItem>,
  selected_index: usize,
  empty_message: SharedString,
  theme: UIThemes,
  on_select: Rc<QuickPickSelectHandler>,
) -> AnyElement {
  if items.is_empty() {
    return div()
      .h(DEFAULT_QUICK_PICK_ITEM_HEIGHT)
      .flex()
      .items_center()
      .px_3()
      .text_color(theme.text.secondary)
      .child(empty_message)
      .into_any_element();
  }

  div()
    .flex()
    .flex_col()
    .children(
      items
        .into_iter()
        .take(DEFAULT_QUICK_PICK_VISIBLE_ROWS)
        .enumerate()
        .map(|(index, item)| render_item(index, item, index == selected_index, theme, on_select.clone())),
    )
    .into_any_element()
}

/// Renders one quick-pick row.
///
/// # Parameters
///
/// `index` is the row index passed to selection callbacks.
///
/// `item` is the row content.
///
/// `selected` controls highlight state.
///
/// `theme` supplies colors.
///
/// `on_select` handles pointer selection.
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
    .justify_between()
    .h(DEFAULT_QUICK_PICK_ITEM_HEIGHT)
    .px_3()
    .py_3()
    .bg(if selected {
      theme.background.hover
    } else {
      theme.background.secondary
    })
    .hover(move |style| style.bg(theme.background.hover))
    .on_mouse_up(MouseButton::Left, move |_, window, cx| {
      on_select(index, window, cx);
    })
    .child(
      div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().text_sm().text_color(theme.text.primary).child(item.title))
        .child(div().text_xs().text_color(theme.text.secondary).child(item.subtitle)),
    )
    .child(div().text_xs().text_color(theme.text.secondary).child(shortcut))
}

/// Renders quick-pick form content.
///
/// # Parameters
///
/// `form` is the form content.
///
/// `theme` supplies colors.
///
/// # Returns
///
/// A quick-pick form element.
fn render_form(form: QuickPickForm, theme: UIThemes) -> AnyElement {
  div()
    .flex()
    .flex_col()
    .gap_2()
    .p_3()
    .child(div().text_sm().text_color(theme.text.primary).child(form.title))
    .child(
      div()
        .h(px(34.0))
        .flex()
        .items_center()
        .rounded_sm()
        .border_1()
        .border_color(theme.border.primary)
        .bg(theme.background.tertiary)
        .px_2()
        .text_xs()
        .text_color(theme.text.secondary)
        .child(form.value),
    )
    .into_any_element()
}
