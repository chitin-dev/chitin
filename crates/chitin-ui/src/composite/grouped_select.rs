// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Chitin Contributors

//! Grouped single-selection controls.

use gpui::{Entity, IntoElement, ParentElement, RenderOnce, SharedString, Styled, div, px};

use crate::{
  primitive::input::select::{Select, SelectContent, SelectInputState, SelectInputStyle, SelectInputVariant},
  themes::{UIThemes, builtins},
};

/// One independently selectable group rendered by [`GroupedSelect`].
#[derive(Clone)]
pub struct GroupedSelectGroup {
  /// Heading displayed above this group's selector.
  label: SharedString,
  /// Single-selection state owned by the caller.
  state: Entity<SelectInputState>,
  /// Select popup content and its selectable options.
  content: SelectContent,
}

impl GroupedSelectGroup {
  /// Creates a group with its label, state, and popup content.
  pub fn new(label: impl Into<SharedString>, state: Entity<SelectInputState>, content: SelectContent) -> Self {
    Self {
      label: label.into(),
      state,
      content,
    }
  }
}

/// A vertical composition of independently single-selectable groups.
///
/// Each group receives its own [`SelectInputState`]. Selecting an option in
/// one group therefore never clears the selection in another group, while
/// every group retains the keyboard, focus, and accessibility behavior of the
/// reusable select primitive.
#[derive(IntoElement)]
pub struct GroupedSelect {
  /// Independently selectable groups in display order.
  groups: Vec<GroupedSelectGroup>,
  /// Semantic colors shared by all nested selectors.
  theme: UIThemes,
  /// Width applied to each nested selector.
  width: Option<gpui::Pixels>,
}

impl GroupedSelect {
  /// Creates an empty grouped selector.
  pub fn new() -> Self {
    Self {
      groups: Vec::new(),
      theme: builtins::dark(),
      width: None,
    }
  }

  /// Adds one independently single-selectable group.
  pub fn group(mut self, group: GroupedSelectGroup) -> Self {
    self.groups.push(group);
    self
  }

  /// Sets the theme used by labels and nested selectors.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Sets the width of every nested selector.
  pub fn width(mut self, width: gpui::Pixels) -> Self {
    self.width = Some(width);
    self
  }
}

impl Default for GroupedSelect {
  fn default() -> Self {
    Self::new()
  }
}

impl RenderOnce for GroupedSelect {
  fn render(self, _: &mut gpui::Window, _: &mut gpui::App) -> impl IntoElement {
    let theme = self.theme;
    // The explicit width describes the whole composite, including its
    // horizontal padding. Reserve that space before sizing each child select.
    let width = self.width.map(|width| width - px(16.0));
    self.groups.into_iter().fold(
      // Keep the grouped controls visually separated from the surrounding
      // popover surface while leaving each Select responsible for its own
      // trigger and popup geometry.
      div().flex().flex_col().gap_2().p_2(),
      move |container, group| {
        let mut select = Select::new(group.state)
          .theme(theme)
          .variant(SelectInputVariant::Secondary)
          .content(group.content);
        if let Some(width) = width {
          select = select.style(SelectInputStyle::new().width(width));
        }
        container.child(
          div()
            .flex()
            .flex_col()
            .gap_1()
            .child(div().text_xs().text_color(theme.text.secondary).child(group.label))
            .child(select),
        )
      },
    )
  }
}
