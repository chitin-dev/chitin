//! GPUI rendering for the document panel area.

use std::{rc::Rc, time::Instant};

use chitin_command::PanelTabCommand;
use chitin_molecule_renderer::AtomRepresentation;
use chitin_ui::{
  composite::panel::{
    PanelContainerConfig, PanelId, PanelResizeConfig, PanelSplitAxis, PanelTabActivateHandler, PanelTabCloseHandler,
    PanelTabCloseIconRenderer, PanelTabDragConfig, PanelTabDragStartHandler, PanelTabDragTargetHandler,
    PanelTabDropHandler, render_panel_container,
  },
  primitive::popover::{Popover, PopoverPlacement, PopoverStyle},
  primitive::{
    button::{Button, ButtonEvent, ButtonSize, ButtonState, ButtonStyle, ButtonVariant},
    menu::{Menu, MenuEvent, MenuOption, MenuState},
  },
  themes::UIThemes,
};
use gpui::{
  AnyElement, App, AppContext, AsyncApp, Context, Entity, FocusHandle, FontWeight, InteractiveElement, IntoElement,
  MouseButton, ParentElement, Pixels, RenderOnce, Styled, Subscription, WeakEntity, Window, div, point,
  prelude::FluentBuilder, px, size, svg,
};

use crate::{
  app::ChitinApp,
  keybindings::{CloseTab, FocusNextPanelTab, FocusPreviousPanelTab, PANEL_CONTAINER_KEY_CONTEXT},
};

use super::state::{DocumentPanelContent, DocumentPanelState, OpenedProjectDocument};

/// Asset path for the horizontal split panel action.
const SPLIT_HORIZONTAL_ICON_PATH: &str = "icons/panel-split-horizontal.svg";
/// Asset path for the vertical split panel action.
const SPLIT_VERTICAL_ICON_PATH: &str = "icons/panel-split-vertical.svg";
/// Asset path for the molecular document options action.
const PANEL_MORE_ICON_PATH: &str = "icons/panel-more.svg";
/// Asset path for the stick representation icon.
const STICK_ICON_PATH: &str = "icons/atom-stick.svg";
/// Asset path for the ball-and-stick representation icon.
const BALL_AND_STICK_ICON_PATH: &str = "icons/atom-ball-and-stick.svg";
/// Asset path for the sphere representation icon.
const SPHERE_ICON_PATH: &str = "icons/atom-sphere.svg";
/// Asset path for the cartoon representation icon.
const CARTOON_ICON_PATH: &str = "icons/atom-cartoon.svg";
/// Asset path for document tab close buttons.
const TAB_CLOSE_ICON_PATH: &str = "icons/tab-close.svg";
/// Size used by tab strip action icons.
const PANEL_ACTION_ICON_SIZE: Pixels = px(16.0);
/// Size used by close tab button icons.
const TAB_CLOSE_ICON_SIZE: Pixels = px(12.0);
/// Width of the molecular document options menu.
const DOCUMENT_OPTIONS_MENU_WIDTH: Pixels = px(220.0);

/// Deferred molecular document menu rendered above the clipped tab strip.
#[derive(IntoElement)]
struct DocumentOptionsMenu {
  /// Panel whose active molecular view receives menu changes.
  panel_id: PanelId,
  /// Representation selected when this menu is rendered.
  atom_representation: AtomRepresentation,
  /// Semantic colors for the menu surface and rows.
  theme: UIThemes,
  /// Weak root app entity used by dismissal and selection callbacks.
  app: WeakEntity<ChitinApp>,
  /// Trigger bounds used to align the popup with the ellipsis button.
  options_menu_anchor: Option<gpui::Bounds<Pixels>>,
  /// Persistent interaction state used by the semantic menu primitive.
  controls: DocumentOptionsControls,
}

/// Persistent semantic controls used by molecular document panels.
#[derive(Clone)]
pub(crate) struct DocumentOptionsControls {
  /// Button state for the ellipsis trigger.
  more: Entity<ButtonState>,
  /// Menu state for atom representation choices.
  menu: Entity<MenuState>,
}

impl DocumentOptionsControls {
  /// Creates the persistent trigger and representation menu state.
  pub(crate) fn new(cx: &mut Context<ChitinApp>) -> Self {
    let menu = cx.new(|cx| {
      MenuState::new(
        [
          MenuOption::new("stick", "Stick").icon(STICK_ICON_PATH),
          MenuOption::new("ball-and-stick", "Ball and stick").icon(BALL_AND_STICK_ICON_PATH),
          MenuOption::new("sphere", "Sphere").icon(SPHERE_ICON_PATH),
          MenuOption::new("cartoon", "Cartoon").icon(CARTOON_ICON_PATH),
        ],
        cx,
      )
    });
    Self {
      more: cx.new(ButtonState::new),
      menu,
    }
  }

  /// Subscribes semantic button and menu events to document-panel commands.
  ///
  /// # Parameters
  ///
  /// * `window` supplies focus routing for the menu when it opens.
  /// * `cx` owns subscriptions and updates the application state.
  pub(crate) fn subscribe(&self, window: &mut Window, cx: &mut Context<ChitinApp>) {
    let menu = self.menu.clone();
    let subscription: Subscription = cx.subscribe_in(&self.more, window, move |app, _, event, window, cx| {
      if !matches!(event, ButtonEvent::Click) {
        return;
      }
      let panel_id = app.document_panels.focused_panel_id;
      if app.toggle_document_options_menu(panel_id) && app.document_panels.options_menu_panel_id == Some(panel_id) {
        let focus = menu.read(cx).focus_handle().clone();
        window.focus(&focus, cx);
      }
      cx.notify();
    });
    subscription.detach();

    let subscription: Subscription = cx.subscribe_in(&self.menu, window, move |app, _, event, _, cx| {
      let MenuEvent::SelectionChange { selected_id } = event else {
        return;
      };
      let Some(panel_id) = app.document_panels.options_menu_panel_id else {
        return;
      };
      let representation = match selected_id.as_ref() {
        "stick" => AtomRepresentation::Stick,
        "ball-and-stick" => AtomRepresentation::BallAndStick,
        "sphere" => AtomRepresentation::Sphere,
        "cartoon" => AtomRepresentation::Cartoon,
        _ => return,
      };
      app.select_document_atom_representation(panel_id, representation, cx);
      cx.notify();
    });
    subscription.detach();
  }
}

/// Renders the main workbench document area.
///
/// The document area always renders the panel container. Before any file is
/// opened, the container shows its empty root panel instead of the legacy
/// workspace summary placeholder.
///
/// # Parameters
///
/// * `document_panels` contains the current document panel layout.
/// * `theme` supplies colors for the document area.
/// * `focus_handle` is the focus handle used to track and direct keyboard focus
///   for the document area element.
/// * `app` is the weak app entity used by document panel callbacks.
/// * `controls` contains persistent button and menu interaction state.
/// * `cx` is the GPUI context used by action listeners and callbacks to update
///   application state.
///
/// # Returns
///
/// A GPUI element for the main document region.
pub fn render_document_area(
  document_panels: &DocumentPanelState,
  theme: UIThemes,
  focus_handle: &FocusHandle,
  app: WeakEntity<ChitinApp>,
  controls: DocumentOptionsControls,
  cx: &mut Context<ChitinApp>,
) -> impl IntoElement {
  render_opened_document_panels(document_panels, theme, focus_handle, app, controls, cx)
}

/// Renders document panels for files opened from the workspace tree.
///
/// # Parameters
///
/// * `document_panels` contains the panel tree and tab state to render.
/// * `theme` supplies colors for the placeholder document surface.
/// * `focus_handle` is the focus handle used to track and direct keyboard focus
///   for the panel container element.
/// * `app` is the weak app entity used by tab and split button callbacks.
/// * `controls` contains persistent button and menu interaction state.
/// * `cx` is the GPUI context used by action listeners and callbacks to update
///   application state.
///
/// # Returns
///
/// A GPUI `Div` containing the placeholder opened-document view.
fn render_opened_document_panels(
  document_panels: &DocumentPanelState,
  theme: UIThemes,
  focus_handle: &FocusHandle,
  app: WeakEntity<ChitinApp>,
  controls: DocumentOptionsControls,
  cx: &mut Context<ChitinApp>,
) -> gpui::Div {
  let activate_app = app.clone();
  let on_activate_tab: Rc<PanelTabActivateHandler> =
    Rc::new(move |panel_id: PanelId, tab_id, _: &mut Window, cx: &mut App| {
      let _ = activate_app.update(cx, |app, cx| {
        if app.activate_document_panel_tab(panel_id, tab_id) {
          cx.notify();
        }
      });
    });
  let close_app = app.clone();
  let on_close_tab: Rc<PanelTabCloseHandler> =
    Rc::new(move |panel_id: PanelId, tab_id, _: &mut Window, cx: &mut App| {
      let _ = close_app.update(cx, |app, cx| {
        if app.close_document_panel_tab(panel_id, tab_id) {
          cx.notify();
        }
      });
    });
  let render_tab_close_icon: Rc<PanelTabCloseIconRenderer> = Rc::new(move |theme| {
    svg()
      .path(TAB_CLOSE_ICON_PATH)
      .size(TAB_CLOSE_ICON_SIZE)
      .text_color(theme.text.primary)
      .into_any_element()
  });
  let drag_start_app = app.clone();
  let on_tab_drag_start: Rc<PanelTabDragStartHandler> = Rc::new(move |drag, _: &mut Window, cx: &mut App| {
    let _ = drag_start_app.update(cx, |app, cx| {
      if app.start_document_panel_tab_drag(drag) {
        cx.notify();
      }
    });
  });
  let drag_target_app = app.clone();
  let on_tab_drag_target: Rc<PanelTabDragTargetHandler> = Rc::new(move |target, _: &mut Window, cx: &mut App| {
    let _ = drag_target_app.update(cx, |app, cx| {
      if app.update_document_panel_tab_drag_target(target) {
        cx.notify();
      }
    });
  });
  let drop_app = app.clone();
  let on_tab_drop: Rc<PanelTabDropHandler> = Rc::new(move |drag, panel_id, _: &mut Window, cx: &mut App| {
    let _ = drop_app.update(cx, |app, cx| {
      app.drop_document_panel_tab(drag, panel_id);
      cx.notify();
    });
  });
  let tab_drag =
    PanelTabDragConfig::new(on_tab_drag_start, on_tab_drag_target, on_tab_drop).state(document_panels.tab_drag.clone());

  let actions_app = app.clone();
  let panel_state = document_panels.clone();
  let render_tab_strip_actions = Rc::new(move |panel_id| {
    let atom_representation = panel_state.active_atom_representation(panel_id);
    let options_menu_open = panel_state.options_menu_panel_id == Some(panel_id);
    let options_menu_anchor = options_menu_open.then_some(panel_state.options_menu_anchor).flatten();
    render_panel_tab_strip_actions(
      panel_id,
      theme,
      actions_app.clone(),
      atom_representation,
      options_menu_open,
      options_menu_anchor,
      controls.clone(),
    )
    .into_any_element()
  });
  let resize_app = app.clone();
  let resize = PanelResizeConfig::new(move |path, axis, start_position, _, cx| {
    let _ = resize_app.update(cx, |app, cx| {
      if app.start_document_panel_resize(path, axis, start_position) {
        cx.notify();
      }
    });
  });
  let now = Instant::now();
  let panel_config = PanelContainerConfig::new()
    .resize(resize)
    .focused_panel_id(document_panels.focused_panel_id)
    .on_activate_tab(on_activate_tab)
    .on_close_tab(on_close_tab)
    .render_tab_close_icon(render_tab_close_icon)
    .render_tab_strip_actions(render_tab_strip_actions)
    .tab_drag(tab_drag)
    .tab_scroll(document_panels.tab_scroll.clone())
    .now(now);

  let mouse_focus_handle = focus_handle.clone();
  let panel_container = render_panel_container(&document_panels.tree, theme, panel_config, &|tab| {
    render_document_panel_content(&tab.payload, theme).into_any_element()
  });
  schedule_tab_scroll_indicator_hide(document_panels, now, cx);

  div()
    .flex()
    .flex_1()
    .min_w_0()
    .min_h_0()
    .track_focus(focus_handle)
    .key_context(PANEL_CONTAINER_KEY_CONTEXT)
    .on_action(cx.listener(|this, _: &FocusPreviousPanelTab, _, cx| {
      this.dispatch_command(PanelTabCommand::FocusPrevious.into(), cx);
    }))
    .on_action(cx.listener(|this, _: &FocusNextPanelTab, _, cx| {
      this.dispatch_command(PanelTabCommand::FocusNext.into(), cx);
    }))
    .on_action(cx.listener(|this, _: &CloseTab, _, cx| {
      this.dispatch_command(PanelTabCommand::Close.into(), cx);
    }))
    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
      window.focus(&mouse_focus_handle, cx);
    })
    .child(panel_container)
}

/// Schedules redraws for expiring tab-scroll indicators.
///
/// # Parameters
///
/// * `document_panels` owns the presentation-only scroll indicator state.
/// * `now` is the current UI clock timestamp.
/// * `cx` schedules the wake-up task and receives the redraw notification.
///
/// # Returns
///
/// This function has no return value.
fn schedule_tab_scroll_indicator_hide(document_panels: &DocumentPanelState, now: Instant, cx: &mut Context<ChitinApp>) {
  for expires_at in document_panels.tab_scroll.indicator_redraws_to_schedule(now) {
    let delay = expires_at.saturating_duration_since(now);
    cx.spawn(move |this: WeakEntity<ChitinApp>, async_cx: &mut AsyncApp| {
      let mut async_cx = async_cx.clone();
      async move {
        let timer = async_cx.background_executor().timer(delay);
        timer.await;
        let _ = this.update(&mut async_cx, |_, cx| cx.notify());
      }
    })
    .detach();
  }
}

/// Renders split controls at the right end of one document panel tab strip.
///
/// # Parameters
///
/// * `panel_id` identifies the panel controlled by the split buttons.
/// * `theme` supplies colors for the action buttons.
/// * `app` is the weak app entity updated by button clicks.
/// * `controls` contains the persistent trigger state and menu state.
///
/// # Returns
///
/// A GPUI element containing horizontal and vertical split buttons.
fn render_panel_tab_strip_actions(
  panel_id: PanelId,
  theme: UIThemes,
  app: WeakEntity<ChitinApp>,
  atom_representation: Option<AtomRepresentation>,
  options_menu_open: bool,
  options_menu_anchor: Option<gpui::Bounds<Pixels>>,
  controls: DocumentOptionsControls,
) -> gpui::Div {
  let mut actions = div()
    .flex()
    .items_center()
    .h_full()
    .flex_none()
    .border_l_1()
    .border_color(theme.border.primary)
    .bg(theme.background.primary);
  if let Some(atom_representation) = atom_representation {
    actions = actions.child(render_document_options_button(
      panel_id,
      atom_representation,
      options_menu_open,
      theme,
      app.clone(),
      options_menu_anchor,
      controls,
    ));
  }
  actions
    .child(render_panel_split_button(
      panel_id,
      PanelSplitAxis::Horizontal,
      SPLIT_HORIZONTAL_ICON_PATH,
      theme,
      app.clone(),
    ))
    .child(render_panel_split_button(
      panel_id,
      PanelSplitAxis::Vertical,
      SPLIT_VERTICAL_ICON_PATH,
      theme,
      app,
    ))
}

/// Renders the semantic molecular document menu trigger and its popup.
///
/// The trigger is a [`Button`] so pointer and keyboard activation follow the
/// shared primitive contract. The popup is backed by [`MenuState`] so its
/// options remain focusable after the panel is redrawn.
fn render_document_options_button(
  panel_id: PanelId,
  atom_representation: AtomRepresentation,
  open: bool,
  theme: UIThemes,
  app: WeakEntity<ChitinApp>,
  options_menu_anchor: Option<gpui::Bounds<Pixels>>,
  controls: DocumentOptionsControls,
) -> gpui::Div {
  let trigger = Button::new(controls.more.clone())
    .size(ButtonSize::Small)
    .variant(ButtonVariant::Transparent)
    .style(
      ButtonStyle::new()
        .width(px(30.0))
        .height(px(30.0))
        .horizontal_padding(px(0.0)),
    )
    .theme(theme)
    .child(
      svg()
        .path(PANEL_MORE_ICON_PATH)
        .size(PANEL_ACTION_ICON_SIZE)
        .text_color(theme.text.secondary),
    );

  let anchor_app = app.clone();
  div()
    .relative()
    .size(px(30.0))
    .on_children_prepainted(move |children_bounds, _, cx| {
      if let Some(bounds) = children_bounds.first().copied() {
        let _ = anchor_app.update(cx, |app, cx| {
          if app.document_panels.set_options_menu_anchor(panel_id, bounds) {
            cx.notify();
          }
        });
      }
    })
    .child(trigger)
    // Wait for the trigger's window-space bounds before rendering the popup.
    // Rendering the fallback local placement for one frame causes a visible
    // jump when the deferred overlay is repositioned to its real anchor.
    .when(open && options_menu_anchor.is_some(), |anchor| {
      anchor.child(render_document_options_menu(
        panel_id,
        atom_representation,
        theme,
        app,
        options_menu_anchor,
        controls,
      ))
    })
}

/// Creates the deferred popup menu for one molecular document panel.
///
/// Rendering is deferred because the panel tab strip clips its children; the
/// returned element contributes the popup backdrop and surface at the root
/// overlay layer.
fn render_document_options_menu(
  panel_id: PanelId,
  atom_representation: AtomRepresentation,
  theme: UIThemes,
  app: WeakEntity<ChitinApp>,
  options_menu_anchor: Option<gpui::Bounds<Pixels>>,
  controls: DocumentOptionsControls,
) -> DocumentOptionsMenu {
  DocumentOptionsMenu {
    panel_id,
    atom_representation,
    theme,
    app,
    options_menu_anchor,
    controls,
  }
}

impl RenderOnce for DocumentOptionsMenu {
  fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let selected_id = atom_representation_id(self.atom_representation);
    self
      .controls
      .menu
      .update(cx, |state, cx| state.set_selected_id(Some(selected_id), cx));
    let popover = build_document_options_popover(
      self.panel_id,
      self.theme,
      self.app,
      self.options_menu_anchor,
      self.controls,
    );
    let viewport_size = window.viewport_size();
    div()
      .child(popover.deferred_backdrop(viewport_size))
      .child(popover.deferred_content())
  }
}

/// Builds the molecular document popover surface and dismissal interaction.
///
/// Selection itself is handled by [`MenuState`] and routed through
/// [`MenuEvent`] by [`DocumentOptionsControls`]. Keeping dismissal here lets
/// the generic menu remain unaware of panel layout state.
fn build_document_options_popover(
  panel_id: PanelId,
  theme: UIThemes,
  app: WeakEntity<ChitinApp>,
  options_menu_anchor: Option<gpui::Bounds<Pixels>>,
  controls: DocumentOptionsControls,
) -> Popover {
  let representation_section = div()
    .flex()
    .flex_col()
    .text_size(px(12.0))
    .child(
      div()
        .h(px(28.0))
        .flex()
        .items_center()
        .px_2()
        .text_xs()
        .text_color(theme.text.secondary)
        .child("Atom representation"),
    )
    .child(Menu::new(controls.menu).theme(theme).width(DOCUMENT_OPTIONS_MENU_WIDTH));

  let dismiss_app = app;
  let mut popover = Popover::new(("document-options-menu", panel_id.value()), representation_section)
    .anchor_size(size(px(30.0), px(30.0)))
    .style(
      PopoverStyle::new()
        .width(DOCUMENT_OPTIONS_MENU_WIDTH)
        .background(theme.background.secondary)
        .border_color(theme.border.primary),
    )
    .theme(theme)
    .on_dismiss(move |_, cx| {
      let _ = dismiss_app.update(cx, |app, cx| {
        if app.dismiss_document_options_menu() {
          cx.notify();
        }
      });
    });
  if let Some(bounds) = options_menu_anchor {
    popover = popover
      .anchor_position(point(
        bounds.origin.x + bounds.size.width,
        bounds.origin.y + bounds.size.height,
      ))
      .offset(point(px(-220.0), px(0.0)));
  } else {
    popover = popover.placement(PopoverPlacement::Below);
  }
  popover
}

/// Returns the stable menu identifier for one atom representation.
fn atom_representation_id(representation: AtomRepresentation) -> &'static str {
  match representation {
    AtomRepresentation::Stick => "stick",
    AtomRepresentation::BallAndStick => "ball-and-stick",
    AtomRepresentation::Sphere => "sphere",
    AtomRepresentation::Cartoon => "cartoon",
  }
}

/// Renders one split button for a document panel.
///
/// # Parameters
///
/// * `panel_id` identifies the panel that should be split when clicked.
/// * `axis` controls the orientation of the created split.
/// * `icon_path` is the desktop asset path for the button icon.
/// * `theme` supplies colors for the button.
/// * `app` is the weak app entity updated by button clicks.
///
/// # Returns
///
/// A GPUI element for one icon-only split button.
fn render_panel_split_button(
  panel_id: PanelId,
  axis: PanelSplitAxis,
  icon_path: &'static str,
  theme: UIThemes,
  app: WeakEntity<ChitinApp>,
) -> gpui::Div {
  div()
    .flex()
    .items_center()
    .justify_center()
    .size(px(30.0))
    .cursor_pointer()
    .text_color(theme.text.secondary)
    .hover(move |style| style.bg(theme.background.hover).text_color(theme.text.primary))
    .on_mouse_up(MouseButton::Left, move |_, window, cx| {
      let _ = app.update(cx, |app, cx| {
        if app.split_document_panel(panel_id, axis, window, cx) {
          cx.notify();
        }
      });
    })
    .child(
      svg()
        .path(icon_path)
        .size(PANEL_ACTION_ICON_SIZE)
        .text_color(theme.text.secondary),
    )
}

/// Renders the body for one document-panel tab payload.
///
/// # Parameters
///
/// * `content` is the tab payload selected by the panel container.
/// * `theme` supplies colors for placeholder project-document content.
///
/// # Returns
///
/// A GPUI element for either a project document placeholder or WGPU viewport.
fn render_document_panel_content(content: &DocumentPanelContent, theme: UIThemes) -> AnyElement {
  match content {
    DocumentPanelContent::ProjectDocument(document) => render_opened_document_body(document, theme),
    DocumentPanelContent::WgpuInteractive { view, .. } => view.clone().into_any_element(),
  }
}

/// Renders placeholder content for an opened document tab.
///
/// # Parameters
///
/// * `document` is the opened file descriptor to display.
/// * `theme` supplies colors for the document body.
///
/// # Returns
///
/// A GPUI element containing placeholder document content.
fn render_opened_document_body(document: &OpenedProjectDocument, theme: UIThemes) -> AnyElement {
  div()
    .flex()
    .flex_col()
    .flex_1()
    .min_h_0()
    .p_8()
    .gap_3()
    .child(
      div()
        .text_lg()
        .font_weight(FontWeight::SEMIBOLD)
        .child("Placeholder document"),
    )
    .child(
      div()
        .text_sm()
        .text_color(theme.text.secondary)
        .child(document.path.display().to_string()),
    )
    .into_any_element()
}
