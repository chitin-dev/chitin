//! Scientific entry browsing; commands remain the authority for selection and visibility.
//!
//! The model caches scene-local topology, while this module projects rows,
//! mounts Kit controls, and submits typed commands through the desktop command
//! bus. Delayed callbacks verify rendering-view identity before using atom IDs.

mod model;
#[cfg(test)]
mod tests;
use crate::features::molecule::settings::MolecularSettingsControls;
use crate::{
  app::ChitinApp,
  workbench::documents::layout::{PanelId, PanelTabId},
};
use chitin_command::{
  RenderAtomStyle, RenderCommand, RenderLayer, RenderPolymerStyle, RenderSurfaceStyle, SelectionCommand, SelectionMode,
  VisibilityCommand, VisibilityTarget,
};
use chitin_molecule_renderer::{AtomStyle, BallAndStickStyle, RepresentationLayers};
use gpui::{
  AppContext, Context, Div, Entity, EntityId, FontWeight, InteractiveElement, IntoElement, ParentElement,
  StatefulInteractiveElement, Styled, WeakEntity, Window, div, list, px,
};
use gpui_kit::component::{
  Disableable as _, Icon, Selectable as _, Sizable as _,
  button::{Button, ButtonVariants as _},
  input::{Input, InputEvent, InputState},
  theme::ThemeColor,
};
pub(crate) use model::EntriesSidebarState;
use model::{EntryState, NodeKind, RowLayoutKey, SavedSelection};
use std::sync::Arc;

/// Persistent Kit inputs shared by selection submission and bookmark naming.
#[derive(Clone)]
pub(super) struct EntriesControls {
  /// ChimeraX-style atom specification, such as `/A:10-50`.
  query: Entity<InputState>,
  /// Optional name for a snapshot of the current canonical selection.
  name: Entity<InputState>,
}

/// Owned row projection captured by virtualized rendering and event callbacks.
///
/// `tab` identifies the command target; `view` guards scene-local atom and node
/// identities if the document's rendering entity is replaced before a click.
#[derive(Clone)]
struct TreeRow {
  /// Numeric document identity accepted by the command bus.
  tab: u64,
  /// Current docking leaf used when activating the document.
  panel: PanelId,
  /// Typed tab identity used by document activation.
  tab_id: PanelTabId,
  /// Rendering entity guard; absent for non-renderable document roots.
  view: Option<EntityId>,
  /// Scene-local index into the cached hierarchy.
  node: usize,
  /// Nesting depth controlling row indentation.
  depth: usize,
  /// Current scientific or document label.
  label: String,
  /// Scientific role controlling icon and label emphasis.
  kind: NodeKind,
  /// Atom identities represented by this row's command actions.
  atoms: Arc<Vec<chitin_bio::structure::AtomId>>,
  /// Whether this node's children appear in the tree.
  expanded: bool,
  /// Whether the node has children and needs an expansion control.
  branch: bool,
  /// Whether this row is the scientific Properties scope or an expanded layer.
  selected: bool,
  /// Object visibility and layer enablement are distinct command domains.
  visibility: RowVisibility,
  /// Up to three distinct packed sRGB colors for the row's preview.
  colors: Vec<u32>,
  /// Only an expanded representation row owns an inline settings snapshot.
  editor: Option<InlineLayerEditor>,
}

/// Immutable rendering snapshot paired with this entry's persistent controls.
#[derive(Clone)]
struct InlineLayerEditor {
  layer: RenderLayer,
  layers: RepresentationLayers,
  backend: Option<chitin_bio::surface::MolecularSurfaceBackend>,
  controls: MolecularSettingsControls,
}

/// Semantic eye action for one row; rendering layers never hide topology atoms.
#[derive(Clone)]
enum RowVisibility {
  /// Canonical hidden-atom projection for a scientific object.
  Object { hidden: usize, supported: bool },
  /// Entry-wide layer state and the typed command that toggles it.
  Layer { enabled: bool, toggle: RenderCommand },
  /// Grouping nodes have no independent visibility action.
  None,
}

/// Projects open document entries, selection controls, bookmarks, and properties.
///
/// # Parameters
///
/// * `app` owns document identities and the retained Entries cache.
/// * `theme` supplies the shared sidebar colors.
/// * `window` and `cx` mount persistent inputs and connect semantic events.
///
/// # Returns
///
/// A virtualized scientific tree and controls targeting the active renderable
/// document. Non-renderable documents remain visible as root rows, but do not
/// expose scientific selection or visibility actions.
pub(crate) fn render_entries(
  app: &mut ChitinApp,
  theme: ThemeColor,
  window: &mut Window,
  cx: &mut Context<ChitinApp>,
) -> impl IntoElement {
  let controls = app.entries_controls(window, cx);
  let tabs = app.document_panels.tree.tabs_in_order();
  let live: Vec<_> = tabs
    .iter()
    .filter_map(|(panel, tab)| app.document_panels.tree.find_tab(*panel, *tab).map(|_| tab.value()))
    .collect();
  app.entries_sidebar.entries.retain(|id, _| live.contains(id));
  let focused_tab = app
    .document_panels
    .tree
    .leaf(app.document_panels.focused_panel_id)
    .and_then(|leaf| leaf.active_tab)
    .map(|tab| tab.value());
  app.entries_sidebar.active = if focused_tab.is_some() {
    focused_tab
  } else {
    app.entries_sidebar.active.or_else(|| {
      live
        .iter()
        .copied()
        .find(|id| app.document_panels.selection_control(*id).is_some())
    })
  }
  .filter(|id| live.contains(id) && app.document_panels.selection_control(*id).is_some());
  let mut rows = Vec::new();
  let palette = BallAndStickStyle::default().palette;
  for (panel, tab) in tabs {
    let Some(entry) = app.document_panels.tree.find_tab(panel, tab) else {
      continue;
    };
    let title = entry.title.to_string();
    let Some(control) = app.document_panels.selection_control(tab.value()) else {
      rows.push(TreeRow {
        tab: tab.value(),
        panel,
        tab_id: tab,
        view: None,
        node: 0,
        depth: 0,
        label: title,
        kind: NodeKind::Structure,
        atoms: Arc::default(),
        expanded: false,
        branch: false,
        selected: app.document_panels.focused_panel_id == panel,
        visibility: RowVisibility::None,
        colors: Vec::new(),
        editor: None,
      });
      continue;
    };
    let Some(view) = app.document_panels.rendering_view_id(tab.value()) else {
      continue;
    };
    let state = app
      .entries_sidebar
      .entries
      .entry(tab.value())
      .or_insert_with(|| EntryState::new(view, control.scene.clone()));
    // Atom IDs and node indices are scene-local, so replacement also drops bookmarks.
    if state.view != view || !Arc::ptr_eq(&state.scene, &control.scene) {
      *state = EntryState::new(view, control.scene.clone());
    }
    state.sync_hidden(&control.hidden);
    state.sync_selection(&control.atoms);
    state.nodes[0].label = title.clone();
    let (_, layers, backend) = app
      .document_panels
      .rendering_settings(tab.value())
      .unwrap_or_else(|| (title.clone(), RepresentationLayers::default(), None));
    let visible = state.rows(&title);
    let editor_visible = state.open_layer.is_some_and(|layer| {
      visible
        .iter()
        .any(|(node, _, _)| state.nodes[*node].kind == NodeKind::Representation(layer))
    });
    if editor_visible && state.settings.is_none() {
      let settings = MolecularSettingsControls::new(window, cx);
      settings.subscribe(window, cx);
      state.settings = Some(settings);
    }
    if let Some(settings) = &state.settings {
      settings.bind_entry(editor_visible.then_some((tab.value(), view)));
    }
    for (node, depth, label) in visible {
      let item = &state.nodes[node];
      let appearances = layers.appearances();
      let structural = item.polymer || matches!(item.kind, NodeKind::Structure | NodeKind::Chain | NodeKind::Chains);
      // Preview the outermost enabled polymer representation, matching highlight priority.
      let layer = if let NodeKind::Representation(layer) = item.kind {
        match layer {
          RenderLayer::Atom => 0,
          RenderLayer::Polymer => 1,
          RenderLayer::Surface => 2,
        }
      } else if layers.surface_style().is_some() && appearances[2].opacity() > 0. && structural {
        2
      } else if layers.polymer_style().is_some() && appearances[1].opacity() > 0. && structural {
        1
      } else {
        0
      };
      let mut colors = Vec::new();
      for &(element, chain) in &item.samples {
        let color = srgb(layers.appearances()[layer].color_for(element, chain, &palette));
        if !colors.contains(&color) {
          colors.push(color);
        }
        if colors.len() == 3 {
          break;
        }
      }
      rows.push(TreeRow {
        tab: tab.value(),
        panel,
        tab_id: tab,
        view: Some(view),
        node,
        depth,
        label: if let NodeKind::Representation(layer) = item.kind {
          representation_label(layer, layers)
        } else {
          label
        },
        kind: item.kind,
        atoms: item.atoms.clone(),
        expanded: match item.kind {
          NodeKind::Representation(layer) => state.open_layer == Some(layer),
          _ => state.expanded.contains(&node),
        },
        branch: !item.children.is_empty() || matches!(item.kind, NodeKind::Representation(_)),
        selected: if let NodeKind::Representation(layer) = item.kind {
          state.open_layer == Some(layer)
        } else {
          app.entries_sidebar.active == Some(tab.value()) && state.selected == node
        },
        visibility: match item.kind {
          NodeKind::Representations => RowVisibility::None,
          NodeKind::Representation(layer) => representation_visibility(layer, layers),
          _ => RowVisibility::Object {
            hidden: state.hidden_counts[node],
            supported: control.supports_visibility(),
          },
        },
        colors,
        editor: match item.kind {
          NodeKind::Representation(layer) if state.open_layer == Some(layer) => {
            state.settings.as_ref().map(|controls| InlineLayerEditor {
              layer,
              layers,
              backend,
              controls: controls.clone(),
            })
          }
          _ => None,
        },
      });
    }
  }
  let weak = cx.weak_entity();
  let count = live.len();
  let layout = rows
    .iter()
    .map(|row| RowLayoutKey {
      tab: row.tab,
      view: row.view,
      node: row.node,
      editor: row.editor.as_ref().map(|editor| {
        let index = match editor.layer {
          RenderLayer::Atom => 0,
          RenderLayer::Polymer => 1,
          RenderLayer::Surface => 2,
        };
        let enabled = match editor.layer {
          RenderLayer::Atom => editor.layers.atom_style().is_some(),
          RenderLayer::Polymer => editor.layers.polymer_style().is_some(),
          RenderLayer::Surface => editor.layers.surface_style().is_some(),
        };
        (
          enabled
            && editor.layers.appearances()[index].scheme == chitin_molecule_renderer::appearance::ColorScheme::Uniform,
          editor.layer == RenderLayer::Surface && editor.backend.is_some() && editor.controls.advanced_open(),
        )
      }),
    })
    .collect();
  let focus = rows
    .iter()
    .map(|row| row.editor.as_ref().map(|editor| editor.controls.focus_handle()))
    .collect::<Vec<_>>();
  app.entries_sidebar.sync_layout(layout, &focus);
  let list = list(app.entries_sidebar.list.clone(), move |index, window, cx| {
    render_entry_item(rows[index].clone(), theme, weak.clone(), window, cx).into_any_element()
  })
  .flex_1()
  .min_h(px(120.0));
  let active = app.entries_sidebar.active;
  let submit = cx.weak_entity();
  let save = cx.weak_entity();
  let clear = cx.weak_entity();
  let tree = div()
    .flex()
    .flex_col()
    .flex_1()
    .min_h(px(120.0))
    .child(heading(format!("Open entries · {count}"), theme).py_2())
    .child(list);
  let mut content = div()
    .id("entries-inspector")
    .flex()
    .flex_col()
    .flex_1()
    .min_h_0()
    .overflow_y_scroll()
    .child(
      div()
        .flex_none()
        .border_t_1()
        .border_color(theme.border)
        .px_3()
        .py_2()
        .gap_2()
        .flex()
        .flex_col()
        .child(heading("Selections", theme))
        .child(Input::new(&controls.query).small().disabled(active.is_none()))
        .child(
          div()
            .flex()
            .gap_1()
            .child(
              Button::new("entries-select")
                .ghost()
                .small()
                .label("Select")
                .disabled(active.is_none())
                .on_click(move |_, _, cx| {
                  let _ = submit.update(cx, |app, cx| app.submit_entries_selection(cx));
                }),
            )
            .child(
              Button::new("entries-clear")
                .ghost()
                .small()
                .label("Clear")
                .disabled(active.is_none())
                .on_click(move |_, _, cx| {
                  let _ = clear.update(cx, |app, cx| {
                    app.entries_selection(SelectionCommand::Clear, cx);
                  });
                }),
            ),
        )
        .child(Input::new(&controls.name).small().disabled(active.is_none()))
        .child(
          Button::new("entries-save")
            .ghost()
            .small()
            .label("Save current selection")
            .disabled(active.is_none())
            .on_click(move |_, _, cx| {
              let _ = save.update(cx, |app, cx| app.save_entries_selection(cx));
            }),
        ),
    );
  if let Some(error) = &app.entries_sidebar.error {
    content = content.child(div().px_3().text_xs().text_color(theme.danger).child(error.clone()));
  }
  if let Some(id) = active {
    if let Some(state) = app.entries_sidebar.entries.get(&id) {
      content = content.child(render_saved(id, state, theme, cx.weak_entity()));
    }
    if let Some(state) = app.entries_sidebar.entries.get(&id) {
      let (atoms, residues, chains) = state.selection_counts;
      content = content.child(
        div()
          .px_3()
          .pb_2()
          .text_xs()
          .text_color(theme.foreground)
          .child(format!(
            "Selected: {atoms} atoms · {residues} residues · {chains} chains"
          )),
      );
    }
  }
  if let Some(id) = active
    && let Some(state) = app.entries_sidebar.entries.get(&id)
  {
    let body = properties(state);
    content = content.child(
      div()
        .flex_none()
        .border_t_1()
        .border_color(theme.border)
        .px_3()
        .py_2()
        .child(heading("Properties", theme))
        .child(body),
    );
  } else {
    content = content.child(
      div()
        .flex_none()
        .border_t_1()
        .border_color(theme.border)
        .px_3()
        .py_2()
        .child(heading("Properties", theme))
        .child(
          div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child("Select an entry to inspect its properties."),
        ),
    );
  }
  div().flex().flex_col().flex_1().min_h_0().child(tree).child(content)
}

/// Builds the shared lightweight section heading for Entries controls.
fn heading(title: impl Into<gpui::SharedString>, theme: ThemeColor) -> Div {
  div()
    .flex_none()
    .px_2()
    .py_1()
    .text_xs()
    .font_weight(FontWeight::SEMIBOLD)
    .text_color(theme.foreground)
    .child(title.into())
}

/// Projects each layer's current style into its entry tree label.
fn representation_label(layer: RenderLayer, layers: RepresentationLayers) -> String {
  let style = match layer {
    RenderLayer::Atom => match layers.atom_style() {
      Some(AtomStyle::Stick) => "Stick",
      Some(AtomStyle::BallAndStick) => "Ball and stick",
      Some(AtomStyle::Sphere) => "Space filling",
      None => "None",
    },
    RenderLayer::Polymer => {
      if layers.polymer_style().is_some() {
        "Cartoon"
      } else {
        "None"
      }
    }
    RenderLayer::Surface => {
      if layers.surface_style().is_some() {
        "Solid"
      } else {
        "None"
      }
    }
  };
  format!("{layer:?} · {style}")
}

/// Builds an entry-wide layer toggle; enabling uses that layer's standard style.
///
/// Appearance and object visibility remain unchanged. Non-default atom styles
/// can be chosen explicitly from the layer's inline editor.
fn representation_visibility(layer: RenderLayer, layers: RepresentationLayers) -> RowVisibility {
  let (enabled, toggle) = match layer {
    RenderLayer::Atom => (
      layers.atom_style().is_some(),
      RenderCommand::AtomStyle(if layers.atom_style().is_some() {
        RenderAtomStyle::None
      } else {
        RenderAtomStyle::Stick
      }),
    ),
    RenderLayer::Polymer => (
      layers.polymer_style().is_some(),
      RenderCommand::PolymerStyle(if layers.polymer_style().is_some() {
        RenderPolymerStyle::None
      } else {
        RenderPolymerStyle::Cartoon
      }),
    ),
    RenderLayer::Surface => (
      layers.surface_style().is_some(),
      RenderCommand::SurfaceStyle(if layers.surface_style().is_some() {
        RenderSurfaceStyle::None
      } else {
        RenderSurfaceStyle::Solid
      }),
    ),
  };
  RowVisibility::Layer { enabled, toggle }
}

/// Renders a measured row block, keeping an expanded editor adjacent to its header.
fn render_entry_item(
  row: TreeRow,
  theme: ThemeColor,
  app: WeakEntity<ChitinApp>,
  window: &mut Window,
  cx: &mut gpui::App,
) -> Div {
  let editor = row.editor.clone();
  let depth = row.depth;
  let id = format!("entry-inline-editor-{}-{}", row.tab, row.node);
  let mut block = div().flex().flex_col().w_full().child(render_row(row, theme, app));
  if let Some(editor) = editor {
    block = block.child(
      div()
        .id(id.clone())
        .debug_selector(move || id.clone())
        .flex_none()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .pl(px(24.0 + depth as f32 * 12.0))
        .pr_3()
        .py_2()
        .track_focus(&editor.controls.focus_handle())
        .child(
          editor
            .controls
            .render_layer(editor.layer, editor.layers, editor.backend, theme, window, cx),
        ),
    );
  }
  block
}

/// Renders expansion, scientific selection, color preview, and visibility actions.
fn render_row(row: TreeRow, theme: ThemeColor, app: WeakEntity<ChitinApp>) -> Div {
  let selector = format!("entry-row-{}-{}", row.tab, row.node);
  let mut line = div()
    .debug_selector(move || selector.clone())
    .flex_none()
    .flex()
    .items_center()
    .h(px(28.0))
    .w_full()
    .pl(px(4.0 + row.depth as f32 * 12.0))
    .pr_1();
  if row.branch {
    let expand = app.clone();
    let id = row.tab;
    let node = row.node;
    let view = row.view;
    let item = row.clone();
    line = line.child(
      Button::new(format!("entry-expand-{id}-{node}"))
        .ghost()
        .size(px(20.0))
        .px_0()
        .accessibility_label(if row.expanded { "Collapse entry" } else { "Expand entry" })
        .icon(Icon::default().path(if row.expanded {
          "icons/tree-expand.svg"
        } else {
          "icons/tree-collapse.svg"
        }))
        .on_click(move |_, window, cx| {
          let _ = expand.update(cx, |app, cx| {
            if matches!(item.kind, NodeKind::Representation(_)) {
              app.select_entry_node(&item, window, cx);
              return;
            }
            if app.document_panels.rendering_view_id(id) != view {
              return;
            }
            if let Some(entry) = app.entries_sidebar.entries.get_mut(&id) {
              if !entry.expanded.remove(&node) {
                entry.expanded.insert(node);
              }
              cx.notify();
            }
          });
        }),
    );
  } else {
    line = line.child(div().w(px(20.0)));
  }
  let select = app.clone();
  let item = row.clone();
  line = line.child(
    Button::new(format!("entry-select-{}-{}", row.tab, row.node))
      .ghost()
      .small()
      .selected(row.selected)
      .flex_1()
      .min_w_0()
      .px_1()
      .accessibility_label(row.label.clone())
      .child(
        div()
          .flex()
          .w_full()
          .min_w_0()
          .items_center()
          .gap_1()
          .text_xs()
          .child(Icon::default().path(row.kind.icon()).with_size(px(14.0)))
          .child(
            div()
              .min_w_0()
              .truncate()
              .font_weight(match row.kind {
                NodeKind::Structure => FontWeight::SEMIBOLD,
                NodeKind::Representations
                | NodeKind::Representation(_)
                | NodeKind::Chains
                | NodeKind::Chain
                | NodeKind::Residues
                | NodeKind::Ligands
                | NodeKind::Waters => FontWeight::MEDIUM,
                NodeKind::Residue => FontWeight::NORMAL,
              })
              .child(row.label.clone()),
          ),
      )
      .on_click(move |_, window, cx| {
        let _ = select.update(cx, |app, cx| app.select_entry_node(&item, window, cx));
      }),
  );
  let mut swatch = div().flex().w(px(12.0)).h(px(12.0)).rounded_sm().overflow_hidden();
  for color in &row.colors {
    swatch = swatch.child(div().flex_1().h_full().bg(gpui::rgb(*color)));
  }
  if row.colors.is_empty() {
    swatch = swatch.border_1().border_color(theme.border);
  }
  if matches!(row.kind, NodeKind::Representation(_)) {
    let inspect = app.clone();
    let item = row.clone();
    line = line.child(
      Button::new(format!("entry-color-{}-{}", row.tab, row.node))
        .ghost()
        .size(px(16.0))
        .px_0()
        .accessibility_label("Inspect layer color")
        .child(swatch)
        .on_click(move |_, window, cx| {
          let _ = inspect.update(cx, |app, cx| app.edit_entry_layer_color(&item, window, cx));
        }),
    );
  } else if row.kind != NodeKind::Representations {
    line = line.child(swatch);
  }
  let (visible, disabled, tooltip) = match &row.visibility {
    RowVisibility::Object { hidden, supported } => {
      let visible = *hidden < row.atoms.len();
      (
        visible,
        !supported || row.atoms.is_empty(),
        if *hidden > 0 && visible {
          "Partially visible — hide all"
        } else if visible {
          "Hide"
        } else {
          "Show"
        },
      )
    }
    RowVisibility::Layer { enabled, .. } => (*enabled, false, if *enabled { "Disable layer" } else { "Enable layer" }),
    RowVisibility::None => return line.child(div().w(px(24.0))),
  };
  let eye = app.clone();
  let id = row.tab;
  let view = row.view;
  let atoms = row.atoms;
  let visibility = row.visibility;
  line.child(
    Button::new(format!("entry-visibility-{id}-{}", row.node))
      .ghost()
      .size(px(24.0))
      .px_0()
      .disabled(disabled)
      .accessibility_label(tooltip)
      .tooltip(tooltip)
      .icon(Icon::default().path(if visible { "icons/eye.svg" } else { "icons/eye-off.svg" }))
      .on_click(move |_, _, cx| {
        let _ = eye.update(cx, |app, cx| {
          if app.document_panels.rendering_view_id(id) != view {
            return;
          }
          let result = match visibility.clone() {
            RowVisibility::Layer { toggle, .. } => app.dispatch_render_command(Some(id), toggle, cx),
            RowVisibility::Object { .. } => app.dispatch_visibility_command(
              Some(id),
              VisibilityCommand {
                target: VisibilityTarget::Atoms(atoms.iter().copied().collect()),
                visible: !visible,
              },
              cx,
            ),
            RowVisibility::None => return,
          };
          app.entries_sidebar.error = result.err().map(|error| error.to_string());
          cx.notify();
        });
      }),
  )
}

/// Renders view-scoped bookmarks; restoration uses the shared selection command bus.
fn render_saved(id: u64, state: &EntryState, theme: ThemeColor, app: WeakEntity<ChitinApp>) -> Div {
  let mut list = div()
    .flex()
    .flex_col()
    .flex_none()
    .max_h(px(140.0))
    .overflow_y_scroll()
    .px_3()
    .child(heading("Saved selections", theme));
  if state.saved.is_empty() {
    return list.child(
      div()
        .text_xs()
        .text_color(theme.secondary_foreground)
        .child("No saved selections"),
    );
  }
  for saved in &state.saved {
    let restore = app.clone();
    let remove = app.clone();
    let atoms = saved.atoms.clone();
    let saved_id = saved.id;
    let view = state.view;
    list = list.child(
      div()
        .flex()
        .items_center()
        .child(
          Button::new(format!("entry-bookmark-{id}-{saved_id}"))
            .ghost()
            .small()
            .flex_1()
            .accessibility_label(saved.name.clone())
            .child(
              div()
                .w_full()
                .text_xs()
                .child(format!("{} · {} atoms", saved.name, atoms.len())),
            )
            .on_click(move |_, _, cx| {
              let _ = restore.update(cx, |app, cx| {
                if app.document_panels.rendering_view_id(id) != Some(view) {
                  return;
                }
                app.entries_sidebar.active = Some(id);
                app.entries_selection(
                  SelectionCommand::Atoms {
                    atoms: atoms.as_ref().clone(),
                    mode: SelectionMode::Replace,
                  },
                  cx,
                );
              });
            }),
        )
        .child(
          Button::new(format!("entry-bookmark-remove-{id}-{saved_id}"))
            .ghost()
            .size(px(22.0))
            .px_0()
            .accessibility_label("Remove saved selection")
            .icon(Icon::default().path("icons/terminal-trash.svg"))
            .on_click(move |_, _, cx| {
              let _ = remove.update(cx, |app, cx| {
                if app.document_panels.rendering_view_id(id) != Some(view) {
                  return;
                }
                if let Some(entry) = app.entries_sidebar.entries.get_mut(&id) {
                  entry.saved.retain(|saved| saved.id != saved_id);
                }
                cx.notify();
              });
            }),
        ),
    );
  }
  list
}

/// Combines inspected-node counts with structure-wide experimental metadata.
fn properties(state: &EntryState) -> Div {
  let node = &state.nodes[state.selected];
  let metadata = state.scene.selection.metadata();
  let residues = node.residue_count;
  let chains = if node.atoms.is_empty() {
    0
  } else if node.chain.is_some() {
    1
  } else {
    state.nodes[1].children.len()
  };
  let fields = [
    ("Scope", node.label.clone()),
    ("Chains", chains.to_string()),
    ("Residues", residues.to_string()),
    ("Atoms", node.atoms.len().to_string()),
    (
      "Resolution",
      metadata
        .resolution
        .map(|value| format!("{value:.2} Å"))
        .unwrap_or_else(|| "Not provided".into()),
    ),
    (
      "Method",
      if metadata.experimental_methods.is_empty() {
        "Not provided".into()
      } else {
        metadata.experimental_methods.join("; ")
      },
    ),
    (
      "Identifier",
      metadata.identifier.clone().unwrap_or_else(|| "Not provided".into()),
    ),
  ];
  let mut panel = div().flex().flex_col().flex_none();
  for (label, value) in fields {
    panel = panel.child(
      div()
        .flex()
        .gap_2()
        .py_1()
        .text_xs()
        .child(
          div()
            .w(px(72.0))
            .flex_none()
            .font_weight(FontWeight::MEDIUM)
            .child(label),
        )
        .child(div().flex_1().min_w_0().child(value)),
    );
  }
  panel
}

/// Encodes the renderer's linear RGB sample as a packed sRGB UI swatch.
fn srgb(color: [f32; 3]) -> u32 {
  let bytes = color.map(|value| {
    let value = if value <= 0.0031308 {
      value * 12.92
    } else {
      1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (value.clamp(0., 1.) * 255.).round() as u32
  });
  (bytes[0] << 16) | (bytes[1] << 8) | bytes[2]
}

impl ChitinApp {
  /// Expands the layer beside its row and opens uniform color editing when applicable.
  ///
  /// Non-uniform schemes expose their local scheme editor instead of silently
  /// switching coloring. Scientific selection and the Properties scope are untouched.
  fn edit_entry_layer_color(&mut self, row: &TreeRow, window: &mut Window, cx: &mut Context<Self>) {
    let NodeKind::Representation(layer) = row.kind else {
      return;
    };
    if self.document_panels.rendering_view_id(row.tab) != row.view {
      return;
    }
    let Some((_, layers, _)) = self.document_panels.rendering_settings(row.tab) else {
      return;
    };
    self.activate_sidebar_entry(row.panel, row.tab_id, window, cx);
    if let Some(state) = self.entries_sidebar.entries.get_mut(&row.tab) {
      state.open_layer = Some(layer);
      let settings = state.settings.get_or_insert_with(|| {
        let settings = MolecularSettingsControls::new(window, cx);
        settings.subscribe(window, cx);
        settings
      });
      settings.bind_entry(Some((row.tab, state.view)));
      let (index, enabled) = match layer {
        RenderLayer::Atom => (0, layers.atom_style().is_some()),
        RenderLayer::Polymer => (1, layers.polymer_style().is_some()),
        RenderLayer::Surface => (2, layers.surface_style().is_some()),
      };
      if enabled && layers.appearances()[index].scheme == chitin_molecule_renderer::appearance::ColorScheme::Uniform {
        settings.open_color(layer, window, cx);
      }
      self.entries_sidebar.active = Some(row.tab);
      cx.notify();
    }
  }

  /// Lazily mounts Entries inputs and retains their Enter/change subscriptions.
  fn entries_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) -> EntriesControls {
    if let Some(controls) = &self.entries_sidebar.controls {
      return controls.clone();
    }
    let controls = EntriesControls {
      query: cx.new(|cx| InputState::new(window, cx).placeholder("/A:10-50")),
      name: cx.new(|cx| InputState::new(window, cx).placeholder("Selection name")),
    };
    let query_subscription = cx.subscribe_in(&controls.query, window, |app, _, event, _, cx| match event {
      InputEvent::PressEnter { .. } => app.submit_entries_selection(cx),
      InputEvent::Change if app.entries_sidebar.error.take().is_some() => cx.notify(),
      _ => {}
    });
    let name_subscription = cx.subscribe_in(&controls.name, window, |app, _, event, _, cx| match event {
      InputEvent::PressEnter { .. } => app.save_entries_selection(cx),
      InputEvent::Change if app.entries_sidebar.error.take().is_some() => cx.notify(),
      _ => {}
    });
    self.entries_sidebar.subscriptions = vec![query_subscription, name_subscription];
    self.entries_sidebar.controls = Some(controls.clone());
    controls
  }

  /// Activates a row's document, inspecting topology or toggling its inline layer editor.
  ///
  /// Stale rendering rows are ignored. Scientific objects update Properties and
  /// select coordinate-bearing atoms. Representation navigation leaves both
  /// scientific selection and the Properties scope unchanged.
  fn select_entry_node(&mut self, row: &TreeRow, window: &mut Window, cx: &mut Context<Self>) {
    if row.view.is_some() && self.document_panels.rendering_view_id(row.tab) != row.view {
      return;
    }
    self.activate_sidebar_entry(row.panel, row.tab_id, window, cx);
    if let NodeKind::Representation(layer) = row.kind {
      if let Some(state) = self.entries_sidebar.entries.get_mut(&row.tab) {
        state.toggle_layer(layer);
        self.entries_sidebar.active = Some(row.tab);
        cx.notify();
      }
      return;
    }
    if row.kind == NodeKind::Representations {
      return;
    }
    if let Some(state) = self.entries_sidebar.entries.get_mut(&row.tab) {
      self.entries_sidebar.active = Some(row.tab);
      state.selected = row.node;
      if !row.atoms.is_empty() {
        self.entries_selection(
          SelectionCommand::Atoms {
            atoms: row.atoms.iter().copied().collect(),
            mode: SelectionMode::Replace,
          },
          cx,
        );
      }
      cx.notify();
    }
  }

  /// Submits a selection command for the active entry and exposes errors inline.
  fn entries_selection(&mut self, command: SelectionCommand, cx: &mut Context<Self>) {
    let result = self.dispatch_selection_command(self.entries_sidebar.active, command, cx);
    self.entries_sidebar.error = result.err().map(|error| error.to_string());
    cx.notify();
  }

  /// Parses the shared atom specification grammar and replaces the active selection.
  ///
  /// Parse failures remain inline and never reach the command bus; successful
  /// commands use the same validation and undo path as built-in shell selection.
  fn submit_entries_selection(&mut self, cx: &mut Context<Self>) {
    let Some(controls) = &self.entries_sidebar.controls else {
      return;
    };
    let text = controls.query.read(cx).value();
    match text.trim().parse() {
      Ok(spec) => self.entries_selection(
        SelectionCommand::Match {
          spec,
          mode: SelectionMode::Replace,
        },
        cx,
      ),
      Err(error) => {
        self.entries_sidebar.error = Some(error);
        cx.notify();
      }
    }
  }

  /// Saves the canonical nonempty selection as an entry-local named bookmark.
  ///
  /// Blank names receive a monotonic default name. Duplicate names and empty
  /// selections report an inline error; bookmarks expire with the rendering view.
  fn save_entries_selection(&mut self, cx: &mut Context<Self>) {
    let Some(id) = self.entries_sidebar.active else {
      return;
    };
    let Some(control) = self.document_panels.selection_control(id) else {
      return;
    };
    if control.atoms.is_empty() {
      self.entries_sidebar.error = Some("Select atoms before saving a selection.".into());
      cx.notify();
      return;
    }
    let name = self
      .entries_sidebar
      .controls
      .as_ref()
      .map(|controls| controls.name.read(cx).value().trim().to_owned())
      .unwrap_or_default();
    let Some(entry) = self.entries_sidebar.entries.get_mut(&id) else {
      return;
    };
    let name = if name.is_empty() {
      format!("Selection {}", entry.next_saved)
    } else {
      name
    };
    if entry.saved.iter().any(|saved| saved.name == name) {
      self.entries_sidebar.error = Some("A selection with this name already exists.".into());
    } else {
      entry.saved.push(SavedSelection {
        id: entry.next_saved,
        name,
        atoms: control.atoms.clone(),
      });
      entry.next_saved += 1;
      self.entries_sidebar.error = None;
    }
    cx.notify();
  }
}
