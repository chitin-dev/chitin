//! Read-only progress indicators for long-running work.

use std::time::Duration;

use gpui::{
  Animation, AnimationExt, App, IntoElement, ParentElement, RenderOnce, SharedString, Window, div, prelude::*, px,
  relative,
};

use crate::themes::{UIThemes, builtins};

const TRACK_HEIGHT: gpui::Pixels = px(6.0);

/// A read-only progress indicator composed from a label, value, and track.
#[derive(IntoElement)]
pub struct Progress {
  value: f32,
  label: Option<ProgressLabel>,
  theme: UIThemes,
  indeterminate: bool,
}

impl Progress {
  /// Creates a progress indicator from a percentage value.
  pub fn new(value: f32) -> Self {
    Self {
      value,
      label: None,
      theme: builtins::dark(),
      indeterminate: false,
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
}

impl RenderOnce for Progress {
  fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
    let track = ProgressTrack::new(self.value).indeterminate(self.indeterminate);

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
            ProgressValue::new(self.value).theme(self.theme).into_any_element()
          }),
      )
      .child(track.theme(self.theme))
  }
}

/// Text that explains the work represented by a [`Progress`] indicator.
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
  value: f32,
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

/// Horizontal track and completed segment shown below a [`Progress`] heading.
#[derive(IntoElement)]
pub struct ProgressTrack {
  value: f32,
  theme: UIThemes,
  indeterminate: bool,
}

impl ProgressTrack {
  /// Creates a progress track from a percentage value.
  pub fn new(value: f32) -> Self {
    Self {
      value,
      theme: builtins::dark(),
      indeterminate: false,
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

  /// Returns the clamped completion ratio used by the filled segment.
  pub fn completion_ratio(&self) -> f32 {
    (self.value / 100.0).clamp(0.0, 1.0)
  }
}

impl RenderOnce for ProgressTrack {
  fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
    log::debug!(
      "ProgressTrack render: value={:.2}, indeterminate={}, completion_ratio={:.4}",
      self.value,
      self.indeterminate,
      self.completion_ratio()
    );
    let segment = div().h_full().rounded_sm().bg(self.theme.text.primary);
    let segment = if self.indeterminate {
      segment
        .absolute()
        .left(relative(-0.3))
        .w(relative(0.3))
        .with_animation(
          "progress-indeterminate",
          Animation::new(Duration::from_millis(1200)).repeat(),
          |segment, delta| {
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
