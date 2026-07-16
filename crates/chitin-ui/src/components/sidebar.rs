use gpui::{AnyElement, Div, IntoElement, div, prelude::*};

use crate::themes::{UIThemes, builtins};

/// Header region for a sidebar panel.
///
/// A sidebar header is the compact top strip of a side panel. It can hold a
/// title, toolbar buttons, search controls, or any other GPUI elements supplied
/// by the caller. The component is intentionally generic so it can be reused for
/// file trees, search panes, agent panels, job queues, and future extension
/// panels.
pub struct SidebarHeader {
  base: Div,
  theme: UIThemes,
  children: Vec<AnyElement>,
  selected: bool,
  collapsed: bool,
  hidden: bool,
}

impl SidebarHeader {
  /// Creates an empty sidebar header.
  pub fn new() -> Self {
    Self {
      base: div(),
      theme: builtins::dark(),
      children: Vec::new(),
      selected: false,
      collapsed: false,
      hidden: false,
    }
  }

  /// Adds a child element to the header.
  pub fn child(mut self, child: impl IntoElement) -> Self {
    self.children.push(child.into_any_element());
    self
  }

  /// Overrides the visual theme used by this header.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Marks this header as selected.
  ///
  /// This is useful for sidebars that can switch between multiple panels while
  /// keeping the header visually connected to the active panel.
  pub fn selected(mut self, selected: bool) -> Self {
    self.selected = selected;
    self
  }

  /// Marks this header as collapsed.
  ///
  /// Collapsed headers remain visible but use tighter spacing. The owning
  /// application is responsible for hiding or showing the panel body.
  pub fn collapsed(mut self, collapsed: bool) -> Self {
    self.collapsed = collapsed;
    self
  }

  /// Hides this header from layout.
  pub fn hidden(mut self, hidden: bool) -> Self {
    self.hidden = hidden;
    self
  }
}

impl Default for SidebarHeader {
  fn default() -> Self {
    Self::new()
  }
}

impl IntoElement for SidebarHeader {
  type Element = Div;

  fn into_element(self) -> Self::Element {
    if self.hidden {
      return div().hidden();
    }

    let background = if self.selected {
      self.theme.background.selection
    } else {
      self.theme.background.secondary
    };

    let padding = if self.collapsed { 1 } else { 2 };

    self
      .base
      .flex()
      .items_center()
      .justify_between()
      .gap_2()
      .px_2()
      .py(px_units(padding))
      .border_b_1()
      .border_color(self.theme.border.primary)
      .bg(background)
      .children(self.children)
  }
}

fn px_units(units: u32) -> gpui::Pixels {
  gpui::px(units as f32 * 4.0)
}
