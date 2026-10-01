//! Open molecular-view discovery and session-local shell targeting.

use chitin_builtin_shell::RenderingPanelCommand;
use gpui::Context;

use super::shell_host::{DesktopShellDispatch, DesktopShellHost, DesktopShellHostError};
use crate::app::ChitinApp;

impl ChitinApp {
  /// Resolves shell navigation without making UI focus the shell's authority.
  pub(super) fn run_shell_panel_command(
    &mut self,
    host: &DesktopShellHost,
    command: RenderingPanelCommand,
    cx: &mut Context<Self>,
  ) -> Result<DesktopShellDispatch, DesktopShellHostError> {
    let panels = self.document_panels.rendering_panels();
    host.session().reconcile_rendering_panel(&panels)?;
    let output = match command {
      RenderingPanelCommand::List => {
        if panels.is_empty() {
          "No protein rendering panels are open.".to_owned()
        } else {
          let selected = host.session().rendering_panel()?.map(|panel| panel.id());
          let mut output = String::from("  ID       Title\n");
          for panel in &panels {
            let marker = if selected == Some(panel.id()) { '*' } else { ' ' };
            output.push_str(&format!(
              "{marker} {:<8} {}\n",
              panel.id(),
              terminal_label(panel.title())
            ));
          }
          output.push_str("\nEnter a view with: panel enter <ID>");
          output
        }
      }
      RenderingPanelCommand::Enter { id } => {
        let panel = panels
          .into_iter()
          .find(|panel| panel.id() == id)
          .ok_or(DesktopShellHostError::RenderingPanelUnavailable { id })?;
        let (leaf, tab) = self
          .document_panels
          .rendering_panel_location(id)
          .ok_or(DesktopShellHostError::RenderingPanelUnavailable { id })?;
        self.document_panels.activate_tab(leaf, tab);
        let output = format!(
          "Entered rendering panel #{}: {}",
          panel.id(),
          terminal_label(panel.title())
        );
        host.session().set_rendering_panel(Some(panel))?;
        cx.notify();
        output
      }
      RenderingPanelCommand::Leave => {
        host.session().set_rendering_panel(None)?;
        "Left rendering panel context.".to_owned()
      }
    };
    Ok(DesktopShellDispatch::Display { output })
  }
}

/// Prevents filenames from injecting control sequences into terminal output.
pub(crate) fn terminal_label(label: &str) -> String {
  label
    .chars()
    .map(|character| if character.is_control() { ' ' } else { character })
    .collect()
}

#[cfg(test)]
mod tests {
  use std::path::{Path, PathBuf};

  use chitin_builtin_shell::{RenderingPanel, ShellInvocationSource};
  use chitin_molecule_renderer::RepresentationLayers;
  use gpui::{App, AppContext, IntoElement, Render, Window};

  use super::*;
  use crate::workbench::documents::{
    OpenedProjectDocument,
    layout::{PanelSplitAxis, PanelTabDropTarget},
    state::{DocumentPanelContent, WgpuDocumentView, WgpuDocumentViewFactory},
  };

  struct MolecularProbe;

  impl Render for MolecularProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
      gpui::div()
    }
  }

  fn molecular_view(cx: &mut App) -> WgpuDocumentView {
    WgpuDocumentView::with_representation_layers(cx.new(|_| MolecularProbe), RepresentationLayers::empty(), |_, _| {})
  }

  #[gpui::test]
  fn shell_target_should_follow_its_tab_across_docking_and_scope_close(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_kit::init);
    let (app, cx) = cx.add_window_view(|_, cx| {
      ChitinApp::new_with_wgpu_document_panel(
        Some(PathBuf::from("/tmp")),
        cx.focus_handle(),
        "4HHB.cif",
        molecular_view(cx),
        WgpuDocumentViewFactory::new(|_, cx| molecular_view(cx)),
      )
    });
    cx.update(|window, cx| {
      app.update(cx, |app, cx| {
        let host = DesktopShellHost::new(chitin_command::CommandExecutionContext::new("."));
        let root = app.document_panels.focused_panel_id;
        app.open_project_document(OpenedProjectDocument::new(Path::new("/tmp/notes.txt")));
        let content = DocumentPanelContent::wgpu_interactive(
          Some(PathBuf::from("/tmp/1Y7Q.pdb")),
          "1Y7Q.pdb",
          molecular_view(cx),
          WgpuDocumentViewFactory::new(|_, cx| molecular_view(cx)),
        );
        assert!(
          app
            .document_panels
            .open_content_as_tab(Path::new("/tmp/1Y7Q.pdb"), content)
        );
        let Some(second) = app.document_panels.split_panel(root, PanelSplitAxis::Horizontal) else {
          panic!("test panel should split");
        };
        let panels = app.document_panels.rendering_panels();
        assert_eq!(
          panels.len(),
          3,
          "inactive molecular tabs included, generic files excluded"
        );
        let target = panels[0].clone();
        let line = format!("panel enter {}", target.id());
        assert!(
          app
            .submit_shell_line_with_host(&host, line, ShellInvocationSource::Interactive, window, cx)
            .is_ok()
        );
        assert_eq!(host.session().rendering_panel().ok().flatten(), Some(target.clone()));
        let Some((leaf, tab)) = app.document_panels.rendering_panel_location(target.id()) else {
          panic!("selected view should remain open");
        };
        assert_eq!(
          app.document_panels.tree.leaf(leaf).and_then(|leaf| leaf.active_tab),
          Some(tab)
        );
        assert!(app.document_panels.tree.move_tab(
          leaf,
          tab,
          PanelTabDropTarget {
            panel_id: second,
            insertion_index: 0
          }
        ));
        assert_eq!(
          app.document_panels.rendering_panel_location(target.id()),
          Some((second, tab))
        );
        app.document_panels.focused_panel_id = root;
        assert!(
          app
            .submit_shell_line_with_host(&host, "tab.close", ShellInvocationSource::Interactive, window, cx)
            .is_ok()
        );
        assert!(app.document_panels.rendering_panel_location(target.id()).is_none());
        assert_eq!(host.session().rendering_panel().ok().flatten(), None);
        let snapshot = host.session().snapshot();
        assert!(snapshot.is_ok_and(|snapshot| {
          snapshot
            .executions
            .last()
            .and_then(|record| record.rendering_panel.as_ref())
            == Some(&target)
        }));
      });
    });
  }

  #[gpui::test]
  fn unavailable_panel_should_not_replace_context_and_leave_should_not_close_view(cx: &mut gpui::TestAppContext) {
    cx.update(gpui_kit::init);
    let (app, cx) = cx.add_window_view(|_, cx| {
      ChitinApp::new_with_wgpu_document_panel(
        None,
        cx.focus_handle(),
        "4HHB.cif",
        molecular_view(cx),
        WgpuDocumentViewFactory::new(|_, cx| molecular_view(cx)),
      )
    });
    cx.update(|window, cx| {
      app.update(cx, |app, cx| {
        let host = DesktopShellHost::new(chitin_command::CommandExecutionContext::new("."));
        assert!(app.submit_shell_line_with_host(&host, "panel enter 1", ShellInvocationSource::Interactive, window, cx).is_ok());
        let result = app.submit_shell_line_with_host(&host, "panel enter 999", ShellInvocationSource::Interactive, window, cx);
        assert!(matches!(result, Err(DesktopShellHostError::RenderingPanelUnavailable { id: 999 })));
        assert_eq!(host.session().rendering_panel().ok().flatten(), Some(RenderingPanel::new(1, "4HHB.cif")));
        let listed = app.submit_shell_line_with_host(&host, "panel list", ShellInvocationSource::Interactive, window, cx);
        assert!(matches!(listed, Ok(DesktopShellDispatch::Display { output }) if output.contains("4HHB.cif") && output.contains('*')));
        assert!(app.submit_shell_line_with_host(&host, "panel leave", ShellInvocationSource::Interactive, window, cx).is_ok());
        assert_eq!(host.session().rendering_panel().ok().flatten(), None);
        assert_eq!(app.document_panels.rendering_panels().len(), 1);
        assert!(host.session().snapshot().is_ok_and(|snapshot| snapshot.active.is_none()));
      });
    });
  }
}
