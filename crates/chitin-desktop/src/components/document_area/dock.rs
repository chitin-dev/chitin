//! GPUI Kit docking adapter for application-owned documents.
//!
//! Kit is the authority for pointer interactions and the live pane hierarchy.
//! The desktop projects that hierarchy back into its command model. Stable tab
//! entities survive layout replacements, so docking never recreates GPU views.

use std::collections::{HashMap, HashSet};

use chitin_ui::composite::panel::{
  PanelId, PanelLeaf, PanelNode, PanelSplit, PanelSplitAxis, PanelTab, PanelTabId, PanelTree,
};
use gpui::prelude::FluentBuilder;
use gpui::{
  App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement,
  ParentElement, Render, SharedString, Size, Styled, WeakEntity, Window, div, px,
};
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{
  ActiveTheme,
  dock::{
    BasePanel, DockArea, DockEvent, DockLayout, DockPlacement, DockSkin, NodeId, PaneNode, PaneRef, Panel, PanelEvent,
    PanelInfo, PanelState, PanelStyle, panel_handle,
  },
};

use super::{
  render::{
    DocumentOptionsControls, MolecularDocumentOptions, render_document_panel_content, render_panel_tab_strip_actions,
  },
  state::{DEFAULT_DOCUMENT_PANEL_ID, DocumentPanelContent, DocumentPanelState},
};
use crate::app::ChitinApp;

/// Retained docking state; does not own a second drag or resize state machine.
pub(crate) struct DocumentDock {
  pub(crate) area: Entity<DockArea>,
  tabs: HashMap<PanelTabId, Entity<DocumentTab>>,
  projected: Option<PanelTree<DocumentPanelContent>>,
  /// Kit group IDs survive selection, reordering, and cross-group moves.
  leaf_ids: HashMap<NodeId, PanelId>,
}

/// One persistent Kit panel wrapping one application document.
struct DocumentTab {
  tab: PanelTab<DocumentPanelContent>,
  panel_id: PanelId,
  focus: FocusHandle,
  app: WeakEntity<ChitinApp>,
  controls: DocumentOptionsControls,
  options_open: bool,
  /// Kit keeps its final dock panel; documents may still close to an empty center.
  last_document: bool,
}

impl EventEmitter<PanelEvent> for DocumentTab {}

impl Focusable for DocumentTab {
  fn focus_handle(&self, _: &App) -> FocusHandle {
    self.focus.clone()
  }
}

impl BasePanel for DocumentTab {
  fn panel_name(&self) -> &'static str {
    "chitin.document"
  }

  fn zoomable(&self, _: &App) -> bool {
    false
  }

  fn dump(&self, _: &App) -> PanelState {
    // Preserve Kit's panel-name contract; the payload carries the application
    // identity, independent of the temporary GPUI entity ID.
    PanelState {
      panel_name: self.panel_name().to_string(),
      info: PanelInfo::panel(self.tab.id.value().into()),
      ..PanelState::default()
    }
  }
}

impl Panel for DocumentTab {
  fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    let app = self.app.clone();
    let panel = self.panel_id;
    let tab = self.tab.id;
    let focus = self.focus.clone();
    div()
      .flex()
      .items_center()
      .gap_1()
      .text_sm()
      .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| window.focus(&focus, cx))
      .child(self.tab.title.clone())
      .when(self.last_document, |title| {
        title.child(
          gpui_kit::component::button::Button::new(("close-last-document", tab.value()))
            .ghost()
            .icon(gpui_kit::component::IconName::Close)
            .size(px(18.0))
            .px(px(0.0))
            .tooltip("Close document")
            .accessibility_label("Close document")
            .debug_selector(|| "document-last-close".to_string())
            .on_click(move |_, _, cx| {
              cx.stop_propagation();
              let _ = app.update(cx, |app, cx| {
                if app.document_panels.close_tab(panel, tab) {
                  cx.notify();
                }
              });
            }),
        )
      })
  }

  fn tab_name(&self, _: &App) -> Option<SharedString> {
    None
  }

  fn inner_padding(&self, _: &App) -> bool {
    false
  }

  fn zoom_control(&self, _: &App) -> Option<gpui_kit::component::dock::PanelControl> {
    None
  }

  fn title_suffix(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Option<impl IntoElement> {
    let options = self
      .tab
      .payload
      .representation_layers()
      .zip(self.tab.payload.surface_backend())
      .map(|(representation, surface_backend)| MolecularDocumentOptions {
        representation,
        surface_backend,
      });
    Some(render_panel_tab_strip_actions(
      self.panel_id,
      cx.theme().colors,
      self.app.clone(),
      options,
      self.options_open,
      self.controls.clone(),
    ))
  }
}

impl Render for DocumentTab {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let app = self.app.clone();
    let panel_id = self.panel_id;
    // Focus changes also arise from clicks inside an already-active GPU view;
    // layout events alone cannot capture those.
    div()
      // GPU views grow through flex_1 and contain only absolutely positioned
      // surfaces/overlays; a non-flex parent leaves their height at zero.
      .flex()
      .flex_col()
      .size_full()
      .min_w_0()
      .min_h_0()
      .track_focus(&self.focus)
      .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
        let _ = app.update(cx, |app, _| app.document_panels.focused_panel_id = panel_id);
      })
      .child(render_document_panel_content(&self.tab.payload, cx.theme().colors))
  }
}

impl DocumentDock {
  /// Creates a Kit center area and bridges semantic layout changes.
  ///
  /// # Parameters
  ///
  /// * `window` creates Kit docking entities and their focus handles.
  /// * `cx` subscribes the application to Kit's layout events.
  ///
  /// # Returns
  ///
  /// An empty mount whose document entities are created by `sync`.
  pub(crate) fn new(window: &mut Window, cx: &mut Context<ChitinApp>) -> Self {
    let (area, skin) = DockSkin::dock_area("chitin-documents", None, window, cx);
    skin.set_panel_style(PanelStyle::TabBar, cx);
    skin.set_close_button_visible(true, cx);
    skin.set_toggle_button_visible(false, cx);
    skin.set_menu_button_visible(false, cx);
    skin.set_toolbar_separator_visible(false, cx);
    cx.subscribe_in(&area, window, |app, _, event, window, cx| {
      if matches!(event, DockEvent::LayoutChanged) {
        let Some(mut dock) = app.document_dock.take() else {
          return;
        };
        if dock.read_layout(&mut app.document_panels, window, cx) {
          cx.notify();
        }
        app.document_dock = Some(dock);
      }
    })
    .detach();
    Self {
      area,
      tabs: HashMap::new(),
      projected: None,
      leaf_ids: HashMap::new(),
    }
  }

  /// Reconciles domain commands with Kit without remounting on ordinary redraws.
  ///
  /// # Parameters
  ///
  /// * `state` supplies documents, selection, and requested splits.
  /// * `controls` contains the molecular options controls, not docking state.
  /// * `window` and `cx` create or update persistent document entities.
  ///
  /// # Returns
  ///
  /// Nothing. A changed command layout is installed once; existing GPU views
  /// and Kit panels are reused by stable tab ID.
  pub(crate) fn sync(
    &mut self,
    state: &DocumentPanelState,
    controls: DocumentOptionsControls,
    window: &mut Window,
    cx: &mut Context<ChitinApp>,
  ) {
    let mut entries = Vec::new();
    collect_tabs(&state.tree.root, &mut entries);
    let retained: HashSet<_> = entries.iter().map(|(_, tab)| tab.id).collect();
    let last_document = retained.len() == 1;
    for (panel_id, tab) in entries {
      if let Some(entity) = self.tabs.get(&tab.id) {
        entity.update(cx, |panel, cx| {
          let changed = panel.tab != *tab
            || panel.panel_id != panel_id
            || panel.options_open != (state.options_menu_panel_id == Some(panel_id))
            || panel.last_document != last_document;
          panel.last_document = last_document;
          panel.tab = tab.clone();
          panel.panel_id = panel_id;
          panel.options_open = state.options_menu_panel_id == Some(panel_id);
          if changed {
            cx.notify();
          }
        });
      } else {
        let app = cx.weak_entity();
        let controls = controls.clone();
        let entity = cx.new(|cx| {
          let focus = cx.focus_handle();
          // Selection is Kit-owned; only project focus into command targeting.
          cx.on_focus_in(&focus, window, |panel: &mut DocumentTab, _, cx| {
            let tab = panel.tab.id;
            let _ = panel.app.update(cx, |app, cx| {
              if let Some((id, _)) = app
                .document_panels
                .tree
                .tabs_in_order()
                .into_iter()
                .find(|(_, id)| *id == tab)
                && app.document_panels.focused_panel_id != id
              {
                app.document_panels.focused_panel_id = id;
                cx.notify();
              }
            });
          })
          .detach();
          DocumentTab {
            tab: tab.clone(),
            panel_id,
            focus,
            app,
            controls,
            options_open: state.options_menu_panel_id == Some(panel_id),
            last_document,
          }
        });
        self.tabs.insert(tab.id, entity);
      }
    }
    if self.projected.as_ref() != Some(&state.tree) {
      let measured = self.area.read(cx).bounds().size;
      let viewport = window.bounds().size;
      let extent = Size {
        width: measured.width.max(px(1.0)).max(if measured.width <= px(1.0) {
          viewport.width
        } else {
          px(0.0)
        }),
        height: measured.height.max(px(1.0)).max(if measured.height <= px(1.0) {
          viewport.height
        } else {
          px(0.0)
        }),
      };
      let layout = build_layout(&state.tree.root, &self.tabs, extent, cx);
      self.area.update(cx, |area, cx| area.set_center(layout, window, cx));
      self.remember_leaf_ids(&state.tree, cx);
      self.projected = Some(state.tree.clone());
    }
    // Drop obsolete handles only after Kit has detached the old layout.
    self.tabs.retain(|id, _| retained.contains(id));
  }

  /// Projects Kit's measured layout back into the command model.
  ///
  /// # Parameters
  ///
  /// * `state` receives topology, active tabs, and focus after an interaction.
  /// * `window` identifies the tab that currently holds keyboard focus.
  /// * `cx` reads Kit's measured split sizes and persistent document entities.
  ///
  /// # Returns
  ///
  /// Whether the application layout changed. Events from an obsolete layout
  /// are ignored when a newer domain command has not yet been installed.
  fn read_layout(&mut self, state: &mut DocumentPanelState, window: &mut Window, cx: &App) -> bool {
    if self.projected.as_ref() != Some(&state.tree) {
      return false;
    }
    let area = self.area.read(cx);
    let mut live = area.dump(cx).center;
    if let Some(layout) = area.layout(DockPlacement::Center) {
      tag_leaf_ids(layout.root(), &mut live, &self.leaf_ids);
    }
    let mut entries = Vec::new();
    collect_tabs(&state.tree.root, &mut entries);
    // Use the latest projection, not panel fields awaiting the next render.
    // Several Kit events may be delivered before the root redraws.
    let sources: HashMap<_, _> = entries
      .into_iter()
      .map(|(panel, tab)| (tab.id, (panel, tab.clone())))
      .collect();
    let mut used = HashSet::new();
    let root = project_layout(&live, &sources, &mut state.next_panel_id, &mut used);
    let tree = PanelTree { root };
    let focused_tab = self
      .tabs
      .iter()
      .find(|(_, entity)| {
        let focus = &entity.read(cx).focus;
        // A moved focused panel may not be in the last painted focus path yet.
        focus.is_focused(window) || focus.contains_focused(window, cx)
      })
      .map(|(id, _)| *id);
    let focused = tree
      .tabs_in_order()
      .into_iter()
      .find(|(_, id)| Some(*id) == focused_tab)
      .map(|(panel, _)| panel)
      .or_else(|| tree.leaf(state.focused_panel_id).map(|leaf| leaf.id))
      .unwrap_or_else(|| {
        tree
          .first_active_tab()
          .map(|(id, _)| id)
          .unwrap_or(DEFAULT_DOCUMENT_PANEL_ID)
      });
    let changed = state.tree != tree || state.focused_panel_id != focused;
    state.tree = tree.clone();
    state.focused_panel_id = focused;
    if state
      .options_menu_panel_id
      .is_some_and(|id| state.tree.leaf(id).is_none())
    {
      state.options_menu_panel_id = None;
    }
    self.projected = Some(tree);
    self.remember_leaf_ids(&state.tree, cx);
    changed
  }

  /// Associates live Kit groups with the corresponding command-model leaves.
  fn remember_leaf_ids(&mut self, tree: &PanelTree<DocumentPanelContent>, cx: &App) {
    let mut groups = Vec::new();
    if let Some(layout) = self.area.read(cx).layout(DockPlacement::Center) {
      layout.root().walk(&mut |node| {
        if matches!(node.kind(), PaneRef::Tabs { .. }) {
          groups.push(node.id());
        }
      });
    }
    let mut ids = Vec::new();
    collect_leaf_ids(&tree.root, &mut ids);
    self.leaf_ids = groups.into_iter().zip(ids).collect();
  }
}

/// Visits leaf identities in the same order used by Kit's normalized splits.
fn collect_leaf_ids(node: &PanelNode<DocumentPanelContent>, out: &mut Vec<PanelId>) {
  match node {
    PanelNode::Leaf(leaf) => out.push(leaf.id),
    PanelNode::Split(split) => {
      collect_leaf_ids(&split.first, out);
      collect_leaf_ids(&split.second, out);
    }
  }
}

/// Adds live group identity to a measured dump without changing panel records.
///
/// # Parameters
///
/// * `node` supplies Kit's stable node IDs.
/// * `dump` supplies the same topology with measured split sizes.
/// * `known` maps existing Kit groups to application leaf IDs.
///
/// # Returns
///
/// Nothing. Existing groups retain their IDs; groups made by drag-to-split
/// are marked for fresh allocation, even if their tab came from another leaf.
fn tag_leaf_ids(node: &PaneNode, dump: &mut PanelState, known: &HashMap<NodeId, PanelId>) {
  match node.kind() {
    PaneRef::Tabs { .. } => {
      dump.panel_name = known
        .get(&node.id())
        .map(|id| id.value().to_string())
        .unwrap_or_else(|| "new-document-group".to_string());
    }
    PaneRef::Split { children, .. } => {
      for (node, dump) in children.iter().zip(&mut dump.children) {
        tag_leaf_ids(node, dump, known);
      }
    }
  }
}

/// Visits command-model leaves without duplicating document contents.
fn collect_tabs<'a>(
  node: &'a PanelNode<DocumentPanelContent>,
  out: &mut Vec<(PanelId, &'a PanelTab<DocumentPanelContent>)>,
) {
  match node {
    PanelNode::Leaf(leaf) => out.extend(leaf.tabs.iter().map(|tab| (leaf.id, tab))),
    PanelNode::Split(split) => {
      collect_tabs(&split.first, out);
      collect_tabs(&split.second, out);
    }
  }
}

/// Lowers binary command splits into Kit's normalized docking hierarchy.
///
/// # Parameters
///
/// * `node` supplies the requested topology and relative split sizes.
/// * `tabs` resolves application IDs to stable Kit panel entities.
/// * `extent` converts ratios to pixel slots for this subtree.
/// * `cx` reads presentation handles.
///
/// # Returns
///
/// A Kit layout; the toolkit owns subsequent resizing and drag/drop.
fn build_layout(
  node: &PanelNode<DocumentPanelContent>,
  tabs: &HashMap<PanelTabId, Entity<DocumentTab>>,
  extent: Size<gpui::Pixels>,
  cx: &App,
) -> DockLayout {
  match node {
    PanelNode::Leaf(leaf) => {
      let mut layout = DockLayout::tabs();
      for tab in &leaf.tabs {
        if let Some(entity) = tabs.get(&tab.id) {
          layout = layout.panel_view(panel_handle(entity.clone()), cx);
        }
      }
      layout.active_index(
        leaf
          .tabs
          .iter()
          .position(|tab| Some(tab.id) == leaf.active_tab)
          .unwrap_or(0),
      )
    }
    PanelNode::Split(split) => {
      let (mut first, mut second) = (extent, extent);
      let (layout, total) = match split.axis {
        PanelSplitAxis::Horizontal => {
          first.width *= split.ratio;
          second.width *= 1.0 - split.ratio;
          (DockLayout::h_split(), extent.width)
        }
        PanelSplitAxis::Vertical => {
          first.height *= split.ratio;
          second.height *= 1.0 - split.ratio;
          (DockLayout::v_split(), extent.height)
        }
      };
      layout
        .child(build_layout(&split.first, tabs, first, cx), Some(total * split.ratio))
        .child(
          build_layout(&split.second, tabs, second, cx),
          Some(total * (1.0 - split.ratio)),
        )
    }
  }
}

/// Restores stable leaf identities and folds n-ary Kit splits into binary nodes.
///
/// # Parameters
///
/// * `layout` is Kit's live dump, including measured (not stale) pixel sizes.
/// * `sources` resolves persisted tab IDs to application content and prior leaves.
/// * `next` allocates IDs for groups created by Kit drag-to-split.
/// * `used` prevents two newly split groups from claiming the same prior ID.
///
/// # Returns
///
/// Equivalent application topology. Folding preserves each child's relative
/// share; an empty center becomes the canonical empty document leaf.
fn project_layout(
  layout: &PanelState,
  sources: &HashMap<PanelTabId, (PanelId, PanelTab<DocumentPanelContent>)>,
  next: &mut PanelId,
  used: &mut HashSet<PanelId>,
) -> PanelNode<DocumentPanelContent> {
  match &layout.info {
    PanelInfo::Stack { sizes, axis } => {
      let children: Vec<_> = layout
        .children
        .iter()
        .map(|child| project_layout(child, sources, next, used))
        .collect();
      fold_split(
        children,
        sizes,
        if *axis == 0 {
          PanelSplitAxis::Horizontal
        } else {
          PanelSplitAxis::Vertical
        },
      )
    }
    PanelInfo::Tabs { active_index } => {
      let tabs: Vec<_> = layout
        .children
        .iter()
        .filter_map(saved_tab_id)
        .filter_map(|id| sources.get(&id))
        .collect();
      let preferred = layout.panel_name.parse::<u64>().ok().map(PanelId::new).or_else(|| {
        if layout.panel_name == "new-document-group" {
          return None;
        }
        tabs
          .get(*active_index)
          .or_else(|| tabs.first())
          .map(|(id, _)| *id)
          .or_else(|| tabs.is_empty().then_some(DEFAULT_DOCUMENT_PANEL_ID))
      });
      let id = preferred.filter(|id| used.insert(*id)).unwrap_or_else(|| {
        let id = *next;
        *next = PanelId::new(next.value() + 1);
        used.insert(id);
        id
      });
      let active_tab = tabs.get(*active_index).or_else(|| tabs.first()).map(|(_, tab)| tab.id);
      PanelNode::Leaf(PanelLeaf {
        id,
        tabs: tabs.into_iter().map(|(_, tab)| tab.clone()).collect(),
        active_tab,
      })
    }
    // Single-panel roots may be normalized away by Kit.
    PanelInfo::Panel(_) => {
      let mut leaf = PanelLeaf::new(DEFAULT_DOCUMENT_PANEL_ID);
      if let Some(id) = saved_tab_id(layout)
        && let Some((panel, tab)) = sources.get(&id)
      {
        leaf.id = *panel;
        leaf.add_tab(tab.clone());
      }
      used.insert(leaf.id);
      PanelNode::Leaf(leaf)
    }
  }
}

/// Reads a stable application tab ID from Kit's panel payload.
fn saved_tab_id(state: &PanelState) -> Option<PanelTabId> {
  match &state.info {
    PanelInfo::Panel(value) => value.as_u64().map(PanelTabId::new),
    _ => None,
  }
}

/// Folds sibling slots while retaining their measured relative widths/heights.
fn fold_split(
  mut children: Vec<PanelNode<DocumentPanelContent>>,
  sizes: &[gpui::Pixels],
  axis: PanelSplitAxis,
) -> PanelNode<DocumentPanelContent> {
  if children.is_empty() {
    return PanelNode::Leaf(PanelLeaf::new(DEFAULT_DOCUMENT_PANEL_ID));
  }
  let first = children.remove(0);
  if children.is_empty() {
    return first;
  }
  let weights: Vec<_> = (0..=children.len())
    .map(|ix| {
      sizes
        .get(ix)
        .copied()
        .map(f32::from)
        .filter(|size| size.is_finite() && *size > 0.0)
        .unwrap_or(1.0)
    })
    .collect();
  let total: f32 = weights.iter().sum();
  let ratio = weights[0] / total;
  // Measured shares are already valid. Command-level 10–90% clamps would
  // distort an n-ary layout folded into a binary tree (e.g. twelve siblings).
  PanelNode::Split(PanelSplit {
    axis,
    ratio,
    first: Box::new(first),
    second: Box::new(fold_split(children, sizes.get(1..).unwrap_or_default(), axis)),
  })
}

#[cfg(test)]
mod tests;
