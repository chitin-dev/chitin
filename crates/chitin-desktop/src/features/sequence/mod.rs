//! Coordinate-bearing polymer sequences for the shared bottom dock.

use std::{
  cell::RefCell,
  collections::{BTreeMap, BTreeSet},
  rc::Rc,
  sync::Arc,
};

use chitin_bio::structure::{AtomId, ChainId, ResidueId, ResidueKind, StructureScene, selection::AtomSelection};
use chitin_command::{SelectionCommand, SelectionMode};
use chitin_ui::{
  widgets::marquee::{Marquee, MarqueeEvent, MarqueeState},
  workbench::WorkbenchStyle,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
  App, AppContext, Bounds, Context, Entity, EntityId, InteractiveElement, IntoElement, ParentElement, Pixels,
  StatefulInteractiveElement, Styled, WeakEntity, div, px,
};
use gpui_kit::component::ElementExt as _;
use gpui_kit::component::{
  Disableable as _, Selectable as _, Sizable as _,
  button::{Button, ButtonVariant, ButtonVariants as _},
  menu::{DropdownMenu as _, PopupMenuItem},
  theme::ThemeColor,
};

use crate::{app::ChitinApp, fonts::TERMINAL_FONT_FAMILY, workbench::documents::DocumentPanelState};

/// Stable identity of the Sequence utility in the overlay dock.
pub(crate) const SEQUENCE_DOCK_ITEM_ID: &str = "sequence";

/// One selectable polymer position in a coordinate-bearing chain.
struct SequenceResidue {
  id: ResidueId,
  name: String,
  number: i32,
  insertion: Option<char>,
  letter: char,
  atoms: Arc<AtomSelection>,
}

/// A chain row uses the source topology order, including insertion codes.
struct SequenceChain {
  id: ChainId,
  label: String,
  residues: Vec<SequenceResidue>,
}

/// Persistent controls and a scene-revision-scoped projection for Sequence.
#[derive(Default)]
pub(crate) struct SequencePanelState {
  wrap: bool,
  chain_filter: Option<ChainId>,
  target: Option<(u64, EntityId)>,
  scene: Option<Arc<StructureScene>>,
  chains: Vec<SequenceChain>,
  selected: Arc<AtomSelection>,
  /// Measured visible cell bounds; clipping masks exclude off-screen residues.
  item_bounds: Rc<RefCell<BTreeMap<ResidueId, Bounds<Pixels>>>>,
  /// Temporary sequence highlight; canonical selection changes only on release.
  preview: Option<AtomSelection>,
  marquee: Option<Entity<MarqueeState>>,
}

impl SequencePanelState {
  /// Refreshes only when the focused rendering view or its scene changes.
  pub(crate) fn sync(&mut self, documents: &DocumentPanelState, cx: &mut App) {
    let target = documents
      .tree
      .leaf(documents.focused_panel_id)
      .and_then(|leaf| leaf.active_tab)
      .map(|tab| tab.value())
      .and_then(|id| {
        let view = documents.rendering_view_id(id)?;
        let control = documents.selection_control(id)?;
        Some((id, view, control))
      });
    let Some((id, view, control)) = target else {
      self.cancel_drag(cx);
      self.target = None;
      self.scene = None;
      self.chains.clear();
      self.selected = Arc::default();
      return;
    };
    let changed = self.target != Some((id, view))
      || self
        .scene
        .as_ref()
        .is_none_or(|scene| !Arc::ptr_eq(scene, &control.scene));
    if changed {
      self.cancel_drag(cx);
      // Dense chain IDs belong to one scene; an old filter can name another chain after a tab switch.
      self.chain_filter = None;
      self.chains = project_chains(&control.scene);
      self.scene = Some(control.scene.clone());
      self.target = Some((id, view));
    }
    self.selected = control.atoms.clone();
  }

  /// Retains the pointer primitive independently of dock visibility.
  pub(crate) fn marquee(&mut self, cx: &mut App) -> Entity<MarqueeState> {
    self
      .marquee
      .get_or_insert_with(|| cx.new(|_| MarqueeState::default()))
      .clone()
  }

  /// Discards preview when hiding the panel or changing layout/filter identity.
  pub(crate) fn cancel_drag(&mut self, cx: &mut App) {
    self.preview = None;
    self.item_bounds.borrow_mut().clear();
    if let Some(marquee) = &self.marquee {
      marquee.update(cx, |state, _| state.cancel());
    }
  }
}

/// Renders the scientific sequence inside the workbench-owned dock surface.
pub(crate) fn render_sequence_panel(
  state: &SequencePanelState,
  theme: ThemeColor,
  app: WeakEntity<ChitinApp>,
  marquee: Entity<MarqueeState>,
) -> impl IntoElement {
  let style = WorkbenchStyle::new(theme);
  let wrap_app = app.clone();
  let filter_app = app.clone();
  let filter_label = state
    .chain_filter
    .and_then(|id| state.chains.iter().find(|chain| chain.id == id))
    .map(|chain| format!("Chain {}", chain.label))
    .unwrap_or_else(|| "All chains".to_owned());
  let choices: Vec<_> = state
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

  // Rebuild geometry during prepaint; old wrap/scroll positions never participate in hit tests.
  state.item_bounds.borrow_mut().clear();
  let mut rows = div().flex().flex_col().gap_2().p_2();
  for chain in state
    .chains
    .iter()
    .filter(|chain| state.chain_filter.is_none_or(|id| id == chain.id))
  {
    rows = rows.child(render_chain(chain, state, theme, app.clone()));
  }
  if state.chains.is_empty() {
    let message = if state.target.is_some() {
      "No polymer sequence in the active structure."
    } else {
      "Open a molecular structure to inspect its sequence."
    };
    rows = rows.child(div().text_sm().text_color(theme.muted_foreground).child(message));
  }
  let target = state.target;
  let selection_app = app;
  let content = Marquee::new(
    marquee,
    div()
      .id("sequence-vertical-scroll")
      .flex_1()
      .min_h_0()
      .overflow_y_scroll()
      .child(rows),
    move |event, _, cx| {
      let _ = selection_app.update(cx, |this, cx| this.sequence_marquee(event, target, cx));
    },
  );
  div()
    .flex()
    .flex_col()
    .flex_1()
    .min_h_0()
    .min_w_0()
    .bg(style.panel_background)
    .child(toolbar)
    .child(content)
}

fn render_chain(
  chain: &SequenceChain,
  state: &SequencePanelState,
  theme: ThemeColor,
  app: WeakEntity<ChitinApp>,
) -> impl IntoElement {
  let style = WorkbenchStyle::new(theme);
  let mut letters = div().flex().items_start();
  if state.wrap {
    letters = letters.flex_wrap();
  } else {
    letters = letters.flex_none();
  }
  for (position, residue) in chain.residues.iter().enumerate() {
    let selection = state.preview.as_ref().unwrap_or(state.selected.as_ref());
    let selected = residue.atoms.iter().any(|atom| selection.contains(atom));
    let atoms = residue.atoms.clone();
    let target = state.target;
    let select_app = app.clone();
    let number = residue.number;
    let insertion = residue.insertion.map(|code| code.to_string()).unwrap_or_default();
    let measure_bounds = state.item_bounds.clone();
    let residue_id = residue.id;
    let cell = Button::new(format!("sequence-{}-{}", chain.id.index(), residue.id.index()))
      .with_variant(ButtonVariant::Ghost)
      .disabled(atoms.is_empty())
      .selected(selected)
      .when(selected, |cell| cell.bg(style.raised_hover_background))
      .tooltip(format!(
        "{} {}{} · chain {}",
        residue.name, number, insertion, chain.label
      ))
      .w(px(20.0))
      .h(px(36.0))
      .px(px(0.0))
      .child(
        div()
          .flex()
          .flex_col()
          .items_center()
          .font_family(TERMINAL_FONT_FAMILY)
          .child(
            div()
              .h(px(14.0))
              .text_size(px(10.0))
              .text_color(theme.muted_foreground)
              .child(if position == 0 || (position + 1) % 10 == 0 {
                (position + 1).to_string()
              } else {
                String::new()
              }),
          )
          .child(div().text_sm().child(residue.letter.to_string())),
      )
      .on_click(move |_, _, cx| {
        let Some((id, view)) = target else { return };
        let _ = select_app.update(cx, |this, cx| {
          if this.document_panels.rendering_view_id(id) == Some(view) {
            let _ = this.dispatch_selection_command(
              Some(id),
              SelectionCommand::Atoms {
                atoms: atoms.as_ref().clone(),
                mode: SelectionMode::Replace,
              },
              cx,
            );
          }
        });
      });
    letters = letters.child(
      div()
        .flex_none()
        .w(px(20.0))
        .h(px(36.0))
        .on_prepaint(move |bounds, window, _| {
          measure_bounds
            .borrow_mut()
            .insert(residue_id, bounds.intersect(&window.content_mask().bounds));
        })
        .child(cell),
    );
  }
  let sequence = if state.wrap {
    div().flex_1().min_w_0().child(letters).into_any_element()
  } else {
    div()
      .id(format!(
        "sequence-horizontal-scroll-{}-{}",
        state.target.map(|(id, _)| id).unwrap_or_default(),
        chain.id.index()
      ))
      .flex_1()
      .min_w_0()
      .overflow_x_scroll()
      .child(letters)
      .into_any_element()
  };
  div()
    .flex()
    .items_start()
    .min_w_0()
    .gap_2()
    .child(
      div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .flex_none()
        .w(px(56.0))
        .h(px(36.0))
        .rounded_md()
        .bg(style.raised_background)
        .child(div().text_sm().child(chain.label.clone()))
        .child(
          div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(chain.residues.len().to_string()),
        ),
    )
    .child(sequence)
}

impl ChitinApp {
  /// Previews screen-space hits and commits one undoable command at pointer release.
  fn sequence_marquee(&mut self, event: MarqueeEvent, target: Option<(u64, EntityId)>, cx: &mut Context<Self>) {
    if matches!(event, MarqueeEvent::Cancel) {
      self.sequence_panel.preview = None;
      cx.notify();
      return;
    }
    let Some((id, view)) = target.filter(|target| Some(*target) == self.sequence_panel.target) else {
      return;
    };
    if self.document_panels.rendering_view_id(id) != Some(view) {
      self.sequence_panel.cancel_drag(cx);
      return;
    }
    let rectangle = match event {
      MarqueeEvent::Preview(rectangle) | MarqueeEvent::Commit(rectangle) => rectangle,
      MarqueeEvent::Cancel => return,
    };
    let hits = self.sequence_panel.item_bounds.borrow();
    let atoms = self
      .sequence_panel
      .chains
      .iter()
      .flat_map(|chain| &chain.residues)
      .filter(|residue| {
        hits
          .get(&residue.id)
          .is_some_and(|bounds| selection_intersects(*bounds, rectangle))
      })
      .flat_map(|residue| residue.atoms.iter().copied())
      .collect();
    drop(hits);
    match event {
      MarqueeEvent::Preview(_) => self.sequence_panel.preview = Some(atoms),
      MarqueeEvent::Commit(_) => {
        self.sequence_panel.preview = None;
        let _ = self.dispatch_selection_command(
          Some(id),
          SelectionCommand::Atoms {
            atoms,
            mode: SelectionMode::Replace,
          },
          cx,
        );
      }
      MarqueeEvent::Cancel => {}
    }
    cx.notify();
  }
}

/// Empty clipped cells never hit; horizontal drags still cross the residue row.
fn selection_intersects(cell: Bounds<Pixels>, rectangle: Bounds<Pixels>) -> bool {
  if cell.size.width <= px(0.0) || cell.size.height <= px(0.0) {
    return false;
  }
  if rectangle.size.width <= px(3.0) && rectangle.size.height <= px(3.0) {
    let point = rectangle.origin;
    return point.x >= cell.left() && point.x < cell.right() && point.y >= cell.top() && point.y < cell.bottom();
  }
  cell.intersects(&Bounds::new(
    rectangle.origin,
    gpui::size(rectangle.size.width.max(px(1.0)), rectangle.size.height.max(px(1.0))),
  ))
}

fn project_chains(scene: &StructureScene) -> Vec<SequenceChain> {
  let mut atoms_by_residue: BTreeMap<ResidueId, Vec<AtomId>> = BTreeMap::new();
  let mut present_chains = BTreeSet::new();
  for atom in &scene.atoms {
    atoms_by_residue.entry(atom.residue_id).or_default().push(atom.atom_id);
    present_chains.insert(atom.chain_id);
  }
  present_chains
    .into_iter()
    .filter_map(|id| {
      let source = scene.selection.chain(id)?;
      let residues: Vec<_> = source
        .residue_ids
        .iter()
        .filter_map(|residue_id| {
          let metadata = scene.selection.residue(*residue_id)?;
          (metadata.kind == ResidueKind::Polymer).then(|| SequenceResidue {
            id: *residue_id,
            name: metadata.name.clone(),
            number: metadata.sequence_number,
            insertion: metadata.insertion_code,
            letter: one_letter(&metadata.name),
            atoms: Arc::new(
              atoms_by_residue
                .remove(residue_id)
                .unwrap_or_default()
                .into_iter()
                .collect(),
            ),
          })
        })
        .collect();
      (!residues.is_empty()).then(|| SequenceChain {
        id,
        label: source
          .auth_id
          .as_ref()
          .or(source.label_id.as_ref())
          .cloned()
          .unwrap_or_else(|| "(blank)".into()),
        residues,
      })
    })
    .collect()
}

fn one_letter(name: &str) -> char {
  match name.to_ascii_uppercase().as_str() {
    "ALA" => 'A',
    "ARG" => 'R',
    "ASN" => 'N',
    "ASP" => 'D',
    "CYS" => 'C',
    "GLN" => 'Q',
    "GLU" => 'E',
    "GLY" => 'G',
    "HIS" => 'H',
    "ILE" => 'I',
    "LEU" => 'L',
    "LYS" => 'K',
    "MET" | "MSE" => 'M',
    "PHE" => 'F',
    "PRO" => 'P',
    "SER" => 'S',
    "THR" => 'T',
    "TRP" => 'W',
    "TYR" => 'Y',
    "VAL" => 'V',
    "SEC" => 'U',
    "PYL" => 'O',
    "DA" | "A" => 'A',
    "DC" | "C" => 'C',
    "DG" | "G" => 'G',
    "DT" | "T" => 'T',
    "U" => 'U',
    _ => 'X',
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_bio::structure::PdbParser;

  #[test]
  fn project_chains_excludes_ligands_and_preserves_author_numbering() {
    let input = concat!(
      "ATOM      1  CA  GLY A  10       0.000   0.000   0.000  1.00 10.00           C  \n",
      "ATOM      2  CA  ALA A  11       1.000   0.000   0.000  1.00 10.00           C  \n",
      "HETATM    3  O   HOH A  12       2.000   0.000   0.000  1.00 10.00           O  \nEND\n",
    );
    let parsed = PdbParser::new()
      .parse_bytes(input.as_bytes())
      .unwrap_or_else(|error| panic!("fixture: {error}"));
    let scene = StructureScene::from_first_model(&parsed.structure).unwrap_or_else(|error| panic!("scene: {error}"));
    let rows = project_chains(&scene);
    assert_eq!(
      rows[0]
        .residues
        .iter()
        .map(|residue| (residue.letter, residue.number))
        .collect::<Vec<_>>(),
      [('G', 10), ('A', 11)]
    );
  }

  #[test]
  fn unknown_polymer_code_uses_x() {
    assert_eq!(one_letter("A2G"), 'X');
  }

  #[test]
  fn horizontal_drag_hits_residues_without_vertical_motion() {
    let cell = Bounds::new(gpui::point(px(20.0), px(40.0)), gpui::size(px(20.0), px(36.0)));
    let rectangle = Bounds::new(gpui::point(px(5.0), px(50.0)), gpui::size(px(60.0), px(0.0)));
    assert!(selection_intersects(cell, rectangle));
  }

  #[test]
  fn fully_clipped_residue_does_not_hit_a_selection_rectangle() {
    let cell = Bounds::new(gpui::point(px(20.0), px(40.0)), gpui::size(px(0.0), px(36.0)));
    let rectangle = Bounds::new(gpui::point(px(5.0), px(5.0)), gpui::size(px(100.0), px(100.0)));
    assert!(!selection_intersects(cell, rectangle));
  }

  #[test]
  fn wrapped_row_hit_testing_does_not_select_the_row_above() {
    let cell = Bounds::new(gpui::point(px(20.0), px(10.0)), gpui::size(px(20.0), px(36.0)));
    let rectangle = Bounds::new(gpui::point(px(5.0), px(50.0)), gpui::size(px(60.0), px(20.0)));
    assert!(!selection_intersects(cell, rectangle));
  }

  #[gpui::test]
  fn previews_leave_canonical_selection_unchanged_and_release_is_one_undoable_edit(cx: &mut gpui::TestAppContext) {
    use crate::workbench::documents::state::{WgpuDocumentView, WgpuDocumentViewFactory};
    use gpui::{Render, Window};

    struct Probe;
    impl Render for Probe {
      fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
      }
    }
    fn view(scene: Arc<StructureScene>, cx: &mut App) -> WgpuDocumentView {
      WgpuDocumentView::with_representation_layers(cx.new(|_| Probe), Default::default(), |_, _| {}).with_selection(
        scene,
        |_, _| {},
        |_, _| {},
      )
    }
    cx.update(chitin_ui::init);
    let input = concat!(
      "ATOM      1  CA  GLY A  10       0.000   0.000   0.000  1.00 10.00           C  \n",
      "ATOM      2  CA  ALA A  11       1.000   0.000   0.000  1.00 10.00           C  \nEND\n",
    );
    let parsed = PdbParser::new()
      .parse_bytes(input.as_bytes())
      .unwrap_or_else(|error| panic!("fixture: {error}"));
    let scene =
      Arc::new(StructureScene::from_first_model(&parsed.structure).unwrap_or_else(|error| panic!("scene: {error}")));
    let (app, cx) = cx.add_window_view(move |_, cx| {
      ChitinApp::new_with_wgpu_document_panel(
        Some("/tmp".into()),
        cx.focus_handle(),
        "sequence.pdb",
        view(scene.clone(), cx),
        WgpuDocumentViewFactory::new(move |_, cx| view(scene.clone(), cx)),
      )
    });
    cx.update(|_, cx| {
      app.update(cx, |app, cx| {
        app.sequence_panel.sync(&app.document_panels, cx);
        let target = app.sequence_panel.target;
        for (index, residue) in app.sequence_panel.chains[0].residues.iter().enumerate() {
          app.sequence_panel.item_bounds.borrow_mut().insert(
            residue.id,
            Bounds::new(
              gpui::point(px(index as f32 * 20.0), px(0.0)),
              gpui::size(px(20.0), px(36.0)),
            ),
          );
        }
        let rectangle = Bounds::new(gpui::point(px(0.0), px(5.0)), gpui::size(px(40.0), px(10.0)));
        app.sequence_marquee(MarqueeEvent::Preview(rectangle), target, cx);
        app.sequence_marquee(MarqueeEvent::Preview(rectangle), target, cx);
        assert!(
          app
            .document_panels
            .selection_control(1)
            .is_some_and(|control| control.atoms.is_empty())
        );
        app.sequence_marquee(MarqueeEvent::Commit(rectangle), target, cx);
        assert_eq!(
          app
            .document_panels
            .selection_control(1)
            .map(|control| control.atoms.len()),
          Some(2)
        );
        assert!(app.dispatch_history_command(false, cx).is_ok());
        assert!(
          app
            .document_panels
            .selection_control(1)
            .is_some_and(|control| control.atoms.is_empty())
        );
      })
    });
  }
}
