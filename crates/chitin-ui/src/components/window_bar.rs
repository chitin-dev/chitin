//! Window title bar components.
//!
//! This module provides a generic custom window bar with an app icon, title,
//! optional subtitle, and caller-supplied right-side items. It does not decide
//! which platform controls should be shown; applications provide minimize,
//! maximize, close, or other commands through
//! [`WindowBarItem`](crate::components::window_bar::WindowBarItem).

use gpui::{
  App, InteractiveElement, IntoElement, MouseButton, MouseUpEvent, ParentElement, Pixels,
  SharedString, Styled, Window, div, px, svg,
};

use crate::themes::{UIThemes, builtins};

type WindowBarItemClickListener = Box<dyn Fn(&MouseUpEvent, &mut Window, &mut App) + 'static>;

/// Default window bar height used in the window title bar
pub const DEFAULT_WINDOW_BAR_HEIGHT: Pixels = px(30.0);
/// Default app icon size used in the window title bar.
pub const DEFAULT_WINDOW_BAR_APP_ICON_SIZE: Pixels = px(20.0);
/// Default window bar minimal, maximal and close button icon size
pub const DEFAULT_WINDOW_BAR_ICON_SIZE: Pixels = px(12.0);
/// Default window button interaction area size, which should be same with
/// the height of window title bar
pub const DEFAULT_WINDOW_BAR_BUTTON_SIZE: Pixels = px(30.0);

/// A single item that can be placed in the window bar.
///
/// `WindowBarItem` represents an individual control or visual element within
/// the window's title bar. It can be used for custom window controls (like a
/// custom close button) or for displaying information (like a window title).
///
/// # Fields
///
/// * `id` - A unique identifier for the item, used for state management and
///   event routing.
/// * `label` - The text label to display alongside or instead of the icon.
/// * `icon_path` - The filesystem path to the icon asset to render.
/// * `theme` - The visual theme applied to the item, defining colors and styles.
/// * `on_click` - An optional callback that is invoked when the item is clicked.
///
/// # Examples
///
/// ```
/// use chitin_ui::components::window_bar::WindowBarItem;
///
/// let item = WindowBarItem::new(
///   "custom-close-btn",
///   "Close",
///   "assets/icons/close.svg",
/// );
/// ```
pub struct WindowBarItem {
  /// Unique identifier for this window bar item.
  id: SharedString,
  /// Text label associated with this item.
  label: SharedString,
  /// Path to the icon asset file to be displayed.
  icon_path: SharedString,
  /// Global visual theme defining the colors and styling
  theme: UIThemes,
  /// Optional click handler for the item.
  on_click: Option<WindowBarItemClickListener>,
}

impl WindowBarItem {
  /// Creates a new window bar item.
  ///
  /// `label` should be human-readable and suitable for future tooltip or
  /// accessibility use. `icon_path` is resolved by GPUI's asset
  /// source and should point to an SVG that can be painted with `currentColor`.
  ///
  /// # Parameters
  ///
  /// `id` is the stable item identifier.
  ///
  /// `label` is the human-readable item label.
  ///
  /// `icon_path` is the asset-relative SVG path rendered by this item.
  ///
  /// # Returns
  ///
  /// A [`WindowBarItem`] with the built-in dark theme and no click handler.
  pub fn new(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    icon_path: impl Into<SharedString>,
  ) -> Self {
    Self {
      id: id.into(),
      label: label.into(),
      icon_path: icon_path.into(),
      theme: builtins::dark(),
      on_click: None,
    }
  }

  /// Returns this item's stable id.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// A borrowed stable item identifier.
  pub fn id(&self) -> &SharedString {
    &self.id
  }

  /// Returns the human-readable label for this item.
  ///
  /// The label is separate from the icon because compact activity bars often
  /// render only an icon while still needing descriptive text for tooltips,
  /// accessibility, command routing, and tests.
  ///
  /// # Parameters
  ///
  /// This method reads `self`.
  ///
  /// # Returns
  ///
  /// A borrowed human-readable item label.
  pub fn label(&self) -> &SharedString {
    &self.label
  }

  /// Overrides the visual theme used by this item.
  ///
  /// This is usually set by [`WindowBar`] while it renders its children. It is
  /// public so callers can render individual activity items directly.
  ///
  /// # Parameters
  ///
  /// `theme` supplies colors used by this item.
  ///
  /// # Returns
  ///
  /// The updated [`WindowBarItem`] for builder chaining.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Registers a mouse click handler for this item.
  ///
  /// The callback receives GPUI's mouse-up event, window, and app context. The
  /// callback should update application state outside `chitin-ui`; for example,
  /// minimalizing, maximalizing and closing.
  ///
  /// # Parameters
  ///
  /// `listener` is invoked when the item receives a left mouse-up event.
  ///
  /// # Returns
  ///
  /// The updated [`WindowBarItem`] for builder chaining.
  pub fn on_click(
    mut self,
    listener: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
  ) -> Self {
    self.on_click = Some(Box::new(listener));
    self
  }
}

impl IntoElement for WindowBarItem {
  type Element = gpui::Div;

  /// Converts this item into a GPUI element.
  ///
  /// # Parameters
  ///
  /// This method consumes `self`, including its optional click listener.
  ///
  /// # Returns
  ///
  /// A GPUI `Div` containing the icon button and click behavior.
  fn into_element(self) -> Self::Element {
    let theme = self.theme;
    let mut item = div()
      .flex()
      .items_center()
      .justify_center()
      .size(DEFAULT_WINDOW_BAR_BUTTON_SIZE)
      .text_color(theme.text.secondary)
      .hover(move |style| {
        style
          .bg(theme.background.hover)
          .text_color(theme.text.primary)
      })
      .cursor_pointer()
      .child(
        svg()
          .path(self.icon_path)
          .size(DEFAULT_WINDOW_BAR_ICON_SIZE)
          .text_color(theme.text.secondary)
          .hover(move |style| style.text_color(theme.text.primary)),
      );

    if let Some(listener) = self.on_click {
      item = item.on_mouse_up(MouseButton::Left, listener);
    }

    item
  }
}

/// Position of the subtitle relative to the main title in the window bar.
///
/// Determines whether the subtitle appears to the left or right of the main
/// title text. This affects the visual layout and information hierarchy of
/// the window bar.
///
/// # Examples
///
/// ```
/// use chitin_ui::components::window_bar::{WindowBar, WindowBarSubtitlePosition};
///
/// // Subtitle on the left (e.g., version number before app name)
/// let bar = WindowBar::new("App", "icon.png", WindowBarSubtitlePosition::Left)
///   .subtitle("v1.0.0");
///
/// // Subtitle on the right (e.g., status indicator after app name)
/// let bar = WindowBar::new("App", "icon.png", WindowBarSubtitlePosition::Right)
///   .subtitle("(unsaved)");
/// ```
pub enum WindowBarSubtitlePosition {
  /// Render the subtitle before the main title.
  Left,
  /// Render the subtitle after the main title.
  Right,
}

/// A configurable window title bar with app icon, title, subtitle, and custom
/// controls.
///
/// The window bar is the topmost UI element of a window, containing the app icon,
/// window title, optional subtitle, and a set of customizable items (e.g., close,
/// minimize, maximize buttons) aligned to the right side.
///
/// # Example
///
/// ```
/// use chitin_ui::components::window_bar::{WindowBar, WindowBarSubtitlePosition};
/// use chitin_ui::themes::builtins;
///
/// let bar = WindowBar::new("My App", "assets/icon.png", WindowBarSubtitlePosition::Right)
///   .subtitle("v1.0.0")
///   .theme(builtins::light());
/// ```
pub struct WindowBar {
  /// The main title displayed in the window bar, required when creating the
  /// window bar
  title: SharedString,
  /// Optional secondary text displayed at left or right of the title.
  subtitle: Option<SharedString>,
  /// Position of the subtitle relative to the main title in the window bar.
  subtitle_position: WindowBarSubtitlePosition,
  /// App icon path, should not change after app launching
  app_icon_path: SharedString,
  /// Global visual theme defining the colors and styling
  theme: UIThemes,
  /// Window bar items rendered at the right edge.
  ///
  /// Platform-specific decoration policies are handled by the application that
  /// chooses which items to provide.
  right_items: Vec<WindowBarItem>,
}

impl WindowBar {
  /// Creates a window bar with a title, app icon, and subtitle placement.
  ///
  /// `app_icon_path` is resolved by GPUI's asset source. Right-side window
  /// controls can be added with [`Self::item`] or [`Self::items`].
  ///
  /// # Parameters
  ///
  /// `title` is the main title rendered in the center of the bar.
  ///
  /// `app_icon_path` is the asset-relative app icon path.
  ///
  /// `subtitle_position` controls whether the optional subtitle is placed
  /// before or after the title.
  ///
  /// # Returns
  ///
  /// A [`WindowBar`] with no subtitle and no right-side items.
  pub fn new(
    title: impl Into<SharedString>,
    app_icon_path: impl Into<SharedString>,
    subtitle_position: WindowBarSubtitlePosition,
  ) -> Self {
    Self {
      title: title.into(),
      subtitle: None,
      subtitle_position,
      app_icon_path: app_icon_path.into(),
      theme: builtins::dark(),
      right_items: Vec::new(),
    }
  }

  /// Sets the subtitle shown beside the main window title.
  ///
  /// # Parameters
  ///
  /// `subtitle` is the secondary title text.
  ///
  /// # Returns
  ///
  /// The updated [`WindowBar`] for builder chaining.
  pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
    self.subtitle = Some(subtitle.into());
    self
  }

  /// Overrides the visual theme used by this activity bar.
  ///
  /// Components default to [`builtins::dark`], but callers can pass another
  /// [`UIThemes`] value to keep an application-wide theme consistent.
  ///
  /// # Parameters
  ///
  /// `theme` supplies colors used by the window bar and right-side items.
  ///
  /// # Returns
  ///
  /// The updated [`WindowBar`] for builder chaining.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Adds an item to the right-aligned section.
  ///
  /// # Parameters
  ///
  /// `item` is the right-side window bar item to append.
  ///
  /// # Returns
  ///
  /// The updated [`WindowBar`] for builder chaining.
  pub fn item(mut self, item: WindowBarItem) -> Self {
    self.right_items.push(item);
    self
  }

  /// Adds multiple items to the right-aligned section.
  ///
  /// # Parameters
  ///
  /// `items` is the collection of right-side window bar items to append.
  ///
  /// # Returns
  ///
  /// The updated [`WindowBar`] for builder chaining.
  pub fn items(mut self, items: impl IntoIterator<Item = WindowBarItem>) -> Self {
    self.right_items.extend(items);
    self
  }
}

impl IntoElement for WindowBar {
  type Element = gpui::Div;

  /// Converts this window bar into a GPUI element.
  ///
  /// # Parameters
  ///
  /// This method consumes `self` and all configured right-side items.
  ///
  /// # Returns
  ///
  /// A GPUI `Div` containing app icon, title text, optional subtitle, and
  /// right-aligned items.
  fn into_element(self) -> Self::Element {
    let theme = self.theme;
    let left = div()
      .flex()
      .items_center()
      .gap_2()
      .min_w(DEFAULT_WINDOW_BAR_BUTTON_SIZE)
      .px_3()
      .child(
        svg()
          .path(self.app_icon_path)
          .size(DEFAULT_WINDOW_BAR_APP_ICON_SIZE)
          .text_color(theme.text.primary),
      );

    let concat_title = if let Some(ref subtitle) = self.subtitle {
      match self.subtitle_position {
        WindowBarSubtitlePosition::Left => format!("{} - {}", subtitle, self.title),
        WindowBarSubtitlePosition::Right => format!("{} - {}", self.title, subtitle),
      }
      .into()
    } else {
      self.title
    };

    let center = div()
      .flex()
      .flex_1()
      .items_center()
      .justify_center()
      .min_w_0()
      .child(
        div()
          .text_xs()
          .text_color(theme.text.secondary)
          .truncate()
          .child(concat_title),
      );

    div()
      .flex()
      .items_center()
      .justify_between()
      .h(DEFAULT_WINDOW_BAR_HEIGHT)
      .w_full()
      .border_b_1()
      .border_color(theme.border.primary)
      .bg(theme.background.primary)
      .child(left)
      .child(center)
      .children(self.right_items.into_iter().map(|item| item.theme(theme)))
  }
}
