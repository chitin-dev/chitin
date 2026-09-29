//! Scrollbar geometry, expressed in the caller's pixel space.
//!
//! The math here is deliberately free of GPUI layout types beyond [`Pixels`],
//! so thumb sizing and drag mapping can be tested without a window.

use gpui::{Pixels, px};

/// Smallest fraction of the track a thumb keeps.
///
/// A long scrollback would otherwise shrink the thumb past the point where it
/// can be grabbed. The floor is applied to the mapping as well as the paint, so
/// the far end of the content stays reachable.
const MIN_THUMB_EXTENT: f32 = 0.05;

/// Geometry of the region a scrollbar moves a viewport through.
///
/// The offset follows GPUI's scroll convention: zero shows the start of the
/// content and it becomes more negative as the viewport moves toward the end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarMetrics {
  /// Total height of the scrollable content.
  pub content_height: Pixels,
  /// Height of the visible viewport.
  pub viewport_height: Pixels,
  /// Distance from the top of the content to the top of the viewport.
  pub offset: Pixels,
}

impl ScrollbarMetrics {
  /// Creates metrics for one viewport position.
  ///
  /// # Parameters
  ///
  /// * `content_height` is the total height of the scrollable content.
  /// * `viewport_height` is the height of the visible viewport.
  /// * `offset` is the distance from the top of the content to the top of the
  ///   viewport, which is negative once the viewport has moved down.
  ///
  /// # Returns
  ///
  /// Metrics describing the requested viewport position.
  pub const fn new(content_height: Pixels, viewport_height: Pixels, offset: Pixels) -> Self {
    Self {
      content_height,
      viewport_height,
      offset,
    }
  }

  /// Returns whether the content is taller than the viewport.
  pub fn is_scrollable(&self) -> bool {
    self.travel() > 0.0
  }

  /// Returns the fraction of the track the thumb occupies.
  pub fn thumb_extent_fraction(&self) -> f32 {
    let content = f32::from(self.content_height);
    if content <= 0.0 {
      return 1.0;
    }

    (f32::from(self.viewport_height) / content).clamp(MIN_THUMB_EXTENT, 1.0)
  }

  /// Returns the thumb's distance from the top of the track, as a fraction of
  /// the track that the thumb can travel over.
  pub fn thumb_top_fraction(&self) -> f32 {
    let travel = self.travel();
    if travel <= 0.0 {
      return 0.0;
    }

    let progress = (-f32::from(self.offset) / travel).clamp(0.0, 1.0);
    progress * self.thumb_reach()
  }

  /// Returns the viewport offset described by a thumb dragged to `top`.
  ///
  /// # Parameters
  ///
  /// * `top` is the thumb's distance from the top of the track, as a fraction
  ///   of the track. Values outside the reachable range land on its ends.
  ///
  /// # Returns
  ///
  /// The viewport offset in the caller's pixel space.
  pub fn offset_for_thumb_top(&self, top: f32) -> Pixels {
    let travel = self.travel();
    let reach = self.thumb_reach();
    if travel <= 0.0 || reach <= 0.0 {
      return Pixels::ZERO;
    }

    px(-(top / reach).clamp(0.0, 1.0) * travel)
  }

  /// Returns the viewport offset for a click at a point on the track.
  ///
  /// The click centres the thumb on itself, which is what a scrollbar track
  /// click does everywhere else.
  ///
  /// # Parameters
  ///
  /// * `fraction` is the clicked distance from the top of the track, as a
  ///   fraction of the track.
  ///
  /// # Returns
  ///
  /// The viewport offset in the caller's pixel space.
  pub fn offset_for_track_click(&self, fraction: f32) -> Pixels {
    let reach = self.thumb_reach();
    let top = (fraction - self.thumb_extent_fraction() / 2.0).clamp(0.0, reach);
    self.offset_for_thumb_top(top)
  }

  /// Returns the offset one viewport closer to the end of the content.
  pub fn offset_for_page_toward_end(&self) -> Pixels {
    self.offset_for_page(true)
  }

  /// Returns the offset one viewport closer to the start of the content.
  pub fn offset_for_page_toward_start(&self) -> Pixels {
    self.offset_for_page(false)
  }

  /// Returns the offset one viewport away from the current position.
  fn offset_for_page(&self, toward_end: bool) -> Pixels {
    let travel = self.travel();
    let step = if toward_end {
      f32::from(self.viewport_height)
    } else {
      -f32::from(self.viewport_height)
    };

    px(-(-f32::from(self.offset) + step).clamp(0.0, travel))
  }

  /// Returns the distance the viewport can travel through the content.
  fn travel(&self) -> f32 {
    (f32::from(self.content_height) - f32::from(self.viewport_height)).max(0.0)
  }

  /// Returns how far the thumb's top may travel, as a track fraction.
  fn thumb_reach(&self) -> f32 {
    1.0 - self.thumb_extent_fraction()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Builds metrics for content that is scrolled to a given pixel offset.
  fn metrics(content: f32, viewport: f32, offset: f32) -> ScrollbarMetrics {
    ScrollbarMetrics::new(px(content), px(viewport), px(offset))
  }

  #[test]
  fn content_that_fits_should_not_scroll() {
    assert!(!metrics(80.0, 200.0, 0.0).is_scrollable());
  }

  #[test]
  fn a_thumb_should_span_the_whole_track_when_nothing_scrolls() {
    let metrics = metrics(80.0, 200.0, 0.0);

    assert_eq!(metrics.thumb_extent_fraction(), 1.0);
    assert_eq!(metrics.thumb_top_fraction(), 0.0);
  }

  #[test]
  fn a_thumb_should_shrink_as_the_content_grows() {
    let some_history = metrics(124.0, 24.0, 0.0).thumb_extent_fraction();
    let more_history = metrics(100_024.0, 24.0, 0.0).thumb_extent_fraction();

    assert!((some_history - 24.0 / 124.0).abs() < 1e-6);
    assert!(
      more_history < some_history,
      "a longer history should give a smaller thumb"
    );
    assert_eq!(metrics(48.0, 24.0, 0.0).thumb_extent_fraction(), 0.5);
  }

  #[test]
  fn a_thumb_should_keep_a_grabbable_size_for_a_long_history() {
    assert_eq!(metrics(100_024.0, 24.0, 0.0).thumb_extent_fraction(), MIN_THUMB_EXTENT);
  }

  #[test]
  fn a_thumb_should_rest_at_the_start_of_the_track_at_the_first_offset() {
    let metrics = metrics(200.0, 100.0, 0.0);

    assert_eq!(metrics.thumb_top_fraction(), 0.0);
  }

  #[test]
  fn a_thumb_should_rest_at_the_end_of_the_track_at_the_last_offset() {
    let metrics = metrics(200.0, 100.0, -100.0);

    // Half the content is visible, so the thumb is half the track and sits at
    // the end of the half it can travel.
    assert_eq!(metrics.thumb_extent_fraction(), 0.5);
    assert_eq!(metrics.thumb_top_fraction(), 0.5);
  }

  #[test]
  fn an_offset_should_round_trip_through_the_thumb() {
    let metrics = metrics(400.0, 100.0, -90.0);
    let top = metrics.thumb_top_fraction();

    assert_eq!(metrics.offset_for_thumb_top(top), px(-90.0));
  }

  #[test]
  fn dragging_toward_the_end_should_ask_for_a_later_offset() {
    let metrics = metrics(400.0, 100.0, 0.0);

    let earlier = metrics.offset_for_thumb_top(0.25);
    let later = metrics.offset_for_thumb_top(0.75);

    assert!(later < earlier, "a lower thumb should ask for a more negative offset");
    assert!(earlier < Pixels::ZERO);
  }

  #[test]
  fn a_drag_past_either_end_should_land_on_it() {
    let metrics = metrics(400.0, 100.0, -150.0);

    assert_eq!(metrics.offset_for_thumb_top(-5.0), Pixels::ZERO);
    assert_eq!(metrics.offset_for_thumb_top(5.0), px(-300.0));
  }

  #[test]
  fn a_track_click_should_centre_the_thumb_on_the_click() {
    let metrics = metrics(400.0, 100.0, 0.0);

    // A click at the midpoint of the track should land the viewport halfway
    // through the content, with the thumb centred under the pointer.
    let offset = metrics.offset_for_track_click(0.5);

    assert_eq!(offset, px(-150.0));
    // Halfway through the content means the thumb's top is at half of its reach.
    let centred = ScrollbarMetrics::new(metrics.content_height, metrics.viewport_height, offset);
    assert_eq!(centred.thumb_top_fraction(), 0.375);
  }

  #[test]
  fn a_track_click_should_not_escape_the_content() {
    let metrics = metrics(400.0, 100.0, 0.0);

    assert_eq!(metrics.offset_for_track_click(0.0), Pixels::ZERO);
    assert_eq!(metrics.offset_for_track_click(1.0), px(-300.0));
  }

  #[test]
  fn a_content_that_fits_should_ignore_every_motion() {
    let metrics = metrics(80.0, 200.0, 0.0);

    assert_eq!(metrics.offset_for_thumb_top(0.5), Pixels::ZERO);
    assert_eq!(metrics.offset_for_track_click(0.5), Pixels::ZERO);
  }
}
