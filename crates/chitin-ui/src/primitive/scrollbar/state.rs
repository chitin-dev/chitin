//! Persistent interaction state for the scrollbar primitive.

use gpui::{Context, EventEmitter, MouseButton, Pixels};

use super::{ScrollbarEvent, ScrollbarMetrics};
use crate::primitive::resize::ResizeGesture;

/// Persistent interaction state for one scrollbar.
///
/// The bar owns its drag: callers subscribe to [`ScrollbarEvent`] instead of
/// attaching their own pointer handlers.
pub struct ScrollbarState {
  drag: Option<ResizeGesture<(), f32>>,
}

impl ScrollbarState {
  /// Creates a scrollbar that is not being dragged.
  pub fn new(_cx: &mut Context<Self>) -> Self {
    Self { drag: None }
  }

  /// Returns whether the user is currently dragging the thumb.
  pub const fn is_dragging(&self) -> bool {
    self.drag.is_some()
  }

  /// Starts a drag from a press on the thumb.
  ///
  /// A press that arrives while a drag is already underway is ignored, so the
  /// thumb cannot be handed to a second pointer mid-gesture.
  ///
  /// # Parameters
  ///
  /// * `pointer_y` is where the pointer pressed the thumb, in window pixels.
  /// * `metrics` describes the content the bar is scrolling.
  /// * `cx` refreshes observers of the drag state.
  pub fn begin_drag(&mut self, pointer_y: Pixels, metrics: ScrollbarMetrics, cx: &mut Context<Self>) {
    if self.is_dragging() {
      return;
    }

    // The grip is kept as a track fraction rather than a pixel delta so it
    // survives the content growing underneath the drag.
    self.drag = Some(ResizeGesture::new((), pointer_y, metrics.thumb_top_fraction()));
    cx.notify();
  }

  /// Continues an in-progress drag from the current pointer position.
  ///
  /// This is a no-op when no drag is underway, which is what lets one handler be
  /// attached to the whole bar without acting on idle pointer movement.
  ///
  /// # Parameters
  ///
  /// * `pointer_y` is the pointer position on the drag axis, in window pixels.
  /// * `pressed_button` is the button the move reports as held, if any.
  /// * `metrics` describes the content the bar is scrolling. The bar is laid out
  ///   over the viewport, so the viewport height is the distance the thumb
  ///   travels over.
  /// * `cx` publishes the requested viewport offset.
  pub fn drag(
    &mut self,
    pointer_y: Pixels,
    pressed_button: Option<MouseButton>,
    metrics: ScrollbarMetrics,
    cx: &mut Context<Self>,
  ) {
    // A release outside the bar never reaches its own handler: GPUI dispatches
    // pointer events by position and offers no pointer capture, so a gesture
    // would otherwise stay open and the next move across the bar would jump the
    // viewport to wherever that move implied. The button the move reports is
    // what closes it instead.
    if pressed_button != Some(MouseButton::Left) {
      self.end_drag(cx);
      return;
    }
    let Some(gesture) = self.drag.as_ref() else {
      return;
    };

    let track_height = f32::from(metrics.viewport_height).max(1.0);
    let thumb_top = gesture.start_value() + gesture.delta(pointer_y) / track_height;
    cx.emit(ScrollbarEvent::Scrolled {
      offset: metrics.offset_for_thumb_top(thumb_top),
    });
  }

  /// Ends an in-progress drag.
  pub fn end_drag(&mut self, cx: &mut Context<Self>) {
    if self.drag.take().is_some() {
      cx.notify();
    }
  }

  /// Moves the viewport by one screenful toward one end of the content.
  ///
  /// # Parameters
  ///
  /// * `metrics` describes the content the bar is scrolling.
  /// * `toward_end` selects the direction of travel.
  /// * `cx` publishes the requested viewport offset.
  pub fn scroll_page(&self, metrics: ScrollbarMetrics, toward_end: bool, cx: &mut Context<Self>) {
    let offset = if toward_end {
      metrics.offset_for_page_toward_end()
    } else {
      metrics.offset_for_page_toward_start()
    };
    cx.emit(ScrollbarEvent::Scrolled { offset });
  }
}

impl EventEmitter<ScrollbarEvent> for ScrollbarState {}
