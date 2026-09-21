//! Generic bottom-dock layout and chrome.

use std::rc::Rc;

use gpui::{
  AnyElement, App, CursorStyle, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
  RenderOnce, SharedString, Window, div, prelude::*, px,
};

use crate::{
  primitive::{
    button::{Button, ButtonSize, ButtonState, ButtonStyle, ButtonVariant},
    icon::Icon,
  },
  themes::{UIThemes, builtins},
};

type BottomDockResizeStartHandler = dyn Fn(Pixels, &mut Window, &mut App);
type BottomDockResizeHandler = dyn Fn(Pixels, &mut Window, &mut App);
type BottomDockResizeEndHandler = dyn Fn(&mut Window, &mut App);

/// Configuration for the dock's top-edge resize handle.
#[derive(Clone)]
pub struct BottomDockResizeConfig {
  handle_height: Pixels,
  on_resize_start: Rc<BottomDockResizeStartHandler>,
  on_resize: Option<Rc<BottomDockResizeHandler>>,
  on_resize_end: Option<Rc<BottomDockResizeEndHandler>>,
}

impl BottomDockResizeConfig {
  /// Creates resize configuration with the standard interactive handle height.
  pub fn new(on_resize_start: impl Fn(Pixels, &mut Window, &mut App) + 'static) -> Self {
    Self {
      handle_height: px(5.0),
      on_resize_start: Rc::new(on_resize_start),
      on_resize: None,
      on_resize_end: None,
    }
  }

  /// Sets the interactive resize-handle height.
  pub fn handle_height(mut self, handle_height: Pixels) -> Self {
    self.handle_height = handle_height;
    self
  }

  /// Sets the callback invoked while the pointer moves inside the dock.
  pub fn on_resize(mut self, on_resize: impl Fn(Pixels, &mut Window, &mut App) + 'static) -> Self {
    self.on_resize = Some(Rc::new(on_resize));
    self
  }

  /// Sets the callback invoked when a resize ends inside the dock.
  pub fn on_resize_end(mut self, on_resize_end: impl Fn(&mut Window, &mut App) + 'static) -> Self {
    self.on_resize_end = Some(Rc::new(on_resize_end));
    self
  }
}

/// Workbench-level overlay dock around one active tool's arbitrary content.
#[derive(IntoElement)]
pub struct BottomDock {
  title: SharedString,
  close: Entity<ButtonState>,
  height: Pixels,
  theme: UIThemes,
  resize: Option<BottomDockResizeConfig>,
  child: Option<AnyElement>,
}

impl BottomDock {
  /// Creates an empty dock for the active tool represented by `title`.
  pub fn new(title: impl Into<SharedString>, close: Entity<ButtonState>) -> Self {
    Self {
      title: title.into(),
      close,
      height: super::DEFAULT_BOTTOM_DOCK_HEIGHT,
      theme: builtins::dark(),
      resize: None,
      child: None,
    }
  }

  /// Sets the dock height.
  pub fn height(mut self, height: Pixels) -> Self {
    self.height = height;
    self
  }

  /// Sets the semantic UI theme.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Enables top-edge resizing with the supplied callback configuration.
  pub fn resizable(mut self, resize: BottomDockResizeConfig) -> Self {
    self.resize = Some(resize);
    self
  }

  /// Sets the active dock item's content.
  pub fn child(mut self, child: impl IntoElement) -> Self {
    self.child = Some(child.into_any_element());
    self
  }
}

impl RenderOnce for BottomDock {
  fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
    let theme = self.theme;
    div()
      .absolute()
      .left_0()
      .right_0()
      .bottom_0()
      .flex()
      .flex_col()
      .h(self.height)
      .min_h(self.height)
      .border_t_1()
      .border_color(theme.border.primary)
      .bg(theme.background.primary)
      .occlude()
      .on_scroll_wheel(|_, _, cx| {
        cx.stop_propagation();
      })
      .child(
        div()
          .flex()
          .items_center()
          .justify_between()
          .h(px(34.0))
          .min_h(px(34.0))
          .px_3()
          .border_b_1()
          .border_color(theme.border.muted)
          .child(
            div()
              .text_xs()
              .font_weight(gpui::FontWeight::SEMIBOLD)
              .child(self.title),
          )
          .child(
            Button::new(self.close)
              .theme(theme)
              .variant(ButtonVariant::Transparent)
              .size(ButtonSize::Small)
              .style(
                ButtonStyle::new()
                  .width(px(26.0))
                  .height(px(24.0))
                  .horizontal_padding(px(0.0)),
              )
              .child(Icon::new("icons/window-close.svg").size(px(14.0)).theme(theme)),
          ),
      )
      .when_some(self.child, |dock, child| dock.child(child))
      .when_some(self.resize, |dock, resize| {
        let on_resize_start = resize.on_resize_start.clone();
        dock
          .when_some(resize.on_resize, |dock, on_resize| {
            dock.on_mouse_move(move |event, window, cx| {
              on_resize(event.position.y, window, cx);
            })
          })
          .when_some(resize.on_resize_end, |dock, on_resize_end| {
            dock.on_mouse_up(MouseButton::Left, move |_, window, cx| {
              on_resize_end(window, cx);
            })
          })
          .child(
            div()
              .absolute()
              .top_0()
              .left_0()
              .right_0()
              .h(resize.handle_height)
              .cursor(CursorStyle::ResizeUpDown)
              .hover(move |style| style.bg(theme.border.focus))
              .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                on_resize_start(event.position.y, window, cx);
              }),
          )
      })
  }
}
