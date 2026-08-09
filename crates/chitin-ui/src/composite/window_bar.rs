//! A custom title bar assembled from button and icon primitives.

use gpui::{Entity, IntoElement, ParentElement, Pixels, SharedString, div, prelude::*, px};

use crate::{
  primitive::{
    button::{Button, ButtonSize, ButtonState, ButtonStyle, ButtonVariant},
    icon::Icon,
    text::{Text, TextSize, TextState},
  },
  themes::{UIThemes, builtins},
};

/// Default window-bar height.
pub const DEFAULT_WINDOW_BAR_HEIGHT: Pixels = px(30.0);
/// Default app-icon size in the window bar.
pub const DEFAULT_WINDOW_BAR_APP_ICON_SIZE: Pixels = px(20.0);
/// Default size for right-side control icons.
pub const DEFAULT_WINDOW_BAR_ICON_SIZE: Pixels = px(12.0);

/// A right-aligned window-bar control backed by a primitive button state.
pub struct WindowBarItem {
  id: SharedString,
  label: SharedString,
  icon_path: SharedString,
  button_state: Entity<ButtonState>,
}

impl WindowBarItem {
  /// Creates a window-bar control from a stable id, label, icon path, and button state.
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
      button_state,
    }
  }

  /// Returns this control's stable identifier.
  pub fn id(&self) -> &SharedString {
    &self.id
  }

  /// Returns this control's human-readable label.
  pub fn label(&self) -> &SharedString {
    &self.label
  }

  fn render(self, theme: UIThemes) -> Button {
    Button::new(self.button_state)
      .theme(theme)
      .variant(ButtonVariant::Transparent)
      .style(
        ButtonStyle::new()
          .width(DEFAULT_WINDOW_BAR_HEIGHT)
          .horizontal_padding(px(0.0))
          .focus_border(builtins::TRANSPARENT),
      )
      .size(ButtonSize::Medium)
      .child(
        Icon::new(self.icon_path)
          .theme(theme)
          .size(DEFAULT_WINDOW_BAR_ICON_SIZE),
      )
  }
}

/// Position of the subtitle relative to the main title.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowBarSubtitlePosition {
  /// Render the subtitle before the main title.
  Left,
  /// Render the subtitle after the main title.
  Right,
}

/// A custom title bar composed from primitive icons and buttons.
pub struct WindowBar {
  title: Entity<TextState>,
  subtitle: Option<SharedString>,
  subtitle_position: WindowBarSubtitlePosition,
  app_icon_path: SharedString,
  theme: UIThemes,
  right_items: Vec<WindowBarItem>,
}

impl WindowBar {
  /// Creates a window bar with title state, app icon, and subtitle placement.
  pub fn new(
    title: Entity<TextState>,
    app_icon_path: impl Into<SharedString>,
    subtitle_position: WindowBarSubtitlePosition,
  ) -> Self {
    Self {
      title,
      subtitle: None,
      subtitle_position,
      app_icon_path: app_icon_path.into(),
      theme: builtins::dark(),
      right_items: Vec::new(),
    }
  }

  /// Sets the subtitle displayed beside the main title.
  pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
    self.subtitle = Some(subtitle.into());
    self
  }

  /// Sets the semantic theme used by this window bar and its controls.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Appends a right-aligned window control.
  pub fn item(mut self, item: WindowBarItem) -> Self {
    self.right_items.push(item);
    self
  }

  /// Appends several right-aligned window controls.
  pub fn items(mut self, items: impl IntoIterator<Item = WindowBarItem>) -> Self {
    self.right_items.extend(items);
    self
  }
}

impl IntoElement for WindowBar {
  type Element = gpui::Div;

  fn into_element(self) -> Self::Element {
    let theme = self.theme;
    let left = div()
      .flex()
      .items_center()
      .gap_2()
      .min_w(DEFAULT_WINDOW_BAR_HEIGHT)
      .px_3()
      .child(
        Icon::new(self.app_icon_path)
          .theme(theme)
          .color(theme.text.primary)
          .size(DEFAULT_WINDOW_BAR_APP_ICON_SIZE),
      );

    let title = Text::new(self.title).theme(theme).size(TextSize::Small);
    let title = match self.subtitle {
      Some(subtitle) => match self.subtitle_position {
        WindowBarSubtitlePosition::Left => div()
          .flex()
          .items_center()
          .gap_1()
          .child(
            div()
              .text_xs()
              .text_color(theme.text.secondary)
              .child(format!("{subtitle} -")),
          )
          .child(title),
        WindowBarSubtitlePosition::Right => div().flex().items_center().gap_1().child(title).child(
          div()
            .text_xs()
            .text_color(theme.text.secondary)
            .child(format!("- {subtitle}")),
        ),
      },
      None => div().child(title),
    };

    let center = div()
      .flex()
      .flex_1()
      .items_center()
      .justify_center()
      .min_w_0()
      .child(div().truncate().child(title));

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
      .children(self.right_items.into_iter().map(|item| item.render(theme)))
  }
}
