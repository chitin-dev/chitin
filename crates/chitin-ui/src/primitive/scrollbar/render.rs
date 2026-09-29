//! Rendering and visual customization for the scrollbar primitive.

use gpui::{
  App, CursorStyle, Entity, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels, RenderOnce,
  Window, div, prelude::*, px, relative,
};

use super::{ScrollbarMetrics, ScrollbarState};
use crate::themes::{UIThemes, builtins};

/// Track width and thumb width for one visual size of a [`Scrollbar`].
#[derive(Clone, Copy, Debug)]
struct ScrollbarSizeMetrics {
  /// Width of the interactive track along the trailing edge.
  lane_width: Pixels,
  /// Width of the painted thumb inside the track.
  thumb_width: Pixels,
}

/// Visual size for a [`Scrollbar`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollbarSize {
  /// Narrow bar for dense panes.
  Small,
  /// Default bar.
  #[default]
  Medium,
}

impl ScrollbarSize {
  /// Returns the track and thumb widths for this size.
  fn metrics(self) -> ScrollbarSizeMetrics {
    match self {
      Self::Small => ScrollbarSizeMetrics {
        lane_width: px(9.0),
        thumb_width: px(4.0),
      },
      Self::Medium => ScrollbarSizeMetrics {
        lane_width: px(13.0),
        thumb_width: px(6.0),
      },
    }
  }
}

/// A draggable bar that reports where a viewport should sit.
///
/// The bar is laid out over the viewport it scrolls and pinned to the trailing
/// edge, so its height is the viewport height. It paints nothing while the
/// content fits, and while idle it only occupies its lane, leaving the rest of
/// the area free for the content underneath.
#[derive(IntoElement)]
pub struct Scrollbar {
  state: Entity<ScrollbarState>,
  metrics: ScrollbarMetrics,
  theme: UIThemes,
  size: ScrollbarSize,
}

impl Scrollbar {
  /// Creates a scrollbar that reports motions through `state`.
  pub fn new(state: Entity<ScrollbarState>) -> Self {
    Self {
      state,
      metrics: ScrollbarMetrics::new(Pixels::ZERO, Pixels::ZERO, Pixels::ZERO),
      theme: builtins::dark(),
      size: ScrollbarSize::default(),
    }
  }

  /// Sets the content geometry this bar describes.
  pub fn metrics(mut self, metrics: ScrollbarMetrics) -> Self {
    self.metrics = metrics;
    self
  }

  /// Sets the semantic theme used for this bar.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Sets the visual size.
  pub fn size(mut self, size: ScrollbarSize) -> Self {
    self.size = size;
    self
  }
}

impl RenderOnce for Scrollbar {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let dragging = self.state.read(cx).is_dragging();
    let mut root = div().absolute().inset_0();
    if !self.metrics.is_scrollable() {
      return root;
    }

    let size = self.size.metrics();
    let theme = self.theme;
    let metrics = self.metrics;
    let thumb_top = metrics.thumb_top_fraction();
    let thumb_extent = metrics.thumb_extent_fraction();

    let thumb_color: Hsla = if dragging {
      theme.text.secondary
    } else {
      theme.border.tertiary
    }
    .into();

    let state_for_thumb = self.state.clone();
    let state_for_before = self.state.clone();
    let state_for_after = self.state.clone();
    let state_for_move = self.state.clone();
    let state_for_up = self.state.clone();

    // While dragging, the pointer is free to leave the narrow lane, so the whole
    // area takes the move and release. This layer only exists during a drag;
    // an idle bar inserts no hitbox here and the content underneath keeps its
    // own pointer behaviour.
    if dragging {
      root = root
        .on_mouse_move(move |event, _, cx| {
          state_for_move.update(cx, |state, cx| {
            state.drag(event.position.y, event.pressed_button, metrics, cx);
          });
        })
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
          state_for_up.update(cx, |state, cx| state.end_drag(cx));
        });
    }

    root.child(
      div()
        .absolute()
        .top_0()
        .bottom_0()
        .right_0()
        .w(size.lane_width)
        .cursor(CursorStyle::Arrow)
        .hover(move |style| style.bg(theme.background.hover))
        .child(
          div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(relative(thumb_top))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
              state_for_before.update(cx, |state, cx| state.scroll_page(metrics, false, cx));
              cx.stop_propagation();
            }),
        )
        .child(
          div()
            .absolute()
            .top(relative(thumb_top))
            .h(relative(thumb_extent))
            .right(px(3.0))
            .w(size.thumb_width)
            .rounded_full()
            .bg(thumb_color)
            .on_mouse_down(MouseButton::Left, move |event, _, cx| {
              state_for_thumb.update(cx, |state, cx| state.begin_drag(event.position.y, metrics, cx));
              cx.stop_propagation();
            }),
        )
        .child(
          div()
            .absolute()
            .top(relative(thumb_top + thumb_extent))
            .bottom_0()
            .left_0()
            .right_0()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
              state_for_after.update(cx, |state, cx| state.scroll_page(metrics, true, cx));
              cx.stop_propagation();
            }),
        ),
    )
  }
}
