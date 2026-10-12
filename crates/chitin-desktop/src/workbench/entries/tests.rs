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
        4,
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
