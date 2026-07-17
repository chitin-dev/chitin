//! Vertical activity bar components.
//!
//! An activity bar is the narrow, always-visible navigation strip usually placed
//! at the far edge of an IDE window. It is similar to Visual Studio Code's
//! activity bar: each item represents a high-level workbench area such as files,
//! search, source control, tasks, extensions, or settings.
//!
//! This module intentionally stays domain-neutral. It does not know about
//! Chitin workspaces, molecules, docking jobs, or agents. Applications should
//! map their own state into
//! [`ActivityBarItem`](crate::components::activity_bar::ActivityBarItem) values and keep selection,
//! routing, permissions, and persistence outside this crate.

use gpui::{
  App, IntoElement, MouseButton, MouseUpEvent, Pixels, Rgba, SharedString, Window, div, prelude::*,
  px, svg,
};

use crate::themes::{UIThemes, builtins};

type ActivityBarItemClickListener = Box<dyn Fn(&MouseUpEvent, &mut Window, &mut App) + 'static>;

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
  theme: UIThemes,
  selected: bool,
  disabled: bool,
  on_click: Option<ActivityBarItemClickListener>,
}

impl ActivityBarItem {
  /// Creates a new activity bar item.
  ///
  /// `id` should be stable across renders because it is used for active-item
  /// comparison. `label` should be human-readable and suitable for future
  /// tooltip or accessibility use. `icon_path` is resolved by GPUI's asset
  /// source and should point to an SVG that can be painted with `currentColor`.
  pub fn new(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    icon_path: impl Into<SharedString>,
  ) -> Self {
    Self {
      id: id.into(),
      label: label.into(),
      icon_path: icon_path.into(),
      badge: None,
      theme: builtins::dark(),
      selected: false,
      disabled: false,
      on_click: None,
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

  /// Registers a mouse click handler for this item.
  ///
  /// The callback receives GPUI's mouse-up event, window, and app context. The
  /// callback should update application state outside `chitin-ui`; for example,
  /// switching the active panel in `chitin-desktop`.
  pub fn on_click(
    mut self,
    listener: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
  ) -> Self {
    self.on_click = Some(Box::new(listener));
    self
  }

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

impl IntoElement for ActivityBarItem {
  type Element = gpui::Div;

  fn into_element(self) -> Self::Element {
    let selected = self.selected;
    let disabled = self.disabled;
    let text_color = self.text_color();
    let badge = self.badge;
    let theme = self.theme;
    let icon_path = self.icon_path;

    let mut icon = svg()
      .path(icon_path)
      .size(DEFAULT_ACTIVITY_BAR_ICON_WIDTH)
      .text_color(text_color);

    if !disabled {
      icon = icon.hover(move |style| style.text_color(theme.accent.primary));
    }

    let mut item = div()
      .relative()
      .flex()
      .items_center()
      .justify_center()
      .size(px(40.0))
      .rounded_sm()
      .bg(theme.background.primary)
      .text_color(text_color)
      .child(
        div()
          .flex()
          .items_center()
          .justify_center()
          .size(DEFAULT_ACTIVITY_BAR_ICON_WIDTH)
          .child(icon),
      );

    if !disabled {
      item = item
        .hover(move |style| style.text_color(theme.accent.primary))
        .cursor_pointer();
    }

    // This creates the effect of left callout block
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

    // Badges currently use the theme's informational color. Status-specific
    // colors should be added when the badge API grows a semantic status field.
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

    if !disabled && let Some(listener) = self.on_click {
      item = item.on_mouse_up(MouseButton::Left, listener);
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
/// # Example
///
/// ```no_run
/// use chitin_ui::components::activity_bar::{ActivityBar, ActivityBarItem};
///
/// let activity_bar = ActivityBar::new()
///   .active_item("files")
///   .item(ActivityBarItem::new("files", "Files", "icons/activity-bar/files.svg"))
///   .item(ActivityBarItem::new("search", "Search", "icons/activity-bar/search.svg"))
///   .bottom_item(ActivityBarItem::new(
///     "settings",
///     "Settings",
///     "icons/activity-bar/settings.svg",
///   ));
/// ```
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
  fn default() -> Self {
    Self::new()
  }
}

impl IntoElement for ActivityBar {
  type Element = gpui::Div;

  fn into_element(self) -> Self::Element {
    let active_item_id = self.active_item_id;
    let theme = self.theme;

    div()
      .flex()
      .flex_col()
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
            let selected = active_item_id
              .as_ref()
              .is_some_and(|active_id| active_id == &item.id);
            item.theme(theme).selected(selected)
          })),
      )
      .child(div().flex().flex_col().items_center().gap_1().children(
        self.bottom_items.into_iter().map(|item| {
          let selected = active_item_id
            .as_ref()
            .is_some_and(|active_id| active_id == &item.id);
          item.theme(theme).selected(selected)
        }),
      ))
  }
}
