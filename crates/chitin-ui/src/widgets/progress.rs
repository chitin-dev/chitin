//! Read-only progress indicators for long-running work.
//!
//! The bar itself is gpui-kit's [`Progress`](gpui_kit::component::progress::Progress):
//! it owns the determinate fill, the indeterminate slide, the transition a value
//! change plays, the theme colour and the accessibility label. Two things gpui-kit
//! has no element for stay here — the heading, which pairs the bar with a label and
//! a percentage, and the sweep that carries a sliding segment out of the track
//! before completion fills it.

use std::time::Duration;

use gpui::{
  Animation, AnimationExt, AnyElement, App, IntoElement, IsZero as _, ParentElement, Pixels, RenderOnce, SharedString,
  Window, div, ease_in_out, prelude::*, px, relative,
};
use gpui_kit::component::{ActiveTheme as _, progress::Progress as ProgressBar};

/// Height of the completion sweep's track, matching gpui-kit's default bar.
const TRACK_HEIGHT: Pixels = px(8.0);
/// Radius of the completion sweep's track and segment.
const TRACK_RADIUS: Pixels = px(4.0);

/// A read-only progress indicator composed of an optional label, a percentage value,
/// and a tracked bar.
///
/// Use [`Progress::indeterminate`] when the amount of work is unknown. A completed
/// indeterminate operation can use [`Progress::finishing_from`] to sweep the current
/// indicator out before filling the track to 100 percent.
#[derive(IntoElement)]
pub struct Progress {
  /// Completion percentage in the inclusive range from 0 to 100.
  value: f32,
  /// Optional description displayed above the track.
  label: Option<ProgressLabel>,
  /// Whether the track should use an animated unknown-progress indicator.
  indeterminate: bool,
  /// Starting percentage for the completion transition, when active.
  finishing_from: Option<f32>,
  /// Stable identity used by GPUI to retain animation state between renders.
  animation_id: SharedString,
}

impl Progress {
  /// Creates a progress indicator from a percentage value.
  pub fn new(value: f32) -> Self {
    Self {
      value,
      label: None,
      indeterminate: false,
      finishing_from: None,
      animation_id: SharedString::from("progress-indeterminate"),
    }
  }

  /// Sets the label displayed above the track.
  pub fn label(mut self, label: ProgressLabel) -> Self {
    self.label = Some(label);
    self
  }

  /// Shows an animated track without claiming a known completion percentage.
  pub fn indeterminate(mut self) -> Self {
    self.indeterminate = true;
    self
  }

  /// Animates the track from the supplied value to completion.
  pub fn finishing_from(mut self, value: f32) -> Self {
    self.finishing_from = Some(value.clamp(0.0, 100.0));
    self
  }

  /// Sets the identity used to preserve animation state across renders.
  pub fn animation_id(mut self, id: impl Into<SharedString>) -> Self {
    self.animation_id = id.into();
    self
  }
}

impl RenderOnce for Progress {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let finishing = self.finishing_from.is_some();
    let value = if finishing { 100.0 } else { self.value };
    let indeterminate = self.indeterminate && !finishing;

    let heading = div()
      .flex()
      .items_center()
      .justify_between()
      .gap_3()
      .min_w_0()
      .child(
        self
          .label
          .map_or_else(|| div().into_any_element(), |label| label.into_any_element()),
      )
      .child(if indeterminate {
        div().flex_none().into_any_element()
      } else {
        ProgressValue::new(value).into_any_element()
      });

    div().flex().flex_col().gap_1().w_full().child(heading).child(track(
      self.finishing_from,
      value,
      indeterminate,
      &self.animation_id,
      cx,
    ))
  }
}

/// Renders the bar for one progress state.
///
/// Everything except the completion sweep is gpui-kit's bar, so the transition a
/// value change plays and the shape of the sliding segment are its design. The
/// sweep is drawn here because gpui-kit cannot express it: an indeterminate bar is
/// a segment moving across the track, and completion has to carry that segment off
/// the end before the fill starts, rather than widening a width that was never
/// standing still.
fn track(
  finishing_from: Option<f32>,
  value: f32,
  indeterminate: bool,
  animation_id: &SharedString,
  cx: &App,
) -> AnyElement {
  let Some(start) = finishing_from else {
    return ProgressBar::new(animation_id.clone())
      .value(value)
      .loading(indeterminate)
      .into_any_element();
  };

  let color = cx.theme().progress_bar;
  let radius = if cx.theme().radius.is_zero() {
    px(0.0)
  } else {
    TRACK_RADIUS
  };
  let start = (start / 100.0).clamp(0.0, 1.0);
  let segment = div().h_full().rounded(radius).bg(color);

  div()
    .relative()
    .h(TRACK_HEIGHT)
    .w_full()
    .overflow_hidden()
    .rounded(radius)
    .bg(color.opacity(0.2))
    .child(
      segment
        .with_animations(
          animation_id.clone(),
          vec![
            Animation::new(Duration::from_millis(1200)),
            Animation::new(Duration::from_millis(280)).with_easing(ease_in_out),
          ],
          move |segment, animation_ix, delta| {
            if animation_ix == 0 {
              // Phase one lets the sliding segment leave the track completely.
              let left = -0.3 + delta * 1.3;
              segment.absolute().left(relative(left)).w(relative(0.3))
            } else {
              // Phase two replaces the slider with a determinate fill.
              let width = start + (1.0 - start) * delta;
              segment.absolute().left(relative(0.0)).w(relative(width))
            }
          },
        )
        .into_any_element(),
    )
    .into_any_element()
}

/// Text label describing the work represented by a [`Progress`] indicator.
#[derive(IntoElement)]
pub struct ProgressLabel(SharedString);

impl ProgressLabel {
  /// Creates a progress label.
  pub fn new(label: impl Into<SharedString>) -> Self {
    Self(label.into())
  }
}

impl RenderOnce for ProgressLabel {
  fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
    div().min_w_0().truncate().text_sm().child(self.0)
  }
}

/// Percentage text shown at the upper-right of a [`Progress`] indicator.
#[derive(IntoElement)]
pub struct ProgressValue {
  /// Raw percentage value, clamped only when it is displayed.
  value: f32,
}

impl ProgressValue {
  /// Creates a percentage value display.
  pub const fn new(value: f32) -> Self {
    Self { value }
  }

  /// Returns the clamped percentage displayed by this value.
  pub fn percentage(&self) -> u8 {
    self.value.clamp(0.0, 100.0).round() as u8
  }
}

impl RenderOnce for ProgressValue {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    div()
      .flex_none()
      .text_sm()
      .text_color(cx.theme().muted_foreground)
      .child(format!("{}%", self.percentage()))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn progress_value_should_round_and_clamp_the_displayed_percentage() {
    assert_eq!(ProgressValue::new(99.6).percentage(), 100);
  }
}
