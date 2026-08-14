//! Read-only progress indicators for long-running work.

use std::time::Duration;

use gpui::{
  Animation, AnimationExt, App, IntoElement, ParentElement, RenderOnce, SharedString, Window, div, ease_in_out,
  prelude::*, px, relative,
};

use crate::themes::{UIThemes, builtins};

const TRACK_HEIGHT: gpui::Pixels = px(6.0);

/// A read-only progress indicator composed of an optional label, a percentage value,
/// and a themed [`ProgressTrack`].
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
  /// Theme used by the value and track.
  theme: UIThemes,
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
      theme: builtins::dark(),
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

  /// Sets the semantic theme used for this progress indicator.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
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
  fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
    let finishing = self.finishing_from.is_some();
    let mut track = ProgressTrack::new(if finishing { 100.0 } else { self.value })
      .indeterminate(self.indeterminate)
      .animation_id(self.animation_id.clone());
    if let Some(value) = self.finishing_from {
      track = track.finishing_from(value);
    }

    div()
      .flex()
      .flex_col()
      .gap_1()
      .w_full()
      .child(
        div()
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
          .child(if self.indeterminate {
            div().flex_none().into_any_element()
          } else {
            ProgressValue::new(if finishing { 100.0 } else { self.value })
              .theme(self.theme)
              .into_any_element()
          }),
      )
      .child(track.theme(self.theme))
  }
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
  /// Theme providing the value's text color.
  theme: UIThemes,
}

impl ProgressValue {
  /// Creates a percentage value display.
  pub fn new(value: f32) -> Self {
    Self {
      value,
      theme: builtins::dark(),
    }
  }

  /// Sets the semantic theme used for this progress value.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Returns the clamped percentage displayed by this value.
  pub fn percentage(&self) -> u8 {
    self.value.clamp(0.0, 100.0).round() as u8
  }
}

impl RenderOnce for ProgressValue {
  fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
    div()
      .flex_none()
      .text_sm()
      .text_color(self.theme.text.secondary)
      .child(format!("{}%", self.percentage()))
  }
}

/// Horizontal track shown below a [`Progress`] heading.
///
/// The track has three visual modes: a fixed completion width, an indeterminate
/// sliding segment, and a two-phase completion transition that first lets the
/// sliding segment finish before filling the track.
#[derive(IntoElement)]
pub struct ProgressTrack {
  /// Raw completion percentage used by the determinate mode.
  value: f32,
  /// Theme providing the track and completed-segment colors.
  theme: UIThemes,
  /// Whether the track displays an unknown-progress sliding segment.
  indeterminate: bool,
  /// Optional starting percentage for the two-phase completion animation.
  finishing_from: Option<f32>,
  /// Stable animation identity shared by the sweep and fill phases.
  animation_id: SharedString,
}

impl ProgressTrack {
  /// Creates a progress track from a percentage value.
  pub fn new(value: f32) -> Self {
    Self {
      value,
      theme: builtins::dark(),
      indeterminate: false,
      finishing_from: None,
      animation_id: SharedString::from("progress-indeterminate"),
    }
  }

  /// Sets the semantic theme used for this progress track.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Switches the track to an animated indeterminate state.
  pub fn indeterminate(mut self, indeterminate: bool) -> Self {
    self.indeterminate = indeterminate;
    self
  }

  /// Animates the completed segment from the supplied value to its final width.
  pub fn finishing_from(mut self, value: f32) -> Self {
    self.finishing_from = Some(value.clamp(0.0, 100.0));
    self
  }

  /// Sets the identity used to preserve animation state across renders.
  pub fn animation_id(mut self, id: impl Into<SharedString>) -> Self {
    self.animation_id = id.into();
    self
  }

  /// Returns the clamped completion ratio used by the filled segment.
  pub fn completion_ratio(&self) -> f32 {
    (self.value / 100.0).clamp(0.0, 1.0)
  }
}

impl RenderOnce for ProgressTrack {
  fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
    log::debug!(
      "ProgressTrack render: value={:.2}, indeterminate={}, finishing_from={:?}, completion_ratio={:.4}",
      self.value,
      self.indeterminate,
      self.finishing_from,
      self.completion_ratio()
    );
    let segment = div().h_full().rounded_sm().bg(self.theme.text.primary);
    let segment = if self.indeterminate {
      // A fixed-width segment communicates activity without implying a percentage.
      segment
        .absolute()
        .left(relative(-0.3))
        .w(relative(0.3))
        .with_animation(
          self.animation_id.clone(),
          Animation::new(Duration::from_millis(1200)).repeat(),
          move |segment, delta| {
            let left = -0.3 + delta * 1.3;
            let right = left + 0.3;
            log::trace!(
              "ProgressTrack animation frame: delta={delta:.4}, left={left:.4}, right={right:.4}, visible_left={:.4}, visible_right={:.4}",
              left.max(0.0),
              right.min(1.0)
            );
            segment.left(relative(left))
          },
        )
        .into_any_element()
    } else if let Some(start) = self.finishing_from {
      let start = (start / 100.0).clamp(0.0, 1.0);
      segment
        .with_animations(
          // Reuse the identity from the active sweep so completion continues from
          // the current animation state instead of jumping back to its beginning.
          self.animation_id.clone(),
          vec![
            Animation::new(Duration::from_millis(1200)),
            Animation::new(Duration::from_millis(280)).with_easing(ease_in_out),
          ],
          move |segment, animation_ix, delta| {
            if animation_ix == 0 {
              // Phase one lets the sliding segment leave the track completely.
              let left = -0.3 + delta * 1.3;
              let right = left + 0.3;
              log::trace!("ProgressTrack finishing sweep: delta={delta:.4}, left={left:.4}, right={right:.4}");
              segment.absolute().left(relative(left)).w(relative(0.3))
            } else {
              // Phase two replaces the slider with a determinate fill.
              let width = start + (1.0 - start) * delta;
              log::trace!("ProgressTrack finishing fill: delta={delta:.4}, width={width:.4}");
              segment.absolute().left(relative(0.0)).w(relative(width))
            }
          },
        )
        .into_any_element()
    } else {
      segment.w(relative(self.completion_ratio())).into_any_element()
    };

    div()
      .relative()
      .h(TRACK_HEIGHT)
      .w_full()
      .overflow_hidden()
      .rounded_sm()
      .bg(self.theme.background.tertiary)
      .child(segment)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn progress_value_should_round_and_clamp_the_displayed_percentage() {
    assert_eq!(ProgressValue::new(99.6).percentage(), 100);
  }

  #[test]
  fn progress_track_should_clamp_the_completion_ratio() {
    assert_eq!(ProgressTrack::new(120.0).completion_ratio(), 1.0);
  }
}
