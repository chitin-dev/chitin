//! Application-owned option data for GPUI Kit selectors.

use gpui::{AnyElement, App, IntoElement, ParentElement, SharedString, Window, div, prelude::*};
use gpui_kit::component::{Icon, select::SelectItem};

/// A stable option value and label with an optional leading application icon.
///
/// GPUI Kit owns selection, keyboard navigation, focus, and popup behavior.
#[derive(Clone)]
pub struct IconSelectItem {
  value: SharedString,
  label: SharedString,
  icon: Option<SharedString>,
}

impl IconSelectItem {
  /// Creates an option identified by `value` and displayed as `label`.
  pub fn new(value: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
    Self {
      value: value.into(),
      label: label.into(),
      icon: None,
    }
  }

  /// Adds a leading icon from the application's asset source.
  pub fn icon(mut self, path: impl Into<SharedString>) -> Self {
    self.icon = Some(path.into());
    self
  }

  /// Builds the same icon and label for both the selected value and menu row.
  fn content(&self) -> impl IntoElement {
    div()
      .flex()
      .items_center()
      .gap_2()
      .when_some(self.icon.clone(), |row, path| row.child(Icon::default().path(path)))
      .child(self.label.clone())
  }
}

impl SelectItem for IconSelectItem {
  type Value = SharedString;

  fn title(&self) -> SharedString {
    self.label.clone()
  }

  fn value(&self) -> &Self::Value {
    &self.value
  }

  fn display_title(&self) -> Option<AnyElement> {
    Some(self.content().into_any_element())
  }

  fn render(&self, _: &mut Window, _: &mut App) -> impl IntoElement {
    self.content()
  }
}
