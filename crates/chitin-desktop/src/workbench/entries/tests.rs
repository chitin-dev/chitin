use super::*;
use crate::workbench::documents::state::{WgpuDocumentView, WgpuDocumentViewFactory};
use chitin_bio::structure::{PdbParser, StructureScene};
use chitin_molecule_renderer::{AtomStyle, RepresentationLayers};
use gpui::{App, AppContext, Render};
use std::path::PathBuf;

struct Probe;
impl Render for Probe {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    div()
  }
}

fn scene() -> Arc<StructureScene> {
  let input = concat!(
    "EXPDTA    X-RAY DIFFRACTION\nREMARK   2 RESOLUTION.    1.80 ANGSTROMS.\n",
    "ATOM      1  CA  GLY A  10       0.000   0.000   0.000  1.00 10.00           C  \n",
    "ATOM      2  CA  ALA B  42       5.000   0.000   0.000  1.00 10.00           C  \n",
    "HETATM    3  C1  1PE A  99      10.000   0.000   0.000  1.00 10.00           C  \n",
    "HETATM    4  O   HOH B 100      15.000   0.000   0.000  1.00 10.00           O  \nEND\n",
  );
  let parsed = PdbParser::new()
    .parse_bytes(input.as_bytes())
    .unwrap_or_else(|error| panic!("fixture: {error}"));
  Arc::new(StructureScene::from_first_model(&parsed.structure).unwrap_or_else(|error| panic!("scene: {error}")))
}

fn view(scene: Arc<StructureScene>, cx: &mut App) -> WgpuDocumentView {
  WgpuDocumentView::with_representation_layers(
    cx.new(|_| Probe),
    RepresentationLayers::atom(AtomStyle::Stick),
    |_, _| {},
  )
  .with_selection(scene, |_, _| {}, |_, _| {})
  .with_visibility(|_, _| {})
}

fn foreign_atom() -> chitin_bio::structure::AtomId {
  let input = (1..=5)
    .map(|number| {
      format!("ATOM  {number:>5}  CA  GLY A{number:>4}       0.000   0.000   0.000  1.00 10.00           C  \n")
    })
    .collect::<String>();
  let parsed = PdbParser::new()
    .parse_bytes(input.as_bytes())
    .unwrap_or_else(|error| panic!("fixture: {error}"));
  let scene = StructureScene::from_first_model(&parsed.structure).unwrap_or_else(|error| panic!("scene: {error}"));
  scene.atoms[4].atom_id
}

fn mount(cx: &mut gpui::TestAppContext) -> (Entity<ChitinApp>, &mut gpui::VisualTestContext) {
  cx.update(chitin_ui::init);
  let scene = scene();
  cx.add_window_view(move |_, cx| {
    ChitinApp::new_with_wgpu_document_panel(
      Some(PathBuf::from("/tmp")),
      cx.focus_handle(),
      "fixture.pdb",
      view(scene.clone(), cx),
      WgpuDocumentViewFactory::new(move |_, cx| view(scene.clone(), cx)),
    )
  })
}

fn populate(app: &mut ChitinApp) {
  let view = app
    .document_panels
    .rendering_view_id(1)
    .unwrap_or_else(|| panic!("view"));
  let scene = app
    .document_panels
    .selection_control(1)
    .unwrap_or_else(|| panic!("control"))
    .scene
    .clone();
  app.entries_sidebar.entries.insert(1, EntryState::new(view, scene));
  app.entries_sidebar.active = Some(1);
}

#[gpui::test]
fn hierarchy_should_partition_model_atoms_and_cache_scope_counts(cx: &mut gpui::TestAppContext) {
  let (app, cx) = mount(cx);
  cx.update(|_, cx| {
    app.update(cx, |app, _| {
      populate(app);
      let state = &app.entries_sidebar.entries[&1];
      assert_eq!(state.nodes[0].atoms.len(), 4);
      assert_eq!(state.nodes[0].residue_count, 4);
      assert_eq!(state.nodes[1].children.len(), 2);
      for (kind, count) in [(NodeKind::Residues, 2), (NodeKind::Ligands, 1), (NodeKind::Waters, 1)] {
        assert_eq!(
          state
            .nodes
            .iter()
            .filter(|node| node.kind == kind && node.label.starts_with(&format!("{kind:?} ·")))
            .map(|node| node.atoms.len())
            .sum::<usize>(),
          count
        );
      }
      assert_eq!(state.scene.selection.metadata().resolution, Some(1.8));
      assert_eq!(
        state.rows("fixture.pdb").len(),
        8,
        "collapsed residue groups must not generate UI rows"
      );
    })
  });
}

#[gpui::test]
fn selection_query_and_bookmarks_should_share_selection_history(cx: &mut gpui::TestAppContext) {
  let (app, cx) = mount(cx);
  cx.update(|_, cx| {
    app.update(cx, |app, cx| {
      populate(app);
      app.entries_selection(
        SelectionCommand::Match {
          spec: "/A:10".parse().unwrap_or_else(|error| panic!("spec: {error}")),
          mode: SelectionMode::Replace,
        },
        cx,
      );
      assert!(app.entries_sidebar.error.is_none());
      assert_eq!(
        app
          .document_panels
          .selection_control(1)
          .map(|control| control.atoms.len()),
        Some(1)
      );
      app.save_entries_selection(cx);
      let saved = app.entries_sidebar.entries[&1].saved[0].atoms.as_ref().clone();
      app.entries_selection(SelectionCommand::Clear, cx);
      app.entries_selection(
        SelectionCommand::Atoms {
          atoms: saved,
          mode: SelectionMode::Replace,
        },
        cx,
      );
      assert_eq!(
        app
          .document_panels
          .selection_control(1)
          .map(|control| control.atoms.len()),
        Some(1)
      );
      assert!(app.dispatch_history_command(false, cx).is_ok());
      assert_eq!(
        app
          .document_panels
          .selection_control(1)
          .map(|control| control.atoms.len()),
        Some(0)
      );
      assert_eq!(app.entries_sidebar.entries[&1].saved.len(), 1);
    })
  });
}

#[gpui::test]
fn tree_visibility_should_be_undoable_and_reject_foreign_atom_ids(cx: &mut gpui::TestAppContext) {
  let (app, cx) = mount(cx);
  cx.update(|_, cx| {
    app.update(cx, |app, cx| {
      populate(app);
      let atoms = app.entries_sidebar.entries[&1].nodes[0].atoms.iter().copied().collect();
      assert!(
        app
          .dispatch_visibility_command(
            Some(1),
            VisibilityCommand {
              target: VisibilityTarget::Atoms(atoms),
              visible: false
            },
            cx
          )
          .is_ok()
      );
      assert_eq!(
        app
          .document_panels
          .selection_control(1)
          .map(|control| control.hidden.len()),
        Some(4)
      );
      assert!(app.dispatch_history_command(false, cx).is_ok());
      assert_eq!(
        app
          .document_panels
          .selection_control(1)
          .map(|control| control.hidden.len()),
        Some(0)
      );
      assert!(
        app
          .dispatch_visibility_command(
            Some(1),
            VisibilityCommand {
              target: VisibilityTarget::Atoms([foreign_atom()].into()),
              visible: false,
            },
            cx
          )
          .is_err()
      );
    })
  });
}

#[gpui::test]
fn closed_rendering_views_should_release_entry_metadata_and_bookmarks(cx: &mut gpui::TestAppContext) {
  let (app, cx) = mount(cx);
  cx.update(|_, cx| {
    app.update(cx, |app, _| {
      populate(app);
      assert_eq!(app.entries_sidebar.entries.len(), 1);
      app.document_panels.close_focused_tab();
      app.entries_sidebar.prune(&app.document_panels);
      assert!(app.entries_sidebar.entries.is_empty());
      assert!(app.entries_sidebar.active.is_none());
    })
  });
}

#[gpui::test]
fn inspecting_layers_should_preserve_selection_and_keep_toggles_undoable(cx: &mut gpui::TestAppContext) {
  let (app, cx) = mount(cx);
  cx.update(|window, cx| {
    app.update(cx, |app, cx| {
      populate(app);
      app.entries_selection(
        SelectionCommand::Match {
          spec: "/A:10".parse().unwrap_or_else(|error| panic!("spec: {error}")),
          mode: SelectionMode::Replace,
        },
        cx,
      );
      let state = &app.entries_sidebar.entries[&1];
      let node = state
        .nodes
        .iter()
        .position(|node| node.kind == NodeKind::Representation(RenderLayer::Atom))
        .unwrap_or_else(|| panic!("atom representation"));
      let (panel, tab_id) = app.document_panels.tree.tabs_in_order()[0];
      let row = TreeRow {
        tab: 1,
        panel,
        tab_id,
        view: Some(state.view),
        node,
        depth: 2,
        label: "Atom · Stick".into(),
        kind: state.nodes[node].kind,
        atoms: state.nodes[node].atoms.clone(),
        expanded: false,
        branch: false,
        selected: false,
        visibility: representation_visibility(RenderLayer::Atom, RepresentationLayers::atom(AtomStyle::Stick)),
        colors: Vec::new(),
        editor: None,
      };
      app.select_entry_node(&row, window, cx);
      assert_eq!(app.entries_sidebar.entries[&1].selected, 0);
      assert_eq!(app.entries_sidebar.entries[&1].open_layer, Some(RenderLayer::Atom));
      assert_eq!(
        app
          .document_panels
          .selection_control(1)
          .map(|control| control.atoms.len()),
        Some(1)
      );
      let RowVisibility::Layer { toggle, .. } = row.visibility else {
        panic!("layer eye action");
      };
      assert!(app.dispatch_render_command(Some(1), toggle, cx).is_ok());
      assert_eq!(
        app
          .document_panels
          .rendering_settings(1)
          .and_then(|(_, layers, _)| layers.atom_style()),
        None
      );
      assert_eq!(
        app
          .document_panels
          .selection_control(1)
          .map(|control| control.hidden.len()),
        Some(0)
      );
      assert!(app.dispatch_history_command(false, cx).is_ok());
      assert_eq!(
        app
          .document_panels
          .rendering_settings(1)
          .and_then(|(_, layers, _)| layers.atom_style()),
        Some(AtomStyle::Stick)
      );
    });
  });
}

#[gpui::test]
fn representation_nodes_should_have_no_scientific_scope_or_descendants(cx: &mut gpui::TestAppContext) {
  let view = cx.new(|_| Probe);
  let state = EntryState::new(view.entity_id(), scene());
  let representation = state.nodes[0].children[0];
  assert_eq!(state.nodes[representation].kind, NodeKind::Representations);
  assert_eq!(state.nodes[representation].children.len(), 3);
  for index in &state.nodes[representation].children {
    let node = &state.nodes[*index];
    assert!(matches!(node.kind, NodeKind::Representation(_)));
    assert!(node.atoms.is_empty());
    assert!(node.children.is_empty());
    assert_eq!(node.residue_count, 0);
  }
  assert_eq!(state.nodes[0].residue_count, 4);
}

#[gpui::test]
fn layer_expansion_should_be_entry_local_and_preserve_properties_scope(cx: &mut gpui::TestAppContext) {
  let view = cx.new(|_| Probe);
  let mut first = EntryState::new(view.entity_id(), scene());
  let mut second = EntryState::new(view.entity_id(), scene());
  first.selected = 1;
  second.toggle_layer(RenderLayer::Surface);
  first.toggle_layer(RenderLayer::Atom);
  first.toggle_layer(RenderLayer::Polymer);
  assert_eq!(first.open_layer, Some(RenderLayer::Polymer));
  assert_eq!(second.open_layer, Some(RenderLayer::Surface));
  first.toggle_layer(RenderLayer::Polymer);
  assert_eq!(first.open_layer, None);
  assert_eq!(first.selected, 1);
}

struct InlineLayoutProbe {
  owner: Entity<ChitinApp>,
  list: gpui::ListState,
  rows: Vec<TreeRow>,
}

impl Render for InlineLayoutProbe {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    let rows = self.rows.clone();
    let owner = self.owner.downgrade();
    div().flex().flex_col().w(px(320.0)).h(px(600.0)).child(
      list(self.list.clone(), move |index, window, cx| {
        let selector = if index == 0 {
          "inline-test-first"
        } else {
          "inline-test-second"
        };
        render_entry_item(rows[index].clone(), *ThemeColor::dark(), owner.clone(), window, cx)
          .debug_selector(move || selector.into())
          .into_any_element()
      })
      .flex_1(),
    )
  }
}

#[gpui::test]
fn inline_editor_height_should_move_following_rows_and_collapse_without_a_gap(cx: &mut gpui::TestAppContext) {
  cx.update(chitin_ui::init);
  let (probe, cx) = cx.add_window_view(|window, cx| {
    let scene = scene();
    let owner = cx.new(|cx| {
      ChitinApp::new_with_wgpu_document_panel(
        None,
        cx.focus_handle(),
        "fixture",
        view(scene.clone(), cx),
        WgpuDocumentViewFactory::new(move |_, cx| view(scene.clone(), cx)),
      )
    });
    let rows = owner.update(cx, |app, cx| {
      populate(app);
      let controls = MolecularSettingsControls::new(window, cx);
      controls.subscribe(window, cx);
      let state = &app.entries_sidebar.entries[&1];
      controls.bind_entry(Some((1, state.view)));
      let (panel, tab_id) = app.document_panels.tree.tabs_in_order()[0];
      [RenderLayer::Atom, RenderLayer::Polymer]
        .into_iter()
        .enumerate()
        .map(|(index, layer)| {
          let node = state
            .nodes
            .iter()
            .position(|node| node.kind == NodeKind::Representation(layer))
            .unwrap_or_else(|| panic!("representation"));
          let layers = RepresentationLayers::atom(AtomStyle::Stick);
          TreeRow {
            tab: 1,
            panel,
            tab_id,
            view: Some(state.view),
            node,
            depth: 2,
            label: representation_label(layer, layers),
            kind: NodeKind::Representation(layer),
            atoms: Arc::default(),
            expanded: index == 0,
            branch: true,
            selected: index == 0,
            visibility: representation_visibility(layer, layers),
            colors: Vec::new(),
            editor: (index == 0).then(|| InlineLayerEditor {
              layer,
              layers,
              backend: None,
              controls: controls.clone(),
            }),
          }
        })
        .collect::<Vec<_>>()
    });
    let list = gpui::ListState::new(rows.len(), gpui::ListAlignment::Top, px(80.0));
    list.set_smooth_scroll_enabled(false);
    list.splice_focusable(
      0..rows.len(),
      rows
        .iter()
        .map(|row| row.editor.as_ref().map(|editor| editor.controls.focus_handle())),
    );
    InlineLayoutProbe { owner, list, rows }
  });
  cx.update(|window, cx| window.draw(cx).clear());
  let header = cx
    .debug_bounds("inline-test-first")
    .unwrap_or_else(|| panic!("header bounds"));
  let following = cx
    .debug_bounds("inline-test-second")
    .unwrap_or_else(|| panic!("following row"));
  assert!(header.size.height > px(28.0));
  assert!(following.origin.y >= header.origin.y + header.size.height);
  probe.update(cx, |probe, cx| {
    probe.rows[0].editor = None;
    probe.rows[0].expanded = false;
    probe.list.splice_focusable(0..1, [None]);
    cx.notify();
  });
  cx.run_until_parked();
  cx.update(|window, cx| window.draw(cx).clear());
  let first = cx.debug_bounds("inline-test-first").unwrap_or_else(|| panic!("header"));
  let second = cx
    .debug_bounds("inline-test-second")
    .unwrap_or_else(|| panic!("following"));
  assert_eq!(second.origin.y, first.origin.y + first.size.height);
  assert_eq!(first.size.height, px(28.0));
  let viewport = cx.update(|_, cx| probe.read(cx).list.viewport_bounds());
  for delta in [-10000.0, 10000.0] {
    cx.simulate_event(gpui::ScrollWheelEvent {
      position: viewport.center(),
      delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.0), px(delta))),
      ..Default::default()
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    let top = cx
      .debug_bounds("inline-test-first")
      .unwrap_or_else(|| panic!("row must remain visible"));
    assert_eq!(
      top.origin.y, first.origin.y,
      "short lists must not scroll out of the viewport"
    );
  }
}
