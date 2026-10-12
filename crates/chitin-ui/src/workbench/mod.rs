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
  /// Continuous background beneath workbench regions.
  pub background: Hsla,
  /// Gray surface behind panel chrome and content.
  pub panel_background: Hsla,
  /// Outer region radius; compact regions have square shared edges.
  pub panel_radius: Pixels,
  /// Total separation between neighboring panel surfaces.
  pub panel_gap: Pixels,
  /// Outer region padding, independent of the inner viewport inset.
  pub panel_inset: Pixels,
  /// Subtle boundary between adjacent workbench regions.
  pub separator: Hsla,
  /// Compact height for window and tool headers.
  pub header_height: Pixels,
  /// Space between panel chrome and its inner viewport.
  pub viewport_inset: Pixels,
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
      background: theme.muted,
      panel_background: theme.muted,
      panel_radius: px(0.0),
      panel_gap: px(0.0),
      panel_inset: px(0.0),
      separator: theme.border,
      header_height: px(32.0),
      viewport_inset: px(8.0),
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
/// Adjacent slots meet directly; resize handles paint their shared boundary.
/// Kit retains ownership of the slot's focus and interactions.
pub fn surface_slot<T: Styled + ParentElement>(slot: T, style: WorkbenchStyle) -> T {
  let half_gap = style.panel_gap * 0.5;
  slot
    .relative()
    .bg(style.panel_background)
    .p(half_gap + style.panel_inset)
    .rounded(style.panel_radius)
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
    .m(style.viewport_inset)
    .rounded(style.viewport_radius)
    .bg(style.viewport_background)
    .overflow_hidden()
}

/// Paints resize feedback without changing Kit's cursor, hit band, or drag state.
pub fn resize_handle_appearance() -> ResizeHandleRenderer {
  Rc::new(|handle, _, cx| {
    let color = match handle.state() {
      ResizeHandleState::Idle => WorkbenchStyle::global(cx).separator,
      ResizeHandleState::Hovered => cx.theme().muted_foreground.opacity(0.4),
      ResizeHandleState::Pressed | ResizeHandleState::Dragging => cx.theme().muted_foreground.opacity(0.65),
    };
    let indicator = div().flex_none().bg(color);
    Some(match handle.axis() {
      Axis::Horizontal => indicator.w(px(1.0)).h_full().into_any_element(),
      Axis::Vertical => indicator.h(px(1.0)).w_full().into_any_element(),
    })
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn compact_workbench_should_have_no_outer_gaps_or_card_corners() {
    let style = WorkbenchStyle::new(*ThemeColor::dark());
    assert_eq!(
      (style.panel_gap, style.panel_inset, style.panel_radius),
      (px(0.0), px(0.0), px(0.0))
    );
    assert!(style.viewport_inset > px(0.0));
    assert!(style.viewport_radius > px(0.0));
  }

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
