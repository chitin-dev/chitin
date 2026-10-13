//! Coordinate-bearing polymer sequences for the shared bottom dock.
//!
//! The app retains document snapshots and toolbar preferences. The independent
//! viewport owns scrolling, virtualization and temporary pointer selection.

mod model;
mod viewport;

use std::{collections::HashSet, rc::Rc, sync::Arc};

use chitin_bio::structure::{ChainId, ResidueId, StructureScene, selection::AtomSelection};
use chitin_ui::workbench::WorkbenchStyle;
use gpui::{App, AppContext, Entity, EntityId, IntoElement, ParentElement, Styled, WeakEntity, div, px};
use gpui_kit::component::{
  Selectable as _, Sizable as _,
  button::{Button, ButtonVariant, ButtonVariants as _},
  menu::{DropdownMenu as _, PopupMenuItem},
  theme::ThemeColor,
};

use crate::{app::ChitinApp, workbench::documents::DocumentPanelState};
use model::SequenceData;
pub(crate) use viewport::SequenceViewport;

/// Stable identity of the Sequence utility in the overlay dock.
pub(crate) const SEQUENCE_DOCK_ITEM_ID: &str = "sequence";

/// Document snapshots and toolbar preferences, independent of viewport lifetime.
#[derive(Default)]
pub(crate) struct SequencePanelState {
  wrap: bool,
  chain_filter: Option<ChainId>,
  target: Option<(u64, EntityId)>,
  scene: Option<Arc<StructureScene>>,
  data: Rc<SequenceData>,
  selected: Arc<AtomSelection>,
  selected_residues: Rc<HashSet<ResidueId>>,
  viewport: Option<Entity<SequenceViewport>>,
}

impl SequencePanelState {
  /// Rebuilds topology only on scene replacement and selection projection only on change.
  pub(crate) fn sync(&mut self, documents: &DocumentPanelState, cx: &mut App) {
    let target = documents
      .tree
      .leaf(documents.focused_panel_id)
      .and_then(|leaf| leaf.active_tab)
      .map(|tab| tab.value())
      .and_then(|id| Some((id, documents.rendering_view_id(id)?, documents.selection_control(id)?)));
    let Some((id, view, control)) = target else {
      if self.target.take().is_some() {
        self.cancel_drag(cx);
        self.scene = None;
        self.data = Rc::default();
        self.selected = Arc::default();
        self.selected_residues = Rc::default();
      }
      return;
    };
    let changed = self.target != Some((id, view))
      || self
        .scene
        .as_ref()
        .is_none_or(|scene| !Arc::ptr_eq(scene, &control.scene));
    if changed {
      self.cancel_drag(cx);
      self.chain_filter = None;
      self.data = Rc::new(SequenceData::project(&control.scene));
      self.scene = Some(control.scene.clone());
      self.target = Some((id, view));
    }
    if changed || !Arc::ptr_eq(&self.selected, &control.atoms) {
      self.selected_residues = Rc::new(
        control
          .atoms
          .iter()
          .filter_map(|atom| self.data.atom_residues.get(atom).copied())
          .collect(),
      );
      self.selected = control.atoms.clone();
    }
  }

  /// Mounts a retained viewport; pointer previews notify this view, not the workbench.
  pub(crate) fn viewport(&mut self, app: WeakEntity<ChitinApp>, cx: &mut App) -> Entity<SequenceViewport> {
    let viewport = self
      .viewport
      .get_or_insert_with(|| cx.new(|cx| SequenceViewport::new(app, cx)))
      .clone();
    viewport.update(cx, |viewport, cx| {
      viewport.sync(
        self.data.clone(),
        self.selected_residues.clone(),
        self.target,
        self.wrap,
        self.chain_filter,
        cx,
      )
    });
    viewport
  }

  /// Discards a temporary gesture when hiding the dock or changing its toolbar state.
  pub(crate) fn cancel_drag(&mut self, cx: &mut App) {
    if let Some(viewport) = &self.viewport {
      viewport.update(cx, |viewport, cx| viewport.cancel_drag(cx));
    }
  }
}

/// Composes lightweight toolbar controls above the retained sequence viewport.
pub(crate) fn render_sequence_panel(
  state: &SequencePanelState,
  theme: ThemeColor,
  app: WeakEntity<ChitinApp>,
  viewport: Entity<SequenceViewport>,
) -> impl IntoElement {
  let style = WorkbenchStyle::new(theme);
  let wrap_app = app.clone();
  let filter_app = app;
  let filter_label = state
    .chain_filter
    .and_then(|id| state.data.chains.iter().find(|chain| chain.id == id))
    .map(|chain| format!("Chain {}", chain.label))
    .unwrap_or_else(|| "All chains".to_owned());
  let choices: Vec<_> = state
    .data
    .chains
    .iter()
    .map(|chain| (chain.id, chain.label.clone()))
    .collect();
  let current_filter = state.chain_filter;
  let toolbar = div()
    .flex()
    .items_center()
    .gap_2()
    .h(px(34.0))
    .flex_none()
    .px_2()
    .border_b_1()
    .border_color(style.separator)
    .child(
      Button::new("sequence-chain-filter")
        .with_variant(ButtonVariant::Ghost)
        .small()
        .h(px(26.0))
        .child(filter_label)
        .dropdown_menu(move |menu, _, _| {
          let all_app = filter_app.clone();
          let menu = menu.item(
            PopupMenuItem::new("All chains")
              .checked(current_filter.is_none())
              .on_click(move |_, _, cx| {
                let _ = all_app.update(cx, |this, cx| {
                  this.sequence_panel.chain_filter = None;
                  this.sequence_panel.cancel_drag(cx);
                  cx.notify();
                });
              }),
          );
          choices.iter().fold(menu, |menu, (id, label)| {
            let select_app = filter_app.clone();
            let id = *id;
            menu.item(
              PopupMenuItem::new(format!("Chain {label}"))
                .checked(current_filter == Some(id))
                .on_click(move |_, _, cx| {
                  let _ = select_app.update(cx, |this, cx| {
                    this.sequence_panel.chain_filter = Some(id);
                    this.sequence_panel.cancel_drag(cx);
                    cx.notify();
                  });
                }),
            )
          })
        }),
    )
    .child(
      Button::new("sequence-wrap")
        .with_variant(ButtonVariant::Ghost)
        .small()
        .h(px(26.0))
        .selected(state.wrap)
        .child("Wrap")
        .on_click(move |_, _, cx| {
          let _ = wrap_app.update(cx, |this, cx| {
            this.sequence_panel.wrap = !this.sequence_panel.wrap;
            this.sequence_panel.cancel_drag(cx);
            cx.notify();
          });
        }),
    );
  div()
    .flex()
    .flex_col()
    .flex_1()
    .min_h_0()
    .min_w_0()
    .bg(style.panel_background)
    .child(toolbar)
    .child(viewport)
}
