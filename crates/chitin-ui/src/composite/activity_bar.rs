//! Vertical activity bar assembled from button and icon primitives.
//!
//! An activity bar is the narrow, always-visible navigation strip usually placed
//! at the far edge of an IDE window. It is similar to Visual Studio Code's
//! activity bar: each item represents a high-level workbench area such as files,
//! search, source control, tasks, extensions, or settings.
//!
//! This module intentionally stays domain-neutral. It does not know about
//! Chitin workspaces, molecules, docking jobs, or agents. Applications should
//! map their own state into
//! [`ActivityBarItem`](crate::composite::activity_bar::ActivityBarItem) values
//! and keep selection, routing, permissions, and persistence outside this crate.

use gpui::{
  App, Entity, IntoElement, ParentElement, Pixels, RenderOnce, Rgba, SharedString, Window, div, prelude::*, px,
};

use crate::{
  primitive::{
    button::{Button, ButtonSize, ButtonState, ButtonStyle, ButtonVariant},
    icon::Icon,
  },
  themes::{UIThemes, builtins},
};

/// Default width of the activity bar.
///
/// The value mirrors the compact sidebar strip commonly used by desktop IDEs:
/// wide enough for a 20-24px icon and selection indicator, but narrow enough to
/// leave horizontal space for the file tree or primary workspace panel.
pub const DEFAULT_ACTIVITY_BAR_WIDTH: Pixels = px(48.0);
/// Default radius of the badge in activity bar items.
pub const DEFAULT_ACTIVITY_BAR_BADGE_RADIUS: Pixels = px(16.0);
/// Default SVG icon size used by activity bar items.
pub const DEFAULT_ACTIVITY_BAR_ICON_WIDTH: Pixels = px(24.0);

/// A single activity bar button.
///
/// An item has a stable id, a user-facing label, and an SVG icon asset path.
/// The component owns icon sizing and state colors, while applications choose
/// which icon assets to pass.
///
/// # State
///
/// `ActivityBarItem` stores visual state such as `selected` and `disabled`, but
/// it does not mutate itself after rendering. The owning application should
/// update state and re-render in response to click handlers.
pub struct ActivityBarItem {
  id: SharedString,
  label: SharedString,
  icon_path: SharedString,
  badge: Option<SharedString>,
  button_state: Entity<ButtonState>,
  theme: UIThemes,
  selected: bool,
  disabled: bool,
}

impl ActivityBarItem {
  /// Creates a new activity bar item.
  ///
  /// `id` should be stable across renders because it is used for active-item
  /// comparison. `label` should be human-readable and suitable for future
  /// tooltip or accessibility use. `icon_path` is resolved by GPUI's asset
  /// source and should point to an SVG that can be painted with `currentColor`.
  ///
  /// # Parameters
  ///
  /// * `id` is the stable item identifier used for selection and command routing.
  /// * `label` is the human-readable item name.
  /// * `icon_path` is the asset-relative SVG path rendered by this item.
  ///
  /// # Returns
  ///
  /// An [`ActivityBarItem`] with default theme, enabled state, and no badge.
  pub fn new(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    icon_path: impl Into<SharedString>,
    button_state: Entity<ButtonState>,
  ) -> Self {
    Self {
      id: id.into(),
      label: label.into(),
      icon_path: icon_path.into(),
      badge: None,
      button_state,
      theme: builtins::dark(),
      selected: false,
      disabled: false,
    }
  }

  /// Returns this item's stable id.
  pub fn id(&self) -> &SharedString {
    &self.id
  }

  /// Returns the human-readable label for this item.
  ///
  /// The label is separate from the icon because compact activity bars often
  /// render only an icon while still needing descriptive text for tooltips,
  /// accessibility, command routing, and tests.
  pub fn label(&self) -> &SharedString {
    &self.label
  }

  /// Adds a compact badge label.
  ///
  /// Badges are intended for small counts or state markers, such as pending
  /// tasks, warnings, or background jobs. Keep badge text short so the activity
  /// bar remains narrow.
  ///
  /// The updated [`ActivityBarItem`] for builder chaining.
  pub fn badge(mut self, badge: impl Into<SharedString>) -> Self {
    self.badge = Some(badge.into());
    self
  }

  /// Overrides the visual theme used by this item.
  ///
  /// This is usually set by [`ActivityBar`] while it renders its children. It is
  /// public so callers can render individual activity items directly.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Marks the item as selected.
  ///
  /// Most callers should prefer [`ActivityBar::active_item`] over setting this
  /// manually. This method is public for direct rendering and advanced
  /// composition cases.
  pub fn selected(mut self, selected: bool) -> Self {
    self.selected = selected;
    self
  }

  /// Marks the item as disabled.
  ///
  /// Disabled items are rendered with muted styling and do not attach click
  /// handlers.
  pub fn disabled(mut self, disabled: bool) -> Self {
    self.disabled = disabled;
    self
  }

  /// Returns the text/icon color for the current item state.
  fn text_color(&self) -> Rgba {
    if self.disabled {
      self.theme.text.disabled
    } else if self.selected {
      self.theme.text.selection
    } else {
      self.theme.text.secondary
    }
  }
}

impl ActivityBarItem {
  /// Renders this item through its primitive button state.
  fn render(self, theme: UIThemes, cx: &mut App) -> gpui::Div {
    let selected = self.selected;
    let disabled = self.disabled;
    let text_color = self.text_color();
    let badge = self.badge;
    let icon_path = self.icon_path;
    let button_state = self.button_state;

    button_state.update(cx, |state, cx| state.set_disabled(disabled, cx));

    let mut item = div().relative().child(
      Button::new(button_state)
        .theme(theme)
        .variant(ButtonVariant::Transparent)
        .style(
          ButtonStyle::new()
            .width(px(40.0))
            .height(px(40.0))
            .horizontal_padding(px(0.0))
            .background(theme.background.primary)
            .hover_background(theme.background.primary)
            .pressed_background(theme.background.active)
            .foreground(text_color)
            .hover_foreground(theme.text.hover)
            .border(builtins::TRANSPARENT)
            .focus_border(builtins::TRANSPARENT),
        )
        .size(ButtonSize::Medium)
        .child(
          Icon::new(icon_path)
            .theme(theme)
            .color(text_color)
            .hover_color(theme.text.hover)
            .size(DEFAULT_ACTIVITY_BAR_ICON_WIDTH),
        ),
    );

    // A selected item receives the conventional activity-bar leading indicator.
    if selected {
      item = item.child(
        div()
          .absolute()
          .left(px(-2.0))
          .top_0()
          .w(px(2.0))
          .h(px(40.0))
          .bg(theme.text.primary),
      );
    }

    if let Some(badge) = badge {
      item = item.child(
        div()
          .absolute()
          .right_0()
          .bottom_0()
          .min_w(DEFAULT_ACTIVITY_BAR_BADGE_RADIUS)
          .h(DEFAULT_ACTIVITY_BAR_BADGE_RADIUS)
          .px_1()
          .rounded_full()
          .bg(theme.background.info)
          .flex()
          .justify_center()
          .items_center()
          .text_xs()
          .text_color(theme.accent.foreground)
          .child(badge),
      );
    }

    item
  }
}

/// A reusable vertical activity bar.
///
/// `ActivityBar` owns presentation data only: a list of top items, a list of
/// bottom items, and the id of the currently selected item. It does not manage
/// application routing or workspace state.
///
/// # Layout
///
/// Items added through [`ActivityBar::item`] are rendered from the top down.
/// Items added through [`ActivityBar::bottom_item`] are rendered from the bottom
/// up. This matches the common IDE pattern where primary navigation lives at the
/// top and account/settings controls live at the bottom.
///
/// Each item receives a persistent [`ButtonState`]. Applications subscribe to
/// its [`crate::primitive::button::ButtonEvent`] values and retain the state
/// across renders.
#[derive(IntoElement)]
pub struct ActivityBar {
  width: Pixels,
  theme: UIThemes,
  active_item_id: Option<SharedString>,
  items: Vec<ActivityBarItem>,
  bottom_items: Vec<ActivityBarItem>,
}

impl ActivityBar {
  /// Creates an empty activity bar with the default IDE-style width.
  pub fn new() -> Self {
    Self {
      width: DEFAULT_ACTIVITY_BAR_WIDTH,
      theme: builtins::dark(),
      active_item_id: None,
      items: Vec::new(),
      bottom_items: Vec::new(),
    }
  }

  /// Overrides the rendered width of the activity bar.
  ///
  /// Use this sparingly. A consistent activity bar width helps the surrounding
  /// application shell feel stable as panels open, close, and resize.
  pub fn width(mut self, width: Pixels) -> Self {
    self.width = width;
    self
  }

  /// Overrides the visual theme used by this activity bar.
  ///
  /// Components default to [`builtins::dark`], but callers can pass another
  /// [`UIThemes`] value to keep an application-wide theme consistent.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Sets the active item id.
  ///
  /// The active item receives selected styling. The id is compared against
  /// [`ActivityBarItem::id`]. Selection state is deliberately passed in from the
  /// application so this component remains stateless and reusable.
  pub fn active_item(mut self, id: impl Into<SharedString>) -> Self {
    self.active_item_id = Some(id.into());
    self
  }

  /// Adds an item to the primary, top-aligned section.
  pub fn item(mut self, item: ActivityBarItem) -> Self {
    self.items.push(item);
    self
  }

  /// Adds multiple items to the primary, top-aligned section.
  pub fn items(mut self, items: impl IntoIterator<Item = ActivityBarItem>) -> Self {
    self.items.extend(items);
    self
  }

  /// Adds an item to the secondary, bottom-aligned section.
  pub fn bottom_item(mut self, item: ActivityBarItem) -> Self {
    self.bottom_items.push(item);
    self
  }

  /// Adds multiple items to the secondary, bottom-aligned section.
  pub fn bottom_items(mut self, items: impl IntoIterator<Item = ActivityBarItem>) -> Self {
    self.bottom_items.extend(items);
    self
  }
}

impl Default for ActivityBar {
  /// Creates the default empty activity bar.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// The same value as [`ActivityBar::new`].
  fn default() -> Self {
    Self::new()
  }
}

impl RenderOnce for ActivityBar {
  /// Renders the top- and bottom-aligned activity items.
  fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
    let active_item_id = self.active_item_id;
    let theme = self.theme;

    div()
      .flex()
      .flex_col()
      .flex_none()
      .justify_between()
      .items_center()
      .h_full()
      .w(self.width)
      .py_2()
      .border_r_1()
      .border_color(theme.border.primary)
      .bg(theme.background.primary)
      .child(
        div()
          .flex()
          .flex_col()
          .items_center()
          .gap_1()
          .children(self.items.into_iter().map(|item| {
            let selected = active_item_id.as_ref().is_some_and(|active_id| active_id == &item.id);
            item.theme(theme).selected(selected).render(theme, cx)
          })),
      )
      .child(
        div()
          .flex()
          .flex_col()
          .items_center()
          .gap_1()
          .children(self.bottom_items.into_iter().map(|item| {
            let selected = active_item_id.as_ref().is_some_and(|active_id| active_id == &item.id);
            item.theme(theme).selected(selected).render(theme, cx)
          })),
      )
  }
}
