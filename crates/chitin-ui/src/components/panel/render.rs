//! GPUI rendering for panel containers.

use std::rc::Rc;

use gpui::{
  AnyElement, App, Context, CursorStyle, Div, InteractiveElement, MouseButton, ParentElement,
  Pixels, SharedString, Styled, Window, div, prelude::*, px, relative,
};

use crate::themes::UIThemes;

use super::{
  drag::{PanelTabDrag, PanelTabDragConfig, PanelTabDropTarget, panel_tab_insertion_index},
  layout::PanelResizeConfig,
  model::{
    PanelId, PanelLeaf, PanelNode, PanelSplit, PanelSplitAxis, PanelSplitBranch, PanelSplitPath,
    PanelTab, PanelTabId, PanelTree,
  },
};

/// Default width of a split resize handle.
pub const DEFAULT_PANEL_SPLIT_HANDLE_SIZE: Pixels = px(2.0);
/// Default height of tab strip
pub const DEFAULT_PANEL_TAB_STRIP_HEIGHT: Pixels = px(34.0);
/// Default square size reserved for a panel tab close button.
pub const DEFAULT_PANEL_TAB_CLOSE_BUTTON_SIZE: Pixels = px(18.0);
/// Default width reserved for trailing tab actions.
pub const DEFAULT_PANEL_TAB_TRAILING_ACTION_WIDTH: Pixels = DEFAULT_PANEL_TAB_STRIP_HEIGHT;

/// Callback invoked when a tab is activated.
pub type PanelTabActivateHandler = dyn Fn(PanelId, PanelTabId, &mut Window, &mut App);
/// Callback invoked when a tab close button is pressed.
pub type PanelTabCloseHandler = dyn Fn(PanelId, PanelTabId, &mut Window, &mut App);
/// Renderer for the icon shown inside one tab close button.
pub type PanelTabCloseIconRenderer = dyn Fn(UIThemes) -> AnyElement;
/// Renderer for caller-owned actions placed at the end of a panel tab strip.
pub type PanelTabStripActionsRenderer = dyn Fn(PanelId) -> AnyElement;

/// Optional chrome behavior for [`render_panel_container`].
#[derive(Clone, Default)]
pub struct PanelContainerConfig {
  /// Optional resize configuration used by split handles.
  resize: Option<PanelResizeConfig>,
  /// Current focused panel id, normally transferred by upper component
  /// [`DocumentPanelState`]
  focused_panel_id: Option<PanelId>,
  /// Optional callback invoked when a tab is activated.
  on_activate_tab: Option<Rc<PanelTabActivateHandler>>,
  /// Optional callback invoked when a tab close button is pressed.
  on_close_tab: Option<Rc<PanelTabCloseHandler>>,
  /// Optional renderer for the tab close icon.
  render_tab_close_icon: Option<Rc<PanelTabCloseIconRenderer>>,
  /// Optional renderer for caller-owned tab-strip actions.
  render_tab_strip_actions: Option<Rc<PanelTabStripActionsRenderer>>,
  /// Optional native GPUI tab drag-and-drop behavior.
  tab_drag: Option<PanelTabDragConfig>,
}

impl PanelContainerConfig {
  /// Creates an empty panel container configuration.
  ///
  /// # Parameters
  ///
  /// This function takes no parameters.
  ///
  /// # Returns
  ///
  /// A [`PanelContainerConfig`] with optional behaviors disabled.
  pub fn new() -> Self {
    Self::default()
  }

  /// Enables split resize handles.
  ///
  /// # Parameters
  ///
  /// `resize` configures resize handle size and resize-start callbacks.
  ///
  /// # Returns
  ///
  /// The updated [`PanelContainerConfig`] for builder chaining.
  pub fn resize(mut self, resize: PanelResizeConfig) -> Self {
    self.resize = Some(resize);
    self
  }

  /// Sets current focused panel id used to compare the panel container config
  /// with it, used to draw activated tab in focused panel differently.
  ///
  /// # Parameters
  ///
  /// `panel_id` configures current focused panel id
  ///
  /// # Returns
  ///
  /// The updated [`PanelContainerConfig`] for builder chaining.
  pub fn focused_panel_id(mut self, panel_id: PanelId) -> Self {
    self.focused_panel_id = Some(panel_id);
    self
  }

  /// Sets the tab activation callback.
  ///
  /// # Parameters
  ///
  /// `handler` is invoked when a rendered tab is pressed.
  ///
  /// # Returns
  ///
  /// The updated [`PanelContainerConfig`] for builder chaining.
  pub fn on_activate_tab(mut self, handler: Rc<PanelTabActivateHandler>) -> Self {
    self.on_activate_tab = Some(handler);
    self
  }

  /// Sets the tab close callback.
  ///
  /// # Parameters
  ///
  /// `handler` is invoked when a rendered tab close button is pressed.
  ///
  /// # Returns
  ///
  /// The updated [`PanelContainerConfig`] for builder chaining.
  pub fn on_close_tab(mut self, handler: Rc<PanelTabCloseHandler>) -> Self {
    self.on_close_tab = Some(handler);
    self
  }

  /// Sets the tab close icon renderer.
  ///
  /// # Parameters
  ///
  /// `renderer` produces the visual icon inside the reusable close button slot.
  ///
  /// # Returns
  ///
  /// The updated [`PanelContainerConfig`] for builder chaining.
  pub fn render_tab_close_icon(mut self, renderer: Rc<PanelTabCloseIconRenderer>) -> Self {
    self.render_tab_close_icon = Some(renderer);
    self
  }

  /// Sets the tab-strip action renderer.
  ///
  /// # Parameters
  ///
  /// `renderer` produces caller-owned controls at the right end of each panel
  /// tab strip.
  ///
  /// # Returns
  ///
  /// The updated [`PanelContainerConfig`] for builder chaining.
  pub fn render_tab_strip_actions(mut self, renderer: Rc<PanelTabStripActionsRenderer>) -> Self {
    self.render_tab_strip_actions = Some(renderer);
    self
  }

  /// Enables native GPUI drag-and-drop for panel tabs.
  ///
  /// # Parameters
  ///
  /// `tab_drag` supplies temporary state plus start, target, and drop callbacks.
  ///
  /// # Returns
  ///
  /// The updated [`PanelContainerConfig`] for builder chaining.
  pub fn tab_drag(mut self, tab_drag: PanelTabDragConfig) -> Self {
    self.tab_drag = Some(tab_drag);
    self
  }
}

/// Shared dependencies used while rendering one panel tree.
struct PanelRenderContext<'a, T> {
  /// Theme tokens used by panel chrome.
  theme: UIThemes,
  /// Optional chrome behavior used while rendering.
  config: &'a PanelContainerConfig,
  /// Renderer for active tab body content.
  render_body: &'a dyn Fn(&PanelTab<T>) -> AnyElement,
}

/// Lightweight GPUI view displayed under the pointer during a tab drag.
struct PanelTabDragPreview {
  /// Title copied from the stable drag payload.
  title: SharedString,
  /// Theme used to match existing panel chrome.
  theme: UIThemes,
}

impl Render for PanelTabDragPreview {
  /// Renders a compact tab-title preview without duplicating panel content.
  ///
  /// # Parameters
  ///
  /// `window` is unused because the preview has no window-specific state.
  ///
  /// `cx` is unused because the preview has no mutable entity state.
  ///
  /// # Returns
  ///
  /// A GPUI element styled from the current UI theme.
  fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
    div()
      .max_w(px(220.0))
      .px_3()
      .h(DEFAULT_PANEL_TAB_STRIP_HEIGHT)
      .flex()
      .items_center()
      .rounded_sm()
      .border_1()
      .border_color(self.theme.border.focus)
      .bg(self.theme.background.secondary)
      .text_color(self.theme.text.primary)
      .text_xs()
      .shadow_md()
      .child(div().min_w_0().truncate().child(self.title.clone()))
  }
}

/// Renders a binary panel tree.
///
/// The renderer owns neutral panel chrome: tab strips, active tab styling,
/// split panes, and resize handles. The caller owns concrete view rendering by
/// mapping active tabs to body elements.
///
/// # Parameters
///
/// `tree` is the panel tree to render.
///
/// `theme` supplies visual tokens for panel chrome.
///
/// `config` contains optional resize, tab activation, close, icon, and
/// tab-strip action behavior.
///
/// `render_body` renders the body for one active tab.
///
/// # Returns
///
/// A GPUI `Div` containing the rendered panel tree.
pub fn render_panel_container<T>(
  tree: &PanelTree<T>,
  theme: UIThemes,
  config: PanelContainerConfig,
  render_body: &dyn Fn(&PanelTab<T>) -> AnyElement,
) -> Div {
  let context = PanelRenderContext {
    theme,
    config: &config,
    render_body,
  };

  render_panel_node(&tree.root, &PanelSplitPath::root(), &context)
}

/// Renders one panel tree node.
///
/// # Parameters
///
/// `node` is the current panel node to render.
///
/// `path` is the split path to `node`.
///
/// `context` contains theme tokens, callbacks, and active tab body rendering.
///
/// # Returns
///
/// A GPUI `Div` for the rendered node.
fn render_panel_node<T>(
  node: &PanelNode<T>,
  path: &PanelSplitPath,
  context: &PanelRenderContext<'_, T>,
) -> Div {
  match node {
    PanelNode::Leaf(leaf) => render_panel_leaf(leaf, context),
    PanelNode::Split(split) => render_panel_split(split, path, context),
  }
}

/// Renders one split node and its handle.
///
/// # Parameters
///
/// `split` is the split node to render.
///
/// `path` identifies `split` for resize callbacks.
///
/// `context` contains theme tokens, callbacks, and active tab body rendering.
///
/// # Returns
///
/// A GPUI `Div` for the split node.
fn render_panel_split<T>(
  split: &PanelSplit<T>,
  path: &PanelSplitPath,
  context: &PanelRenderContext<'_, T>,
) -> Div {
  let first = render_panel_node(&split.first, &path.child(PanelSplitBranch::First), context)
    .flex_basis(relative(split.ratio))
    .flex_shrink()
    .min_w_0()
    .min_h_0();

  let second = render_panel_node(
    &split.second,
    &path.child(PanelSplitBranch::Second),
    context,
  )
  .flex_basis(relative(1.0 - split.ratio))
  .flex_shrink()
  .min_w_0()
  .min_h_0();

  div()
    .flex()
    .size_full()
    .min_w_0()
    .min_h_0()
    .when(split.axis == PanelSplitAxis::Vertical, |panel| {
      panel.flex_col()
    })
    .child(first)
    .child(render_panel_split_handle(
      split.axis,
      path,
      context.theme,
      context.config.resize.as_ref(),
    ))
    .child(second)
}

/// Renders one split resize handle.
///
/// # Parameters
///
/// `axis` controls handle orientation and cursor.
///
/// `path` identifies the split for resize callbacks.
///
/// `theme` supplies visual tokens.
///
/// `resize` optionally enables resize-start behavior.
///
/// # Returns
///
/// A GPUI `Div` for the resize handle.
fn render_panel_split_handle(
  axis: PanelSplitAxis,
  path: &PanelSplitPath,
  theme: UIThemes,
  resize: Option<&PanelResizeConfig>,
) -> Div {
  let handle_size = resize
    .map(|resize| resize.handle_size)
    .unwrap_or(DEFAULT_PANEL_SPLIT_HANDLE_SIZE);
  let mut handle = div()
    .flex_none()
    .bg(theme.border.primary)
    .hover(move |style| style.bg(theme.border.focus));

  handle = match axis {
    PanelSplitAxis::Horizontal => handle
      .w(handle_size)
      .h_full()
      .cursor(CursorStyle::ResizeLeftRight),
    PanelSplitAxis::Vertical => handle
      .h(handle_size)
      .w_full()
      .cursor(CursorStyle::ResizeUpDown),
  };

  if let Some(resize) = resize {
    let path = path.clone();
    let on_resize_start = resize.on_resize_start.clone();
    handle = handle.on_mouse_down(MouseButton::Left, move |event, window, cx| {
      let cursor_position = match axis {
        PanelSplitAxis::Horizontal => event.position.x,
        PanelSplitAxis::Vertical => event.position.y,
      };
      on_resize_start(path.clone(), axis, cursor_position, window, cx);
    });
  }

  handle
}

/// Renders one leaf panel and its active tab body.
///
/// # Parameters
///
/// `leaf` is the leaf panel to render.
///
/// `context` contains theme tokens, callbacks, and active tab body rendering.
///
/// # Returns
///
/// A GPUI `Div` for the leaf panel.
fn render_panel_leaf<T>(leaf: &PanelLeaf<T>, context: &PanelRenderContext<'_, T>) -> Div {
  div()
    .flex()
    .flex_col()
    .size_full()
    .min_w_0()
    .min_h_0()
    .bg(context.theme.background.primary)
    .child(render_panel_tab_strip(leaf, context))
    .child(render_panel_body(leaf, context.theme, context.render_body))
}

/// Renders the tab strip for one leaf panel.
///
/// # Parameters
///
/// `leaf` is the leaf panel whose tabs are rendered.
///
/// `context` contains theme tokens, callbacks, and optional tab-strip actions.
///
/// # Returns
///
/// A GPUI `Div` containing tab buttons.
fn render_panel_tab_strip<T>(leaf: &PanelLeaf<T>, context: &PanelRenderContext<'_, T>) -> Div {
  let panel_focused = context.config.focused_panel_id == Some(leaf.id);
  let active_drop_target = context
    .config
    .tab_drag
    .as_ref()
    .and_then(|config| config.state.as_ref())
    .and_then(|state| state.drop_target)
    .filter(|target| target.panel_id == leaf.id);
  let mut tab_lane = div()
    .relative()
    .flex()
    .items_center()
    .flex_1()
    .min_w_0()
    .h_full()
    .overflow_hidden()
    .children(leaf.tabs.iter().enumerate().map(|(index, tab)| {
      render_panel_tab(
        leaf.id,
        index,
        tab,
        panel_focused,
        leaf.active_tab == Some(tab.id),
        context,
      )
    }))
    .when(
      active_drop_target.is_some_and(|target| target.insertion_index >= leaf.tabs.len())
        && !leaf.tabs.is_empty(),
      |lane| {
        lane.child(render_panel_tab_insertion_marker(
          context.theme.border.focus.into(),
        ))
      },
    )
    .when(
      active_drop_target.is_some() && leaf.tabs.is_empty(),
      |lane| {
        lane.child(
          div()
            .absolute()
            .inset_0()
            .border_1()
            .border_color(context.theme.border.focus),
        )
      },
    );

  if let Some(tab_drag) = context.config.tab_drag.as_ref() {
    let on_target = tab_drag.on_target.clone();
    let on_drop = tab_drag.on_drop.clone();
    let panel_id = leaf.id;
    let insertion_index = leaf.tabs.len();
    tab_lane = tab_lane
      .on_drag_move::<PanelTabDrag>(move |event, window, cx| {
        if event.bounds.contains(&event.event.position) {
          on_target(
            PanelTabDropTarget {
              panel_id,
              insertion_index,
            },
            window,
            cx,
          );
        }
      })
      .on_drop(move |drag: &PanelTabDrag, window, cx| {
        on_drop(drag.clone(), panel_id, window, cx);
      });
  }

  let mut tab_strip = div()
    .flex()
    .items_center()
    .h(DEFAULT_PANEL_TAB_STRIP_HEIGHT)
    .min_w_0()
    .overflow_hidden()
    .bg(context.theme.background.primary)
    .child(tab_lane);

  if let Some(render_tab_strip_actions) = context.config.render_tab_strip_actions.as_ref()
    && panel_focused
  {
    tab_strip = tab_strip.child(render_tab_strip_actions(leaf.id));
  }

  tab_strip
}

/// Renders one tab button.
///
/// # Parameters
///
/// `panel_id` identifies the leaf panel containing the tab.
///
/// `tab` is the tab metadata to render.
///
/// `active` controls active tab styling.
///
/// `context` contains theme tokens, callbacks, and optional icon rendering.
///
/// # Returns
///
/// A GPUI `Div` for the tab button.
fn render_panel_tab<T>(
  panel_id: PanelId,
  tab_index: usize,
  tab: &PanelTab<T>,
  panel_focused: bool,
  active: bool,
  context: &PanelRenderContext<'_, T>,
) -> impl IntoElement {
  let tab_id = tab.id;
  let tab_hover_group =
    SharedString::from(format!("panel-tab-{}-{}", panel_id.value(), tab_id.value()));
  let theme = context.theme;
  let drag_state = context
    .config
    .tab_drag
    .as_ref()
    .and_then(|config| config.state.as_ref());
  let dragged = drag_state
    .is_some_and(|state| state.drag.source_panel_id == panel_id && state.drag.tab_id == tab_id);
  let insertion_before = drag_state
    .and_then(|state| state.drop_target)
    .is_some_and(|target| target.panel_id == panel_id && target.insertion_index == tab_index);
  let mut tab_element = div()
    .id(tab_hover_group.clone())
    .relative()
    .group(tab_hover_group.clone())
    .flex()
    .items_center()
    .h_full()
    .max_w(px(220.0))
    .min_w(px(72.0))
    .border_r_1()
    .border_color(theme.border.primary)
    .text_xs()
    .cursor_pointer()
    .text_color(if active {
      theme.text.primary
    } else {
      theme.text.secondary
    })
    .bg(if active {
      theme.background.secondary
    } else {
      theme.background.primary
    })
    .hover(move |style| style.bg(theme.background.secondary))
    .when(dragged, |tab| tab.opacity(0.55))
    .child(
      div()
        .flex()
        .items_center()
        .flex_1()
        .min_w_0()
        .h_full()
        .pl_3()
        .child(div().min_w_0().truncate().child(tab.title.clone())),
    )
    .child(
      div()
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .h_full()
        .w(DEFAULT_PANEL_TAB_TRAILING_ACTION_WIDTH)
        .child(render_panel_tab_close_button(
          panel_id,
          tab_id,
          active,
          tab_hover_group,
          context,
        )),
    );

  if insertion_before {
    tab_element = tab_element.child(render_panel_tab_insertion_marker(theme.border.focus.into()));
  }

  if panel_focused && active {
    tab_element = tab_element.child(
      div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(1.0))
        .bg(theme.border.focus),
    );
  }

  if let Some(tab_drag) = context.config.tab_drag.as_ref() {
    let drag = PanelTabDrag {
      source_panel_id: panel_id,
      tab_id,
      source_index: tab_index,
      title: tab.title.clone(),
    };
    let on_start = tab_drag.on_start.clone();
    tab_element = tab_element
      .cursor_move()
      .on_drag(drag, move |drag, _, window, cx| {
        on_start(drag.clone(), window, cx);
        cx.new(|_| PanelTabDragPreview {
          title: drag.title.clone(),
          theme,
        })
      });
  }

  if let Some(tab_drag) = context.config.tab_drag.as_ref() {
    let on_target = tab_drag.on_target.clone();
    tab_element = tab_element.on_drag_move::<PanelTabDrag>(move |event, window, cx| {
      if !event.bounds.contains(&event.event.position) {
        return;
      }

      let local_index =
        panel_tab_insertion_index(std::slice::from_ref(&event.bounds), event.event.position.x);
      on_target(
        PanelTabDropTarget {
          panel_id,
          insertion_index: tab_index + local_index,
        },
        window,
        cx,
      );
    });
  }

  if let Some(on_activate_tab) = context.config.on_activate_tab.as_ref() {
    let on_activate_tab = on_activate_tab.clone();
    tab_element = tab_element.on_click(move |_, window, cx| {
      on_activate_tab(panel_id, tab_id, window, cx);
    });
  }

  tab_element
}

/// Renders an overlay marker at one tab insertion boundary.
///
/// # Parameters
///
/// `color` is the theme color painted by the insertion marker.
///
/// # Returns
///
/// A zero-width relative element whose absolute child paints a two-pixel
/// vertical marker without changing tab-strip geometry.
fn render_panel_tab_insertion_marker(color: gpui::Hsla) -> Div {
  div().relative().w_0().h_full().child(
    div()
      .absolute()
      .left_0()
      .top_0()
      .bottom_0()
      .w(px(2.0))
      .bg(color),
  )
}

/// Renders the reserved close button slot for one panel tab.
///
/// The slot is always present so tab titles keep a stable width. Inactive tabs
/// start transparent and become visible when their tab hover group is hovered;
/// active tabs keep the close affordance visible.
///
/// # Parameters
///
/// `panel_id` identifies the leaf panel that owns the tab.
///
/// `tab_id` identifies the tab that should close when the button is pressed.
///
/// `active` controls whether the close glyph is visible by default.
///
/// `tab_hover_group` identifies the GPUI hover group shared with the tab.
///
/// `context` contains theme tokens, close callbacks, and optional icon
/// rendering.
///
/// # Returns
///
/// A GPUI `Div` containing the close button slot.
fn render_panel_tab_close_button<T>(
  panel_id: PanelId,
  tab_id: PanelTabId,
  active: bool,
  tab_hover_group: SharedString,
  context: &PanelRenderContext<'_, T>,
) -> Div {
  let theme = context.theme;
  let close_icon = context
    .config
    .render_tab_close_icon
    .as_ref()
    .map(|render_icon| render_icon(theme))
    .unwrap_or_else(|| div().text_xs().child("x").into_any_element());
  let mut close_button = div()
    .flex()
    .items_center()
    .justify_center()
    .flex_none()
    .size(DEFAULT_PANEL_TAB_CLOSE_BUTTON_SIZE)
    .rounded_sm()
    .text_color(if active {
      theme.text.primary
    } else {
      theme.text.secondary
    })
    .opacity(if active { 1.0 } else { 0.0 })
    .group_hover(tab_hover_group, |style| style.opacity(1.0))
    .hover(move |style| {
      style
        .bg(theme.background.hover)
        .text_color(theme.text.primary)
    })
    .child(close_icon);

  if let Some(on_close_tab) = context.config.on_close_tab.as_ref() {
    let on_close_tab = on_close_tab.clone();
    close_button = close_button
      .cursor_pointer()
      .on_mouse_down(MouseButton::Left, |_, _, cx| {
        cx.stop_propagation();
      })
      .on_mouse_up(MouseButton::Left, move |_, window, cx| {
        cx.stop_propagation();
        on_close_tab(panel_id, tab_id, window, cx);
      });
  }

  close_button
}

/// Renders the body region for one leaf panel.
///
/// # Parameters
///
/// `leaf` is the leaf panel whose active tab body should render.
///
/// `theme` supplies visual tokens.
///
/// `render_body` renders active tab content.
///
/// # Returns
///
/// A GPUI `Div` for the panel body.
fn render_panel_body<T>(
  leaf: &PanelLeaf<T>,
  theme: UIThemes,
  render_body: &dyn Fn(&PanelTab<T>) -> AnyElement,
) -> Div {
  let body = div()
    .relative()
    .flex()
    .flex_col()
    .flex_1()
    .min_h_0()
    .min_w_0()
    .bg(theme.background.secondary)
    .child(
      // here we use an ui trick, using a child gap at the top of body to fill
      // the gap
      div()
        .absolute()
        .left_0()
        .right_0()
        .top(px(-1.0))
        .h(px(2.0))
        .bg(theme.background.secondary),
    );

  match leaf.active_tab() {
    Some(tab) => body.child(render_body(tab)),
    None => body.child(
      div()
        .flex()
        .items_center()
        .justify_center()
        .size_full()
        .text_sm()
        .text_color(theme.text.secondary)
        .child("No tabs"),
    ),
  }
}
