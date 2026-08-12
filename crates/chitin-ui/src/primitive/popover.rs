// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Chitin Contributors

//! Window-bounded modal popovers with arbitrary scrollable content.

use std::rc::Rc;

use gpui::{
  Anchored, AnchoredPositionMode, AnyElement, App, Corner, ElementId, InteractiveElement, IntoElement, ParentElement,
  Pixels, Point, RenderOnce, ScrollHandle, Size, Window, anchored, deferred, div, point, prelude::*, px,
};

use crate::themes::{UIThemes, builtins};

/// Relative placement used by a [`Popover`] when no custom offset is supplied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopoverPlacement {
  /// Places the popup directly beneath its anchor.
  #[default]
  Below,
  /// Places the popup directly above its anchor.
  Above,
}

/// Visual-only overrides for a [`Popover`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PopoverStyle {
  /// Optional fixed popup width.
  width: Option<Pixels>,
  /// Optional fixed popup height.
  height: Option<Pixels>,
  /// Optional maximum popup width.
  max_width: Option<Pixels>,
  /// Optional maximum popup height before content scrolls.
  max_height: Option<Pixels>,
  /// Optional popup surface color.
  background: Option<gpui::Rgba>,
  /// Optional popup border color.
  border_color: Option<gpui::Rgba>,
  /// Optional popup border width.
  border_width: Option<Pixels>,
}

impl PopoverStyle {
  /// Creates popover style with no visual overrides.
  pub fn new() -> Self {
    Self::default()
  }

  /// Sets a fixed popup width.
  pub fn width(mut self, width: Pixels) -> Self {
    self.width = Some(width);
    self
  }

  /// Sets a fixed popup height.
  pub fn height(mut self, height: Pixels) -> Self {
    self.height = Some(height);
    self
  }

  /// Sets the maximum popup width.
  pub fn max_width(mut self, width: Pixels) -> Self {
    self.max_width = Some(width);
    self
  }

  /// Sets the maximum popup height before its content scrolls.
  pub fn max_height(mut self, height: Pixels) -> Self {
    self.max_height = Some(height);
    self
  }

  /// Sets the popup surface color.
  pub fn background(mut self, color: gpui::Rgba) -> Self {
    self.background = Some(color);
    self
  }

  /// Sets the popup border color.
  pub fn border_color(mut self, color: gpui::Rgba) -> Self {
    self.border_color = Some(color);
    self
  }

  /// Sets the popup border width.
  pub fn border_width(mut self, width: Pixels) -> Self {
    self.border_width = Some(width);
    self
  }
}

/// Callback invoked after a modal popover is dismissed by an outside click.
pub type PopoverDismissHandler = dyn Fn(&mut Window, &mut App);

/// A modal popup anchored to a parent element and bounded by the application window.
#[derive(IntoElement)]
pub struct Popover {
  /// Stable identity that preserves the internal scroll position.
  id: ElementId,
  /// Arbitrary content rendered inside the popup viewport.
  content: AnyElement,
  /// Anchor-relative placement used unless a custom offset is supplied.
  placement: PopoverPlacement,
  /// Optional anchor-relative popup origin override.
  offset: Option<Point<Pixels>>,
  /// Anchor dimensions used by built-in placement wrappers.
  anchor_size: Size<Pixels>,
  /// Optional anchor origin in window coordinates.
  anchor_position: Option<Point<Pixels>>,
  /// Visual-only surface overrides.
  style: PopoverStyle,
  /// Semantic colors used when style fields are absent.
  theme: UIThemes,
  /// Persistent vertical scroll position for popup content.
  scroll_handle: ScrollHandle,
  /// Callback used to close the owning component after an outside click.
  on_dismiss: Option<Rc<PopoverDismissHandler>>,
}

impl Popover {
  /// Creates a popover with default below-anchor placement.
  pub fn new(id: impl Into<ElementId>, content: impl IntoElement) -> Self {
    Self {
      id: id.into(),
      content: content.into_any_element(),
      placement: PopoverPlacement::default(),
      offset: None,
      anchor_size: Size::default(),
      anchor_position: None,
      style: PopoverStyle::default(),
      theme: builtins::dark(),
      scroll_handle: ScrollHandle::new(),
      on_dismiss: None,
    }
  }

  /// Sets the anchor dimensions used by [`PopoverPlacement`].
  pub fn anchor_size(mut self, anchor_size: Size<Pixels>) -> Self {
    self.anchor_size = anchor_size;
    self
  }

  /// Sets the anchor origin in window coordinates.
  pub fn anchor_position(mut self, anchor_position: Point<Pixels>) -> Self {
    self.anchor_position = Some(anchor_position);
    self
  }

  /// Sets a built-in anchor-relative popup placement.
  pub fn placement(mut self, placement: PopoverPlacement) -> Self {
    self.placement = placement;
    self
  }

  /// Overrides the popup origin relative to its anchor's top-left corner.
  pub fn offset(mut self, offset: Point<Pixels>) -> Self {
    self.offset = Some(offset);
    self
  }

  /// Applies visual-only popup surface overrides.
  pub fn style(mut self, style: PopoverStyle) -> Self {
    self.style = style;
    self
  }

  /// Sets semantic colors used by the default popup surface.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Reuses a scroll handle across renders.
  pub fn scroll_handle(mut self, scroll_handle: ScrollHandle) -> Self {
    self.scroll_handle = scroll_handle;
    self
  }

  /// Registers a callback invoked after an outside click dismisses the popup.
  pub fn on_dismiss(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
    self.on_dismiss = Some(Rc::new(handler));
    self
  }

  /// Renders the full-window interaction shield for this popover.
  pub fn render_backdrop(&self, viewport_size: Size<Pixels>) -> Anchored {
    let on_dismiss = self.on_dismiss.clone();
    anchored()
      .anchor(Corner::TopLeft)
      .position(point(px(0.0), px(0.0)))
      .position_mode(AnchoredPositionMode::Window)
      .child(
        div()
          .w(viewport_size.width)
          .h(viewport_size.height)
          .occlude()
          .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
          .on_any_mouse_down(move |_, window, cx| {
            if let Some(on_dismiss) = &on_dismiss {
              on_dismiss(window, cx);
            }
            cx.stop_propagation();
          }),
      )
  }

  /// Defers the modal backdrop below the rendered popup surface.
  pub fn deferred_backdrop(&self, viewport_size: Size<Pixels>) -> impl IntoElement {
    deferred(self.render_backdrop(viewport_size)).with_priority(0)
  }

  /// Defers the popup surface above its modal backdrop.
  pub fn deferred_content(self) -> impl IntoElement {
    deferred(self).with_priority(1)
  }

  /// Resolves the popup corner and its anchor-relative position.
  fn resolved_anchor(&self) -> (Corner, Point<Pixels>) {
    if let Some(offset) = self.offset {
      return (Corner::TopLeft, offset);
    }

    match self.placement {
      PopoverPlacement::Below => (Corner::TopLeft, point(px(0.0), self.anchor_size.height)),
      PopoverPlacement::Above => (Corner::BottomLeft, point(px(0.0), px(0.0))),
    }
  }
}

impl RenderOnce for Popover {
  /// Renders the popup surface as a deferred window-bounded overlay.
  ///
  /// # Parameters
  ///
  /// * `_window` supplies the current GPUI window.
  /// * `_cx` supplies the current GPUI application context.
  ///
  /// # Returns
  ///
  /// An anchored, scrollable popup surface containing the caller's content.
  fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
    let uses_custom_offset = self.offset.is_some();
    let (anchor_corner, offset) = self.resolved_anchor();
    log::debug!(
      "Popover {:?}: placement={:?}, custom_offset={}, anchor_position={:?}, anchor_size={:?}, anchor_corner={:?}, offset={:?}",
      self.id,
      self.placement,
      uses_custom_offset,
      self.anchor_position,
      self.anchor_size,
      anchor_corner,
      offset,
    );
    let mut surface = div()
      .id(self.id)
      .overflow_y_scroll()
      .track_scroll(&self.scroll_handle)
      .rounded_sm()
      .border_color(self.style.border_color.unwrap_or(self.theme.border.primary))
      .bg(self.style.background.unwrap_or(self.theme.background.secondary))
      .occlude()
      .child(self.content);

    if let Some(width) = self.style.width {
      surface = surface.w(width);
    }
    if let Some(height) = self.style.height {
      surface = surface.h(height);
    }
    if let Some(max_width) = self.style.max_width {
      surface = surface.max_w(max_width);
    }
    if let Some(max_height) = self.style.max_height {
      surface = surface.max_h(max_height);
    }
    if let Some(border_width) = self.style.border_width {
      surface = surface.border(border_width);
    } else {
      surface = surface.border_1();
    }

    let popover = anchored().anchor(anchor_corner).offset(offset);
    let popover = if let Some(anchor_position) = self.anchor_position {
      popover
        .position(anchor_position)
        .position_mode(AnchoredPositionMode::Window)
    } else {
      popover.position_mode(AnchoredPositionMode::Local)
    };

    // Built-in placements may switch to the opposite anchor when they would
    // overflow. A caller-supplied offset expresses its own geometry, so only
    // snap it into the viewport instead of invalidating that geometry.
    let popover = if uses_custom_offset {
      popover.snap_to_window()
    } else {
      popover
    };

    popover.child(surface)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn above_placement_should_attach_popup_bottom_to_anchor_top() {
    let popover = Popover::new("test-popover", div()).placement(PopoverPlacement::Above);

    assert_eq!(popover.resolved_anchor(), (Corner::BottomLeft, point(px(0.0), px(0.0))));
  }

  #[test]
  fn custom_offset_should_override_built_in_placement() {
    let offset = point(px(12.0), px(24.0));
    let popover = Popover::new("test-popover", div())
      .placement(PopoverPlacement::Above)
      .offset(offset);

    assert_eq!(popover.resolved_anchor(), (Corner::TopLeft, offset));
  }
}
