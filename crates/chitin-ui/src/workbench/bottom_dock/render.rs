//! Generic bottom-dock layout and chrome.

use std::rc::Rc;

use gpui_kit::component::theme::ThemeColor;

use gpui::{
  AnyElement, App, ClickEvent, InteractiveElement, IntoElement, ParentElement, Pixels, RenderOnce, SharedString,
  Window, div, prelude::*, px,
};

use gpui_kit::component::{
  Icon, Sizable as _,
  button::{Button, ButtonVariant, ButtonVariants as _},
  resizable::{resizable_panel, v_resizable},
};

use crate::workbench::{WorkbenchStyle, panel_surface, resize_handle_appearance};

type BottomDockCloseHandler = dyn Fn(&ClickEvent, &mut Window, &mut App);
type BottomDockResizeHandler = dyn Fn(Pixels, &mut Window, &mut App);

/// Workbench-level overlay dock around one active tool's arbitrary content.
#[derive(IntoElement)]
pub struct BottomDock {
  title: SharedString,
  on_close: Option<Rc<BottomDockCloseHandler>>,
  height: Pixels,
  on_resize: Option<Rc<BottomDockResizeHandler>>,
  theme: ThemeColor,
  header_actions: Option<AnyElement>,
  navigation: Option<AnyElement>,
  maximized: bool,
  on_maximize: Option<Rc<BottomDockCloseHandler>>,
  child: Option<AnyElement>,
}

impl BottomDock {
  /// Creates an empty dock for the active tool represented by `title`.
  pub fn new(title: impl Into<SharedString>) -> Self {
    Self {
      title: title.into(),
      on_close: None,
      height: super::DEFAULT_BOTTOM_DOCK_HEIGHT,
      on_resize: None,
      theme: *ThemeColor::dark(),
      header_actions: None,
      navigation: None,
      maximized: false,
      on_maximize: None,
      child: None,
    }
  }

  /// Sets the initial height used when Kit mounts the overlay.
  pub fn height(mut self, height: Pixels) -> Self {
    self.height = height;
    self
  }

  /// Reports the measured bottom pane height after a Kit resize completes.
  ///
  /// # Parameters
  ///
  /// * `on_resize` stores the measured extent; it must not implement its own
  ///   pointer tracking. Kit owns drag state, constraints, and cursor behavior.
  ///
  /// # Returns
  ///
  /// The configured dock. Its overlay never participates in document layout.
  pub fn on_resize(mut self, on_resize: impl Fn(Pixels, &mut Window, &mut App) + 'static) -> Self {
    self.on_resize = Some(Rc::new(on_resize));
    self
  }

  /// Sets the callback invoked when the dock's close control is activated.
  ///
  /// The dock only owns the control's appearance; closing the active tool is
  /// the application's decision, so the callback carries it.
  pub fn on_close(mut self, on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
    self.on_close = Some(Rc::new(on_close));
    self
  }

  /// Sets the semantic UI theme.
  pub fn theme(mut self, theme: ThemeColor) -> Self {
    self.theme = theme;
    self
  }

  /// Adds tool-specific controls before the dock's close button.
  pub fn header_actions(mut self, actions: impl IntoElement) -> Self {
    self.header_actions = Some(actions.into_any_element());
    self
  }

  /// Supplies tool tabs independently of the active tool's content.
  pub fn navigation(mut self, navigation: impl IntoElement) -> Self {
    self.navigation = Some(navigation.into_any_element());
    self
  }

  /// Sets whether the overlay occupies the full central area.
  pub fn maximized(mut self, maximized: bool) -> Self {
    self.maximized = maximized;
    self
  }

  /// Handles maximize/restore without changing document allocation.
  pub fn on_maximize(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
    self.on_maximize = Some(Rc::new(handler));
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
    let style = WorkbenchStyle::new(theme);
    let dock = panel_surface(style)
      .occlude()
      .on_scroll_wheel(|_, _, cx| {
        cx.stop_propagation();
      })
      .child(
        div()
          .flex()
          .items_center()
          .justify_between()
          .h(super::BOTTOM_DOCK_BAR_HEIGHT)
          .min_h(super::BOTTOM_DOCK_BAR_HEIGHT)
          .flex_none()
          .border_b_1()
          .border_color(style.separator)
          .px_2()
          .child(self.navigation.unwrap_or_else(|| {
            div()
              .text_xs()
              .font_weight(gpui::FontWeight::SEMIBOLD)
              .child(self.title)
              .into_any_element()
          }))
          .child(
            div()
              .flex()
              .items_center()
              .gap_1()
              .when_some(self.header_actions, |actions, header_actions| {
                actions.child(header_actions)
              })
              .when_some(self.on_maximize, |actions, on_maximize| {
                actions.child(
                  Button::new("bottom-dock-maximize")
                    .with_variant(ButtonVariant::Ghost)
                    .tooltip(if self.maximized {
                      "Restore dock"
                    } else {
                      "Maximize dock"
                    })
                    .w(px(26.0))
                    .h(px(24.0))
                    .px(px(0.0))
                    .on_click(move |event, window, cx| on_maximize(event, window, cx))
                    .child(Icon::default().path("icons/window-maximize.svg").with_size(px(14.0))),
                )
              })
              .child(
                Button::new("bottom-dock-close")
                  .with_variant(ButtonVariant::Ghost)
                  .tooltip("Hide dock")
                  .cursor_pointer()
                  .w(px(26.0))
                  .h(px(24.0))
                  .px(px(0.0))
                  .when_some(self.on_close, |button, on_close| {
                    button.on_click(move |event, window, cx| on_close(event, window, cx))
                  })
                  .child(Icon::default().path("icons/window-close.svg").with_size(px(14.0))),
              ),
          ),
      )
      .when_some(self.child, |dock, child| dock.child(child));

    if self.maximized {
      return div().absolute().inset_0().child(dock);
    }

    // The document is not a child of this split: the upper pane is transparent
    // and non-occluding, so interaction passes through to the full-size document.
    div().absolute().inset_0().child(
      v_resizable("workbench-bottom-dock-overlay")
        .with_handle_appearance(resize_handle_appearance())
        .child(resizable_panel().size_range(super::DEFAULT_CENTER_AREA_MIN_HEIGHT..px(f32::MAX)))
        .child(
          resizable_panel()
            .size(self.height)
            .size_range(super::DEFAULT_BOTTOM_DOCK_MIN_HEIGHT..px(f32::MAX))
            .child(dock),
        )
        .when_some(self.on_resize, |group, on_resize| {
          group.on_resize(move |state, window, cx| {
            if let Some(height) = state.read(cx).sizes().get(1).copied() {
              on_resize(height, window, cx);
            }
          })
        }),
    )
  }
}
