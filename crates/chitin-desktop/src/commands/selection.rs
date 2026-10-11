//! Shared selection bus; inputs resolve to IDs before mutating one view.

use super::{
  history::{EditPhase, ViewSnapshot},
  render::RenderCommandError,
};
use crate::app::ChitinApp;
use chitin_bio::structure::selection::AtomSelection;
use chitin_command::{SelectionCommand, SelectionMode};
use gpui::Context;
use std::sync::Arc;

impl ChitinApp {
  pub(crate) fn dispatch_selection_command(
    &mut self,
    target: Option<u64>,
    command: SelectionCommand,
    cx: &mut Context<Self>,
  ) -> Result<String, RenderCommandError> {
    let id = target.ok_or(RenderCommandError::MissingContext)?;
    let before = ViewSnapshot::read(&self.document_panels, id).ok_or(RenderCommandError::Unavailable { id })?;
    let control = self
      .document_panels
      .selection_control(id)
      .ok_or(RenderCommandError::UnsupportedSelection { id })?;
    let scene = control.scene.clone();
    let status = command == SelectionCommand::Status;
    let selected = match command {
      SelectionCommand::Status => control.atoms.as_ref().clone(),
      SelectionCommand::Clear => AtomSelection::new(),
      SelectionCommand::All => scene.atoms.iter().map(|atom| atom.atom_id).collect(),
      SelectionCommand::Match { spec, mode } => {
        let matches = scene.selection.resolve(&spec, &scene);
        if matches.is_empty() {
          return Err(RenderCommandError::NoMatches);
        }
        apply_mode(&control.atoms, matches, mode)
      }
      SelectionCommand::Atoms { atoms, mode } => {
        let valid = scene.atoms.iter().map(|atom| atom.atom_id).collect::<AtomSelection>();
        if !atoms.is_subset(&valid) {
          return Err(RenderCommandError::UnsupportedSelection { id });
        }
        apply_mode(&control.atoms, atoms, mode)
      }
    };
    let (atoms, residues, chains) = scene.selection.counts(&scene, &selected);
    if !status {
      self.document_panels.select_atoms(id, Arc::new(selected), cx);
      if let Some(after) = ViewSnapshot::read(&self.document_panels, id) {
        self.edit_history.changed(
          id,
          (chitin_command::CommandId::Selection, None),
          before,
          after,
          EditPhase::Commit,
        );
      }
      cx.notify();
    }
    Ok(format!(
      "Panel #{id}: selected {atoms} atoms, {residues} residues, {chains} chains"
    ))
  }
}

fn apply_mode(current: &AtomSelection, matched: AtomSelection, mode: SelectionMode) -> AtomSelection {
  mode.apply(current, matched)
}
