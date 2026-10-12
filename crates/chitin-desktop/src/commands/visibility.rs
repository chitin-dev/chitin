//! Panel-scoped object visibility through the shared command bus.

use super::{
  history::{EditPhase, ViewSnapshot},
  render::RenderCommandError,
};
use crate::app::ChitinApp;
use chitin_command::{VisibilityCommand, VisibilityTarget};
use gpui::Context;
use std::sync::Arc;

impl ChitinApp {
  pub(crate) fn dispatch_visibility_command(
    &mut self,
    target: Option<u64>,
    command: VisibilityCommand,
    cx: &mut Context<Self>,
  ) -> Result<String, RenderCommandError> {
    let id = target.ok_or(RenderCommandError::MissingContext)?;
    let before = ViewSnapshot::read(&self.document_panels, id).ok_or(RenderCommandError::Unavailable { id })?;
    let control = self
      .document_panels
      .selection_control(id)
      .ok_or(RenderCommandError::UnsupportedSelection { id })?;
    if !control.supports_visibility() {
      return Err(RenderCommandError::UnsupportedVisibility { id });
    }
    let matched = match command.target {
      VisibilityTarget::Selected => control.atoms.as_ref().clone(),
      VisibilityTarget::All => control.scene.atoms.iter().map(|atom| atom.atom_id).collect(),
      VisibilityTarget::Match(spec) => {
        let matched = control.scene.selection.resolve(&spec, &control.scene);
        if matched.is_empty() {
          return Err(RenderCommandError::NoMatches);
        }
        matched
      }
    };
    let mut hidden = control.hidden.as_ref().clone();
    if command.visible {
      hidden.retain(|atom| !matched.contains(atom));
    } else {
      hidden.extend(matched);
    }
    let count = hidden.len();
    self.document_panels.hide_atoms(id, Arc::new(hidden), cx);
    if let Some(after) = ViewSnapshot::read(&self.document_panels, id) {
      self.edit_history.changed(
        id,
        (chitin_command::CommandId::Visibility, None),
        before,
        after,
        EditPhase::Commit,
      );
    }
    cx.notify();
    Ok(format!("Panel #{id}: {count} hidden atoms"))
  }
}
