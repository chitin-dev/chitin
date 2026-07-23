//! Reusable sidebar building blocks.
//!
//! A sidebar is the vertical panel next to an activity bar or primary content
//! area. This module provides generic layout pieces instead of domain-specific
//! panels: applications decide whether a sidebar shows files, search results,
//! agent sessions, job queues, or settings.

use std::rc::Rc;

use gpui::{
  AnyElement, App, CursorStyle, Div, InteractiveElement, IntoElement, MouseButton, ParentElement,
  Pixels, SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::*, px,
};

use crate::themes::{UIThemes, builtins};

/// Callback invoked when a sidebar resize gesture starts.
type SidebarResizeStartHandler = dyn Fn(Pixels, &mut Window, &mut App);

/// Default width of the sidebar panel in pixels.
///
/// This value provides a comfortable width for file trees and navigation panels,
/// balancing content visibility with available screen space. Applications can
/// override this value when creating their sidebar instance.
pub const DEFAULT_SIDEBAR_WIDTH: Pixels = px(260.0);
/// Default minimum width for a resizable sidebar.
pub const DEFAULT_SIDEBAR_MIN_WIDTH: Pixels = px(180.0);
/// Default maximum width for a resizable sidebar.
pub const DEFAULT_SIDEBAR_MAX_WIDTH: Pixels = px(480.0);
/// Default width of the sidebar resize handle.
pub const DEFAULT_SIDEBAR_RESIZE_HANDLE_WIDTH: Pixels = px(4.0);
/// Default height of the header/title bar in pixels.
///
/// This value provides sufficient height for labels, icons, and interactive
/// controls while maintaining a compact UI. Commonly used for workspace headers,
/// panel titles, and toolbar sections.
pub const DEFAULT_HEADER_HEIGHT: Pixels = px(30.0);

/// Drag state for a sidebar resize interaction.
#[derive(Clone, Copy, Debug)]
struct SidebarResizeDrag {
  /// Cursor x-position when the resize started.
  start_x: Pixels,
  /// Sidebar width when the resize started.
  start_width: Pixels,
}

/// Reusable state for a resizable sidebar.
///
/// This type is UI-generic and owns only geometry state: current width, resize
/// bounds, and active drag metadata. Applications keep this state in their own
/// app model, forward pointer positions to it, and pass [`Self::width`] into
/// [`Sidebar::width`].
///
/// # Example
///
/// ```no_run
/// use chitin_ui::components::sidebar::SidebarResizeState;
/// use gpui::px;
///
/// let mut resize = SidebarResizeState::default();
/// resize.start_resize(px(100.0));
/// resize.drag_resize(px(140.0));
/// assert_eq!(resize.width(), px(300.0));
/// resize.stop_resize();
/// ```
#[derive(Clone, Debug)]
pub struct SidebarResizeState {
  /// Current sidebar width.
  width: Pixels,
  /// Minimum allowed sidebar width.
  min_width: Pixels,
  /// Maximum allowed sidebar width.
  max_width: Pixels,
  /// Active resize drag metadata.
  resize_drag: Option<SidebarResizeDrag>,
}

impl SidebarResizeState {
  /// Creates sidebar resize state with default width bounds.
  pub fn new() -> Self {
    Self {
      width: DEFAULT_SIDEBAR_WIDTH,
      min_width: DEFAULT_SIDEBAR_MIN_WIDTH,
      max_width: DEFAULT_SIDEBAR_MAX_WIDTH,
      resize_drag: None,
    }
  }

  /// Sets the initial sidebar width.
  pub fn with_width(mut self, width: Pixels) -> Self {
    self.resize_width(width);
    self
  }

  /// Sets the minimum sidebar width.
  pub fn with_min_width(mut self, min_width: Pixels) -> Self {
    self.min_width = min_width;
    self.resize_width(self.width);
    self
  }

  /// Sets the maximum sidebar width.
  pub fn with_max_width(mut self, max_width: Pixels) -> Self {
    self.max_width = max_width;
    self.resize_width(self.width);
    self
  }

  /// Returns the current sidebar width.
  pub fn width(&self) -> Pixels {
    self.width
  }

  /// Returns whether the sidebar is currently being resized.
  pub fn is_resizing(&self) -> bool {
    self.resize_drag.is_some()
  }

  /// Starts a sidebar resize drag at the current cursor x-position.
  pub fn start_resize(&mut self, start_x: Pixels) {
    self.resize_drag = Some(SidebarResizeDrag {
      start_x,
      start_width: self.width,
    });
  }

  /// Updates sidebar width from the current resize cursor x-position.
  pub fn drag_resize(&mut self, current_x: Pixels) -> bool {
    let Some(resize_drag) = self.resize_drag else {
      return false;
    };

    self.resize_width(px(
      f32::from(resize_drag.start_width) + f32::from(current_x) - f32::from(resize_drag.start_x),
    ));
    //    |------------|----------|
    // 0            current_x  start_x
    // so the current_width = start_width + (current_x - start_x)
    true
  }

  /// Stops the current sidebar resize drag.
  pub fn stop_resize(&mut self) -> bool {
    self.resize_drag.take().is_some()
    // take will remove the dragging state from self.resize_drag
  }

  /// Resizes the sidebar while respecting configured width bounds.
  pub fn resize_width(&mut self, width: Pixels) {
    self.width = self.clamp_width(width);
  }

  /// Clamps a width to this resize state's bounds. Clamping means the width will
  /// not be less than min_width and will not be greater than max_width
  fn clamp_width(&self, width: Pixels) -> Pixels {
    px(f32::from(width).clamp(f32::from(self.min_width), f32::from(self.max_width)))
  }
}

impl Default for SidebarResizeState {
  fn default() -> Self {
    Self::new()
  }
}

/// Configuration for rendering a sidebar resize handle.
///
/// `Sidebar` owns the generic handle visuals and pointer cursor. The caller
/// provides state updates through `on_resize_start` so application state remains
/// outside `chitin-ui`.
#[derive(Clone)]
pub struct SidebarResizeConfig {
  /// Width of the resize handle.
  handle_width: Pixels,
  /// Callback invoked with cursor x-position when resizing starts.
  on_resize_start: Rc<SidebarResizeStartHandler>,
}

impl SidebarResizeConfig {
  /// Creates a resize configuration with the default handle width.
  pub fn new(on_resize_start: impl Fn(Pixels, &mut Window, &mut App) + 'static) -> Self {
    Self {
      handle_width: DEFAULT_SIDEBAR_RESIZE_HANDLE_WIDTH,
      on_resize_start: Rc::new(on_resize_start),
    }
  }

  /// Sets the resize handle width.
  pub fn handle_width(mut self, handle_width: Pixels) -> Self {
    self.handle_width = handle_width;
    self
  }
}

/// Header region for a sidebar panel.
///
/// A sidebar header is the compact top strip of a side panel. It can hold a
/// title, toolbar buttons, search controls, or any other GPUI elements supplied
/// by the caller.
pub struct SidebarHeader {
  /// The underlying container element for the header.
  ///
  /// Provides access to layout, styling, and event handling capabilities.
  /// Applications can chain GPUI methods on this base element to customize
  /// the header's appearance and behavior.
  base: Div,
  /// Visual styling applied to the header and its children.
  ///
  /// Controls text colors, background, borders, and hover states for all
  /// elements rendered within the header region.
  theme: UIThemes,
  /// Child elements rendered inside the header.
  ///
  /// These are the actual UI contents such as title labels, action buttons,
  /// search inputs, or custom controls. The caller provides these as GPUI
  /// `AnyElement`s when constructing the header.
  children: Vec<AnyElement>,
  /// Whether the header should be visually hidden.
  ///
  /// When `true`, the header is not rendered and takes up no space in the
  /// layout. This allows callers to conditionally show/hide the header
  /// without re-creating the component tree.
  hidden: bool,
}

impl SidebarHeader {
  /// Creates an empty sidebar header.
  pub fn new() -> Self {
    Self {
      base: div(),
      theme: builtins::dark(),
      children: Vec::new(),
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

    let theme = self.theme;

    self
      .base
      .flex()
      .items_center()
      .justify_between()
      .gap_2()
      .px_2()
      .h(DEFAULT_HEADER_HEIGHT)
      .bg(theme.background.primary)
      .children(self.children)
  }
}

/// Scrollable or fixed body region for sidebar content.
///
/// Use this for the main area of a sidebar: file trees, result lists, forms, or
/// panel-specific tools. Set [`SidebarBody::scrollable`] when content may exceed
/// the panel height.
pub struct SidebarBody {
  /// Optional stable identifier for element identification and testing.
  ///
  /// When provided, this ID is set as the element's `data-id` attribute,
  /// enabling reliable querying in integration tests and DOM inspection.
  id: Option<SharedString>,
  /// Theme applied to the body and its descendants.
  ///
  /// Controls background color, text colors, and border styles. Typically
  /// inherited from the parent [`Sidebar`], but can be overridden for
  /// custom styling.
  ///
  /// See [`UIThemes`] for available theme tokens.
  theme: UIThemes,
  /// Child elements rendered within the body region.
  ///
  /// These are laid out vertically with standard body padding. Common
  /// children include scrollable lists, tree views, or form controls.
  children: Vec<AnyElement>,
  /// Whether the body content should scroll independently.
  ///
  /// When `true`, the body acquires its own scroll container, allowing
  /// content to scroll while header and footer remain fixed. This is
  /// essential for file trees, search results, or any content that may
  /// exceed the sidebar's viewport height.
  ///
  /// Defaults to `false`.
  scrollable: bool,
  /// Whether the body is visually hidden.
  ///
  /// When `true`, the body is hidden but remains in the DOM (using
  /// `visibility: hidden` or `display: none` depending on context).
  /// Useful for conditional rendering without losing child state.
  ///
  /// Defaults to `false`.
  hidden: bool,
}

impl SidebarBody {
  /// Creates an empty sidebar body.
  pub fn new() -> Self {
    Self {
      id: None,
      theme: builtins::dark(),
      children: Vec::new(),
      scrollable: false,
      hidden: false,
    }
  }

  /// Adds a child element to the body.
  pub fn child(mut self, child: impl IntoElement) -> Self {
    self.children.push(child.into_any_element());
    self
  }

  /// Sets a stable GPUI element id.
  ///
  /// Scrollable bodies need an id because GPUI stores scroll state on stateful
  /// elements.
  pub fn id(mut self, id: impl Into<SharedString>) -> Self {
    self.id = Some(id.into());
    self
  }

  /// Overrides the visual theme used by this body.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Enables vertical scrolling for overflowing content.
  pub fn scrollable(mut self, scrollable: bool) -> Self {
    self.scrollable = scrollable;
    self
  }

  /// Hides this body from layout.
  pub fn hidden(mut self, hidden: bool) -> Self {
    self.hidden = hidden;
    self
  }
}

impl Default for SidebarBody {
  fn default() -> Self {
    Self::new()
  }
}

impl IntoElement for SidebarBody {
  type Element = AnyElement;

  fn into_element(self) -> Self::Element {
    if self.hidden {
      return div().hidden().into_any_element();
    }

    let body = div()
      .flex()
      .flex_col()
      .flex_1()
      .min_h_0()
      .bg(self.theme.background.primary);

    if self.scrollable {
      let id = self.id.unwrap_or_else(|| "sidebar-body-scroll".into());

      body
        .id(id)
        .overflow_y_scroll()
        .children(self.children)
        .into_any_element()
    } else {
      body.children(self.children).into_any_element()
    }
  }
}

/// Footer region for persistent sidebar actions or status.
pub struct SidebarFooter {
  /// The base container element.
  ///
  /// Provides the underlying GPUI [`Div`] that renders the footer.
  /// This field is typically used internally for layout and styling,
  /// but can be accessed for advanced customization.
  base: Div,
  /// Theme applied to the footer and its descendants.
  ///
  /// Controls background color, text colors, and border styles. Typically
  /// inherited from the parent [`Sidebar`], but can be overridden for
  /// custom styling.
  ///
  /// See [`UIThemes`] for available theme tokens.
  theme: UIThemes,
  /// Child elements rendered within the footer region.
  ///
  /// These are laid out horizontally or vertically depending on the footer's
  /// configuration. Common children include buttons, status labels, or
  /// small form controls like search inputs or filter dropdowns.
  children: Vec<AnyElement>,
  /// Whether the footer is visually hidden.
  ///
  /// When `true`, the footer is hidden but remains in the DOM (using
  /// `visibility: hidden` or `display: none` depending on context).
  /// Useful for conditional visibility of footer actions without
  /// rebuilding child components.
  ///
  /// Defaults to `false`.
  hidden: bool,
}

impl SidebarFooter {
  /// Creates an empty sidebar footer.
  pub fn new() -> Self {
    Self {
      base: div(),
      theme: builtins::dark(),
      children: Vec::new(),
      hidden: false,
    }
  }

  /// Adds a child element to the footer.
  pub fn child(mut self, child: impl IntoElement) -> Self {
    self.children.push(child.into_any_element());
    self
  }

  /// Overrides the visual theme used by this footer.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Hides this footer from layout.
  pub fn hidden(mut self, hidden: bool) -> Self {
    self.hidden = hidden;
    self
  }
}

impl Default for SidebarFooter {
  fn default() -> Self {
    Self::new()
  }
}

impl IntoElement for SidebarFooter {
  type Element = Div;

  fn into_element(self) -> Self::Element {
    if self.hidden {
      return div().hidden();
    }

    self
      .base
      .flex()
      .items_center()
      .gap_2()
      .px_2()
      .py_2()
      .border_t_1()
      .border_color(self.theme.border.primary)
      .bg(self.theme.background.secondary)
      .children(self.children)
  }
}

/// Logical subsection inside a sidebar body.
///
/// Sections are useful for grouping filters, tree roots, recent items, or
/// tool-specific controls without forcing the whole sidebar into cards.
pub struct SidebarSection {
  /// Whether the section should fill remaining body space.
  ///
  /// Enable this for children that need a bounded viewport, such as virtual
  /// lists or tree views.
  fill: bool,
  /// Theme applied to the section container.
  theme: UIThemes,
  /// Child elements rendered inside the section.
  children: Vec<AnyElement>,
  /// Whether the section should be hidden from layout.
  hidden: bool,
}

impl SidebarSection {
  /// Creates an empty sidebar section.
  pub fn new() -> Self {
    Self {
      fill: false,
      theme: builtins::dark(),
      children: Vec::new(),
      hidden: false,
    }
  }

  /// Adds a child element to the section.
  pub fn child(mut self, child: impl IntoElement) -> Self {
    self.children.push(child.into_any_element());
    self
  }

  /// Overrides the visual theme used by this section.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Makes this section fill the remaining height of the sidebar body.
  ///
  /// This is useful for virtualized content that needs a measurable viewport.
  pub fn fill(mut self, fill: bool) -> Self {
    self.fill = fill;
    self
  }

  /// Hides this section from layout.
  pub fn hidden(mut self, hidden: bool) -> Self {
    self.hidden = hidden;
    self
  }
}

impl Default for SidebarSection {
  fn default() -> Self {
    Self::new()
  }
}

impl IntoElement for SidebarSection {
  type Element = Div;

  fn into_element(self) -> Self::Element {
    if self.hidden {
      return div().hidden();
    }

    div()
      .flex()
      .flex_col()
      .w_full()
      .when(self.fill, |section| section.flex_1().min_h_0())
      .bg(self.theme.background.primary)
      .children(self.children)
  }
}

/// Compact text title for sidebar headers and section headers.
pub struct SidebarTitle {
  label: SharedString,
  theme: UIThemes,
}

impl SidebarTitle {
  /// Creates a sidebar title.
  pub fn new(label: impl Into<SharedString>) -> Self {
    Self {
      label: label.into(),
      theme: builtins::dark(),
    }
  }

  /// Overrides the visual theme used by this title.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }
}

impl IntoElement for SidebarTitle {
  type Element = Div;

  fn into_element(self) -> Self::Element {
    div()
      .min_w_0()
      .truncate()
      .text_xs()
      .font_weight(gpui::FontWeight::SEMIBOLD)
      .text_color(self.theme.text.primary)
      .child(self.label)
  }
}

/// Generic action slot for sidebar toolbars.
///
/// The component does not prescribe an icon system. Callers can pass a text
/// label, an SVG element, or any other GPUI element as the child.
pub struct SidebarAction {
  base: Div,
  theme: UIThemes,
  children: Vec<AnyElement>,
  hidden: bool,
}

impl SidebarAction {
  /// Creates an empty sidebar action.
  pub fn new() -> Self {
    Self {
      base: div(),
      theme: builtins::dark(),
      children: Vec::new(),
      hidden: false,
    }
  }

  /// Adds a child element to the action.
  pub fn child(mut self, child: impl IntoElement) -> Self {
    self.children.push(child.into_any_element());
    self
  }

  /// Overrides the visual theme used by this action.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Hides this action from layout.
  pub fn hidden(mut self, hidden: bool) -> Self {
    self.hidden = hidden;
    self
  }
}

impl Default for SidebarAction {
  fn default() -> Self {
    Self::new()
  }
}

impl IntoElement for SidebarAction {
  type Element = Div;

  fn into_element(self) -> Self::Element {
    if self.hidden {
      return div().hidden();
    }

    self
      .base
      .flex()
      .items_center()
      .justify_center()
      .size(gpui::px(24.0))
      .rounded_sm()
      .text_color(self.theme.text.secondary)
      .hover(|style| style.bg(self.theme.background.hover))
      .children(self.children)
  }
}

/// Outer shell for a sidebar panel.
///
/// `Sidebar` owns the standard sidebar frame: fixed width, full height,
/// right-side border, background, and vertical layout. Header/body/footer
/// pieces can be composed as children.
pub struct Sidebar {
  /// Base container element for the sidebar shell.
  base: Div,
  /// Theme applied to the sidebar frame and resize handle.
  theme: UIThemes,
  /// Current sidebar width.
  width: gpui::Pixels,
  /// Child elements rendered inside the sidebar.
  children: Vec<AnyElement>,
  /// Optional resize behavior for the sidebar shell.
  resize: Option<SidebarResizeConfig>,
  /// Whether the sidebar should be hidden from layout.
  hidden: bool,
}

impl Sidebar {
  /// Creates an empty sidebar shell.
  pub fn new() -> Self {
    Self {
      base: div(),
      theme: builtins::dark(),
      width: DEFAULT_SIDEBAR_WIDTH,
      children: Vec::new(),
      resize: None,
      hidden: false,
    }
  }

  /// Adds a child element to the sidebar.
  pub fn child(mut self, child: impl IntoElement) -> Self {
    self.children.push(child.into_any_element());
    self
  }

  /// Overrides the visual theme used by the sidebar frame.
  pub fn theme(mut self, theme: UIThemes) -> Self {
    self.theme = theme;
    self
  }

  /// Sets the sidebar width.
  pub fn width(mut self, width: gpui::Pixels) -> Self {
    self.width = width;
    self
  }

  /// Enables the generic right-edge resize handle.
  pub fn resizable(mut self, resize: SidebarResizeConfig) -> Self {
    self.resize = Some(resize);
    self
  }

  /// Hides the sidebar from layout.
  pub fn hidden(mut self, hidden: bool) -> Self {
    self.hidden = hidden;
    self
  }
}

impl Default for Sidebar {
  fn default() -> Self {
    Self::new()
  }
}

impl IntoElement for Sidebar {
  type Element = Div;

  fn into_element(self) -> Self::Element {
    if self.hidden {
      return div().hidden();
    }

    let theme = self.theme;
    let width = self.width;

    let sidebar = self
      .base
      .flex()
      .flex_col()
      .w(width)
      .h_full()
      .border_r_1()
      .border_color(theme.border.primary)
      .bg(theme.background.primary)
      .children(self.children);

    if let Some(resize) = self.resize {
      div()
        .relative()
        .flex()
        .h_full()
        .w(width)
        .child(sidebar)
        .child(render_sidebar_resize_handle(theme, resize))
    } else {
      sidebar
    }
  }
}

/// Renders the generic sidebar right-edge resize handle.
fn render_sidebar_resize_handle(theme: UIThemes, resize: SidebarResizeConfig) -> Div {
  let on_resize_start = resize.on_resize_start.clone();

  div()
    .absolute()
    .right_0()
    .top_0()
    .bottom_0()
    .w(resize.handle_width)
    .cursor(CursorStyle::ResizeLeftRight)
    .hover(move |style| style.bg(theme.border.focus))
    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
      on_resize_start(event.position.x, window, cx);
    })
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Verifies that sidebar resize width is clamped to the configured range.
  #[test]
  fn resize_width_should_clamp_to_sidebar_bounds() {
    let mut state = SidebarResizeState::default();

    state.resize_width(px(1.0));
    assert_eq!(state.width(), DEFAULT_SIDEBAR_MIN_WIDTH);

    state.resize_width(px(10_000.0));
    assert_eq!(state.width(), DEFAULT_SIDEBAR_MAX_WIDTH);
  }

  /// Verifies that drag resize applies cursor delta to the starting width.
  #[test]
  fn drag_resize_should_apply_delta_from_drag_start() {
    let mut state = SidebarResizeState::default();

    state.start_resize(px(100.0));
    assert!(state.drag_resize(px(140.0)));

    assert_eq!(state.width(), px(f32::from(DEFAULT_SIDEBAR_WIDTH) + 40.0));
  }

  /// Verifies that stopping resize clears active resize state.
  #[test]
  fn stop_resize_should_clear_active_resize_state() {
    let mut state = SidebarResizeState::default();

    state.start_resize(px(100.0));
    assert!(state.is_resizing());
    assert!(state.stop_resize());
    assert!(!state.is_resizing());
  }
}
