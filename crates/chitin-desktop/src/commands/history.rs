//! Workbench timeline of successful presentation and scientific-selection edits.

use crate::{app::ChitinApp, workbench::documents::DocumentPanelState};
use chitin_bio::{structure::selection::AtomSelection, surface::MolecularSurfaceBackend};
use chitin_command::{CommandId, RenderLayer, history::CommandHistory};
use chitin_molecule_renderer::RepresentationLayers;
use gpui::{Context, EntityId};
use std::sync::Arc;

/// Continuous controls preview freely, then commit one gesture on release.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditPhase {
  Commit,
  Preview,
  Finish,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ViewSnapshot {
  view: EntityId,
  layers: RepresentationLayers,
  backend: Option<MolecularSurfaceBackend>,
  selection: Option<Arc<AtomSelection>>,
}
impl ViewSnapshot {
  pub(crate) fn read(state: &DocumentPanelState, target: u64) -> Option<Self> {
    let (_, layers, backend) = state.rendering_settings(target)?;
    Some(Self {
      view: state.rendering_view_id(target)?,
      layers,
      backend,
      selection: state.selection_control(target).map(|control| control.atoms.clone()),
    })
  }
}

#[derive(Clone)]
struct ViewEdit {
  target: u64,
  key: (CommandId, Option<RenderLayer>),
  before: ViewSnapshot,
  after: ViewSnapshot,
}
impl ViewEdit {
  fn bytes(&self) -> usize {
    std::mem::size_of::<Self>()
      + [&self.before, &self.after]
        .iter()
        .map(|snapshot| {
          snapshot
            .selection
            .as_ref()
            .map_or(0, |atoms| atoms.len().saturating_mul(64))
        })
        .sum::<usize>()
  }
}

pub(crate) struct WorkbenchHistory {
  timeline: CommandHistory<ViewEdit>,
  gesture: Option<ViewEdit>,
}
impl Default for WorkbenchHistory {
  fn default() -> Self {
    Self {
      timeline: CommandHistory::new(100, 16 * 1024 * 1024),
      gesture: None,
    }
  }
}
impl WorkbenchHistory {
  fn record(&mut self, edit: ViewEdit) {
    if edit.before != edit.after {
      let bytes = edit.bytes();
      self.timeline.record(edit, bytes);
    }
  }
  pub(crate) fn finish_gesture(&mut self) {
    if let Some(edit) = self.gesture.take() {
      self.record(edit);
    }
  }
  pub(crate) fn changed(
    &mut self,
    target: u64,
    key: (CommandId, Option<RenderLayer>),
    before: ViewSnapshot,
    after: ViewSnapshot,
    phase: EditPhase,
  ) {
    if phase == EditPhase::Commit
      || self
        .gesture
        .as_ref()
        .is_some_and(|edit| edit.target != target || edit.key != key || edit.before.view != before.view)
    {
      self.finish_gesture();
    }
    let edit = if let Some(mut edit) = self.gesture.take() {
      edit.after = after;
      edit
    } else {
      ViewEdit {
        target,
        key,
        before,
        after,
      }
    };
    match phase {
      EditPhase::Preview => self.gesture = Some(edit),
      EditPhase::Commit | EditPhase::Finish => self.record(edit),
    }
  }
  pub(crate) fn prune(&mut self, state: &DocumentPanelState) {
    self
      .timeline
      .retain(|edit| state.rendering_view_id(edit.target) == Some(edit.before.view));
    if self
      .gesture
      .as_ref()
      .is_some_and(|edit| state.rendering_view_id(edit.target) != Some(edit.before.view))
    {
      self.gesture = None;
    }
  }
}

impl ChitinApp {
  /// Restores a committed change without recording the restoration as a new edit.
  pub(crate) fn dispatch_history_command(
    &mut self,
    redo: bool,
    cx: &mut Context<Self>,
  ) -> Result<String, super::render::RenderCommandError> {
    self.edit_history.prune(&self.document_panels);
    self.edit_history.finish_gesture();
    let change = if redo {
      self.edit_history.timeline.redo_change()
    } else {
      self.edit_history.timeline.undo_change()
    }
    .cloned();
    let Some(change) = change else {
      return Ok(if redo { "Nothing to redo" } else { "Nothing to undo" }.into());
    };
    let snapshot = if redo { &change.after } else { &change.before };
    if let Some(backend) = snapshot.backend
      && let Some(callback) = self.document_panels.select_surface_backend(change.target, backend)
    {
      callback(backend, cx);
    }
    if let Some(callback) = self
      .document_panels
      .select_representation_layers(change.target, snapshot.layers)
    {
      callback(snapshot.layers, cx);
    }
    if let Some(atoms) = &snapshot.selection {
      self.document_panels.select_atoms(change.target, atoms.clone(), cx);
    }
    if redo {
      self.edit_history.timeline.did_redo();
    } else {
      self.edit_history.timeline.did_undo();
    }
    cx.notify();
    Ok(format!(
      "{} {} in panel #{}",
      if redo { "Redid" } else { "Undid" },
      change.key.0.as_str(),
      change.target
    ))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::workbench::documents::{
    layout::PanelSplitAxis,
    state::{WgpuDocumentView, WgpuDocumentViewFactory},
  };
  use chitin_bio::structure::{PdbParser, StructureScene};
  use chitin_command::{RenderAtomStyle, RenderCommand, RenderOpacity, SelectionCommand, SelectionMode};
  use chitin_molecule_renderer::AtomStyle;
  use gpui::{App, AppContext, IntoElement, Render, Window};

  struct Probe;
  impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
      gpui::div()
    }
  }

  fn molecular_view(cx: &mut App) -> WgpuDocumentView {
    let Ok(parsed) = PdbParser::new().parse_bytes(b"ATOM      1  CA  GLY A  42       1.000   2.000   3.000  1.00 10.00           C  \nATOM      2  N   GLY A  42       0.000   2.000   3.000  1.00 10.00           N  \nEND\n") else { panic!("fixture must parse"); };
    let Ok(scene) = StructureScene::from_first_model(&parsed.structure) else {
      panic!("fixture must produce scene");
    };
    WgpuDocumentView::with_representation_layers(
      cx.new(|_| Probe),
      RepresentationLayers::atom(AtomStyle::Stick),
      |_, _| {},
    )
    .with_surface_backend(MolecularSurfaceBackend::ImplicitScalarField, |_, _| {})
    .with_selection(Arc::new(scene), |_, _| {}, |_, _| {})
  }

  fn mount(cx: &mut gpui::TestAppContext) -> (gpui::Entity<ChitinApp>, &mut gpui::VisualTestContext) {
    cx.update(chitin_ui::init);
    cx.add_window_view(|_, cx| {
      ChitinApp::new_with_wgpu_document_panel(
        None,
        cx.focus_handle(),
        "fixture.pdb",
        molecular_view(cx),
        WgpuDocumentViewFactory::new(|_, cx| molecular_view(cx)),
      )
    })
  }

  fn selection(app: &ChitinApp, id: u64) -> usize {
    app
      .document_panels
      .selection_control(id)
      .map_or(0, |control| control.atoms.len())
  }

  #[gpui::test]
  fn selection_and_representation_should_share_one_undo_timeline(cx: &mut gpui::TestAppContext) {
    let (app, cx) = mount(cx);
    cx.update(|_, cx| {
      app.update(cx, |app, cx| {
        assert!(
          app
            .dispatch_selection_command(Some(1), SelectionCommand::All, cx)
            .is_ok()
        );
        assert_eq!(selection(app, 1), 2);
        assert!(
          app
            .dispatch_render_command(Some(1), RenderCommand::AtomStyle(RenderAtomStyle::Sphere), cx)
            .is_ok()
        );
        assert!(app.dispatch_history_command(false, cx).is_ok());
        assert_eq!(
          app
            .document_panels
            .rendering_settings(1)
            .map(|(_, layers, _)| layers.atom_style()),
          Some(Some(AtomStyle::Stick))
        );
        assert_eq!(selection(app, 1), 2);
        assert!(app.dispatch_history_command(false, cx).is_ok());
        assert_eq!(selection(app, 1), 0);
        assert!(app.dispatch_history_command(true, cx).is_ok());
        assert_eq!(selection(app, 1), 2);
        assert!(app.dispatch_history_command(true, cx).is_ok());
        assert_eq!(
          app
            .document_panels
            .rendering_settings(1)
            .map(|(_, layers, _)| layers.atom_style()),
          Some(Some(AtomStyle::Sphere))
        );
      })
    });
  }

  #[gpui::test]
  fn failed_and_noop_selections_should_preserve_selection_and_redo(cx: &mut gpui::TestAppContext) {
    let (app, cx) = mount(cx);
    cx.update(|_, cx| {
      app.update(cx, |app, cx| {
        assert!(
          app
            .dispatch_selection_command(Some(1), SelectionCommand::All, cx)
            .is_ok()
        );
        assert!(
          app
            .dispatch_selection_command(Some(1), SelectionCommand::All, cx)
            .is_ok()
        );
        let Ok(spec) = "/B:42".parse() else {
          panic!("valid spec");
        };
        assert!(
          app
            .dispatch_selection_command(
              Some(1),
              SelectionCommand::Match {
                spec,
                mode: SelectionMode::Replace
              },
              cx
            )
            .is_err()
        );
        assert_eq!(selection(app, 1), 2);
        assert!(app.dispatch_history_command(false, cx).is_ok());
        assert_eq!(selection(app, 1), 0, "noop did not create a duplicate edit");
        assert!(
          app
            .dispatch_selection_command(Some(1), SelectionCommand::Clear, cx)
            .is_ok()
        );
        assert!(
          app
            .dispatch_selection_command(Some(1), SelectionCommand::Status, cx)
            .is_ok()
        );
        assert!(app.dispatch_history_command(true, cx).is_ok());
        assert_eq!(selection(app, 1), 2, "noops and queries did not clear redo");
      })
    });
  }

  #[gpui::test]
  fn slider_previews_should_commit_once_and_new_edits_should_clear_redo(cx: &mut gpui::TestAppContext) {
    let (app, cx) = mount(cx);
    cx.update(|_, cx| {
      app.update(cx, |app, cx| {
        for (value, phase) in [
          (0.8, EditPhase::Preview),
          (0.6, EditPhase::Preview),
          (0.4, EditPhase::Finish),
        ] {
          let Ok(value) = RenderOpacity::new(value) else {
            panic!("valid opacity");
          };
          assert!(
            app
              .dispatch_render_edit(
                Some(1),
                RenderCommand::Opacity {
                  layer: RenderLayer::Atom,
                  value
                },
                phase,
                cx
              )
              .is_ok()
          );
        }
        assert!(app.dispatch_history_command(false, cx).is_ok());
        assert_eq!(
          app
            .document_panels
            .rendering_settings(1)
            .map(|(_, layers, _)| layers.appearances()[0].opacity()),
          Some(1.)
        );
        assert!(app.edit_history.timeline.undo_change().is_none());
        assert!(app.dispatch_history_command(true, cx).is_ok());
        assert_eq!(
          app
            .document_panels
            .rendering_settings(1)
            .map(|(_, layers, _)| layers.appearances()[0].opacity()),
          Some(0.4)
        );
        assert!(app.dispatch_history_command(false, cx).is_ok());
        assert!(
          app
            .dispatch_selection_command(Some(1), SelectionCommand::All, cx)
            .is_ok()
        );
        assert!(app.edit_history.timeline.redo_change().is_none());
      })
    });
  }

  #[gpui::test]
  fn split_views_should_copy_selection_then_undo_independently_without_focus_changes(cx: &mut gpui::TestAppContext) {
    let (app, cx) = mount(cx);
    cx.update(|window, cx| {
      app.update(cx, |app, cx| {
        assert!(
          app
            .dispatch_selection_command(Some(1), SelectionCommand::All, cx)
            .is_ok()
        );
        let original_panel = app.document_panels.focused_panel_id;
        let content = app
          .document_panels
          .tree
          .leaf(original_panel)
          .and_then(|leaf| leaf.active_tab())
          .map(|tab| tab.payload.clone_for_split(window, cx));
        let Some(_) = app
          .document_panels
          .split_panel_with_content(original_panel, PanelSplitAxis::Horizontal, content)
        else {
          panic!("must split");
        };
        let panels = app.document_panels.rendering_panels();
        let Some(clone) = panels.iter().find(|panel| panel.id() != 1) else {
          panic!("clone exists");
        };
        let clone_id = clone.id();
        assert_eq!(selection(app, clone_id), 2);
        let focus = app.document_panels.focused_panel_id;
        assert!(
          app
            .dispatch_selection_command(Some(1), SelectionCommand::Clear, cx)
            .is_ok()
        );
        assert_eq!(selection(app, clone_id), 2);
        assert_eq!(selection(app, 1), 0);
        assert!(app.dispatch_history_command(false, cx).is_ok());
        assert_eq!(selection(app, 1), 2);
        assert_eq!(app.document_panels.focused_panel_id, focus);
        let Some((leaf, tab)) = app.document_panels.rendering_panel_location(1) else {
          panic!("original exists");
        };
        assert!(app.document_panels.close_tab(leaf, tab));
        assert_eq!(
          app.dispatch_history_command(true, cx).ok().as_deref(),
          Some("Nothing to redo")
        );
        assert_eq!(selection(app, clone_id), 2);
      })
    });
  }

  #[gpui::test]
  fn replacing_a_view_should_invalidate_history_even_when_tab_id_is_reused(cx: &mut gpui::TestAppContext) {
    let (app, cx) = mount(cx);
    cx.update(|_, cx| {
      app.update(cx, |app, cx| {
        assert!(
          app
            .dispatch_selection_command(Some(1), SelectionCommand::All, cx)
            .is_ok()
        );
        let old = app.document_panels.rendering_view_id(1);
        app.document_panels = DocumentPanelState::with_content(
          crate::workbench::documents::state::DocumentPanelContent::wgpu_interactive(
            None,
            "replacement",
            molecular_view(cx),
            WgpuDocumentViewFactory::new(|_, cx| molecular_view(cx)),
          ),
        );
        assert_ne!(app.document_panels.rendering_view_id(1), old);
        assert_eq!(
          app.dispatch_history_command(false, cx).ok().as_deref(),
          Some("Nothing to undo")
        );
        assert_eq!(selection(app, 1), 0);
      })
    });
  }

  #[gpui::test]
  fn color_and_backend_should_restore_exactly_through_history(cx: &mut gpui::TestAppContext) {
    let (app, cx) = mount(cx);
    cx.update(|_, cx| {
      app.update(cx, |app, cx| {
        let before = ViewSnapshot::read(&app.document_panels, 1);
        assert!(
          app
            .dispatch_render_command(
              Some(1),
              RenderCommand::Color {
                layer: RenderLayer::Atom,
                scheme: chitin_command::RenderColorScheme::Chain,
                value: None
              },
              cx
            )
            .is_ok()
        );
        let colored = ViewSnapshot::read(&app.document_panels, 1);
        assert!(colored != before);
        assert!(
          app
            .dispatch_render_command(
              Some(1),
              RenderCommand::SurfaceBackend(chitin_command::RenderSurfaceBackend::Msms),
              cx
            )
            .is_ok()
        );
        let changed = ViewSnapshot::read(&app.document_panels, 1);
        assert!(changed != colored);
        assert!(app.dispatch_history_command(false, cx).is_ok());
        assert!(ViewSnapshot::read(&app.document_panels, 1) == colored);
        assert!(app.dispatch_history_command(false, cx).is_ok());
        assert!(ViewSnapshot::read(&app.document_panels, 1) == before);
        assert!(app.dispatch_history_command(true, cx).is_ok());
        assert!(app.dispatch_history_command(true, cx).is_ok());
        assert!(ViewSnapshot::read(&app.document_panels, 1) == changed);
      })
    });
  }
}
