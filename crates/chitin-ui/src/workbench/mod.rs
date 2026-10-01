//! Shared workbench surfaces and layout spacing over Kit's interaction layers.

use std::rc::Rc;

use gpui::{App, Axis, Bounds, Div, Hsla, IntoElement, ParentElement, Pixels, Point, Styled, div, px};
use gpui_kit::base::{ResizeHandleRenderer, ResizeHandleState};
use gpui_kit::component::{ActiveTheme, theme::ThemeColor};

/// Vertical workbench activity-bar composition.
pub mod activity_bar;
/// Resizable workbench tools dock.
pub mod bottom_dock;
/// Kit docking appearance for independently surfaced workbench panes.
pub mod dock;

/// Semantic surface tokens resolved from Kit's palette.
#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct WorkbenchStyle {
  /// Background exposed between major surfaces.
  pub background: Hsla,
  /// Gray surface behind panel chrome and content.
  pub panel_background: Hsla,
  /// Outer panel corner radius.
  pub panel_radius: Pixels,
  /// Total separation between neighboring panel surfaces.
  pub panel_gap: Pixels,
  /// Padding protecting outer corners from rectangular child backgrounds.
  pub panel_inset: Pixels,
  /// Background of a scientific viewport or terminal content.
  pub viewport_background: Hsla,
  /// Inner viewport corner radius.
  pub viewport_radius: Pixels,
}

/// Converts a window pointer to viewport-local logical pixels, rejecting rounded corners.
pub fn viewport_position(bounds: Bounds<Pixels>, radius: Pixels, position: Point<Pixels>) -> Option<Point<Pixels>> {
  if !bounds.contains(&position) {
    return None;
  }
  let local = position - bounds.origin;
  let half_width = f32::from(bounds.size.width) * 0.5;
  let half_height = f32::from(bounds.size.height) * 0.5;
  let radius = f32::from(radius).max(0.0).min(half_width.min(half_height));
  let dx = (f32::from(local.x) - half_width).abs() - half_width + radius;
  let dy = (f32::from(local.y) - half_height).abs() - half_height + radius;
  let distance = dx.max(dy).min(0.0) + dx.max(0.0).hypot(dy.max(0.0)) - radius;
  (distance <= 0.0).then_some(local)
}

impl WorkbenchStyle {
  /// Resolves workbench surfaces without changing Kit's global palette.
  pub fn new(theme: ThemeColor) -> Self {
    Self {
      background: theme.background,
      panel_background: theme.muted,
      panel_radius: px(8.0),
      panel_gap: px(8.0),
      panel_inset: px(4.0),
      viewport_background: theme.background,
      viewport_radius: px(6.0),
    }
  }

  /// Returns tokens for the current Kit theme.
  pub fn global(cx: &App) -> Self {
    Self::new(cx.theme().colors)
  }
}

/// Decorates one layout slot with a surface, without changing its outer bounds.
///
/// Half the gap belongs to each slot. The workbench adds the other half at its
/// outside edge. An inset background is painted before the slot's content;
/// children stay inside its corner region because GPUI's overflow mask is
/// rectangular. Kit retains ownership of the slot's focus and interactions.
pub fn surface_slot<T: Styled + ParentElement>(slot: T, style: WorkbenchStyle) -> T {
  let half_gap = style.panel_gap * 0.5;
  slot
    .relative()
    .bg(gpui::transparent_black())
    .p(half_gap + style.panel_inset)
    .child(
      div()
        .absolute()
        .top(half_gap)
        .bottom(half_gap)
        .left(half_gap)
        .right(half_gap)
        .rounded(style.panel_radius)
        .bg(style.panel_background),
    )
}

/// Creates a content column occupying one major surface slot.
pub fn panel_surface(style: WorkbenchStyle) -> Div {
  surface_slot(div().flex().flex_col().size_full().min_w_0().min_h_0(), style)
}

/// Creates the inner viewport frame; the hosted renderer must clip its output.
pub fn viewport_container(style: WorkbenchStyle) -> Div {
  div()
    .relative()
    .flex()
    .flex_col()
    .flex_1()
    .min_w_0()
    .min_h_0()
    .rounded(style.viewport_radius)
    .bg(style.viewport_background)
    .overflow_hidden()
}

/// Paints resize feedback without changing Kit's cursor, hit band, or drag state.
pub fn resize_handle_appearance() -> ResizeHandleRenderer {
  Rc::new(|handle, _, cx| {
    let (length, opacity) = match handle.state() {
      ResizeHandleState::Idle => (px(0.0), 0.0),
      ResizeHandleState::Hovered => (px(20.0), 0.25),
      ResizeHandleState::Pressed | ResizeHandleState::Dragging => (px(32.0), 0.5),
    };
    let indicator = div()
      .flex_none()
      .rounded_full()
      .bg(cx.theme().muted_foreground)
      .opacity(opacity);
    Some(match handle.axis() {
      Axis::Horizontal => indicator.w(px(1.0)).h(length).into_any_element(),
      Axis::Vertical => indicator.h(px(1.0)).w(length).into_any_element(),
    })
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn viewport_coordinates_use_inner_origin_and_exclude_clipped_corners() {
    let bounds = Bounds::new(gpui::point(px(120.0), px(80.0)), gpui::size(px(200.0), px(100.0)));
    assert_eq!(
      viewport_position(bounds, px(6.0), gpui::point(px(140.0), px(110.0))),
      Some(gpui::point(px(20.0), px(30.0)))
    );
    assert_eq!(viewport_position(bounds, px(6.0), bounds.origin), None);
    assert_eq!(
      viewport_position(bounds, px(6.0), gpui::point(px(119.0), px(100.0))),
      None
    );
    assert!(viewport_position(bounds, px(6.0), gpui::point(px(220.0), px(81.0))).is_some());
  }
}
