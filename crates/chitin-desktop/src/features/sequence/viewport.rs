//! Independently refreshed sequence viewport with vertical and horizontal virtualization.

use std::{
  cell::RefCell,
  collections::{BTreeMap, HashSet},
  rc::Rc,
};

use chitin_bio::structure::{ChainId, ResidueId, selection::AtomSelection};
use chitin_command::{SelectionCommand, SelectionMode};
use chitin_ui::{
  widgets::marquee::{Marquee, MarqueeEvent, MarqueeState},
  workbench::WorkbenchStyle,
};
use gpui::Axis;
use gpui::prelude::FluentBuilder as _;
use gpui::{
  AppContext, Context, Entity, EntityId, InteractiveElement, IntoElement, ParentElement, Pixels, Render, ScrollHandle,
  StatefulInteractiveElement, Styled, UniformListScrollHandle, WeakEntity, Window, div, px, uniform_list,
};
use gpui_kit::base::ScrollableMask;
use gpui_kit::component::{
  Disableable as _, ElementExt as _, Selectable as _, Theme,
  button::{Button, ButtonVariant, ButtonVariants as _},
  theme::ThemeColor,
};

use super::model::{
  CELL_HEIGHT, CELL_WIDTH, LABEL_WIDTH, ROW_GAP, ROW_HEIGHT, ResidueSpan, RowGeometry, SIDE_PADDING, SequenceData,
  SequenceLayout, sequence_width, visible_columns, wrap_columns,
};
use crate::{app::ChitinApp, fonts::TERMINAL_FONT_FAMILY};

/// Sequence rendering and transient selection live outside the root workbench view.
pub(crate) struct SequenceViewport {
  app: WeakEntity<ChitinApp>,
  data: Rc<SequenceData>,
  selected: Rc<HashSet<ResidueId>>,
  target: Option<(u64, EntityId)>,
  wrap: bool,
  filter: Option<ChainId>,
  width: Pixels,
  layout: SequenceLayout,
  vertical: UniformListScrollHandle,
  horizontal: Vec<ScrollHandle>,
  geometry: Rc<RefCell<BTreeMap<usize, RowGeometry>>>,
  preview: Option<Vec<ResidueSpan>>,
  marquee: Entity<MarqueeState>,
  #[cfg(test)]
  render_count: usize,
  #[cfg(test)]
  row_render_count: std::cell::Cell<usize>,
}

impl SequenceViewport {
  pub(super) fn new(app: WeakEntity<ChitinApp>, cx: &mut Context<Self>) -> Self {
    Self {
      app,
      data: Rc::default(),
      selected: Rc::default(),
      target: None,
      wrap: false,
      filter: None,
      width: px(600.0),
      layout: SequenceLayout::default(),
      vertical: UniformListScrollHandle::new(),
      horizontal: Vec::new(),
      geometry: Rc::default(),
      preview: None,
      marquee: cx.new(|_| MarqueeState::default()),
      #[cfg(test)]
      render_count: 0,
      #[cfg(test)]
      row_render_count: std::cell::Cell::new(0),
    }
  }

  /// Snapshot synchronization avoids touching the viewport when the parent merely repaints.
  pub(super) fn sync(
    &mut self,
    data: Rc<SequenceData>,
    selected: Rc<HashSet<ResidueId>>,
    target: Option<(u64, EntityId)>,
    wrap: bool,
    filter: Option<ChainId>,
    cx: &mut Context<Self>,
  ) {
    let data_changed = !Rc::ptr_eq(&self.data, &data);
    let layout_changed = data_changed || self.target != target || self.wrap != wrap || self.filter != filter;
    let selection_changed = !Rc::ptr_eq(&self.selected, &selected);
    if data_changed {
      self.horizontal = data.chains.iter().map(|_| ScrollHandle::new()).collect();
      self.data = data;
    }
    self.target = target;
    self.wrap = wrap;
    self.filter = filter;
    if layout_changed {
      self.cancel_drag(cx);
      self.rebuild_layout();
      self.vertical = UniformListScrollHandle::new();
    }
    if selection_changed {
      self.selected = selected;
    }
    if layout_changed || selection_changed {
      cx.notify();
    }
  }

  fn rebuild_layout(&mut self) {
    let columns = self.wrap.then(|| wrap_columns(self.width));
    self.layout = SequenceLayout::new(
      self
        .data
        .chains
        .iter()
        .enumerate()
        .filter(|(_, chain)| self.filter.is_none_or(|id| id == chain.id))
        .map(|(index, chain)| (index, chain.residues.len())),
      columns,
    );
  }

  pub(super) fn cancel_drag(&mut self, cx: &mut Context<Self>) {
    let changed = self.preview.take().is_some();
    // Retain measured rows across cached frames; the next render replaces
    // them when the viewport actually changes.
    self.marquee.update(cx, |state, _| state.cancel());
    if changed {
      cx.notify();
    }
  }

  fn resize(&mut self, width: Pixels, cx: &mut Context<Self>) {
    if (f32::from(width - self.width)).abs() < 1.0 {
      return;
    }
    let columns_changed = wrap_columns(width) != wrap_columns(self.width);
    self.width = width;
    if self.wrap && columns_changed {
      self.cancel_drag(cx);
      self.rebuild_layout();
    }
    cx.notify();
  }

  /// Only measured visible rows are queried; no per-residue scan or atom expansion.
  fn marquee_event(&mut self, event: MarqueeEvent, cx: &mut Context<Self>) {
    let rectangle = match event {
      MarqueeEvent::Cancel => {
        if self.preview.take().is_some() {
          cx.notify();
        }
        return;
      }
      MarqueeEvent::Preview(rectangle) | MarqueeEvent::Commit(rectangle) => rectangle,
    };
    let candidates: Vec<_> = self
      .geometry
      .borrow()
      .values()
      .filter_map(|row| row.hit(rectangle))
      .collect();
    let spans = self.selectable_spans(candidates);
    match event {
      MarqueeEvent::Preview(_) => {
        if self.preview.as_ref() != Some(&spans) {
          self.preview = Some(spans);
          cx.notify();
        }
      }
      MarqueeEvent::Commit(_) => self.commit(spans, cx),
      MarqueeEvent::Cancel => {}
    }
  }

  /// Atom expansion and the root command bus run once when the gesture is committed.
  fn commit(&mut self, spans: Vec<ResidueSpan>, cx: &mut Context<Self>) {
    self.preview = None;
    let spans = self.selectable_spans(spans);
    let Some((id, view)) = self.target else {
      return;
    };
    let atoms: AtomSelection = spans
      .iter()
      .flat_map(|span| {
        self
          .data
          .chains
          .get(span.chain)
          .and_then(|chain| chain.residues.get(span.range.clone()))
          .into_iter()
          .flatten()
          .flat_map(|residue| residue.atoms.iter().copied())
      })
      .collect();
    if atoms.is_empty() {
      cx.notify();
      return;
    }
    let _ = self.app.update(cx, |app, cx| {
      if app.sequence_panel.target == Some((id, view)) && app.document_panels.rendering_view_id(id) == Some(view) {
        let _ = app.dispatch_selection_command(
          Some(id),
          SelectionCommand::Atoms {
            atoms,
            mode: SelectionMode::Replace,
          },
          cx,
        );
      }
    });
    cx.notify();
  }

  /// Splits hit spans around residues that have no coordinates in the active scene.
  ///
  /// Such residues remain visible for topology context but their disabled cells
  /// must not participate in marquee previews or selection commands.
  fn selectable_spans(&self, spans: impl IntoIterator<Item = ResidueSpan>) -> Vec<ResidueSpan> {
    let mut selectable = Vec::new();
    for span in spans {
      let Some(chain) = self.data.chains.get(span.chain) else {
        continue;
      };
      let end = span.range.end;
      let mut start = None;
      for position in span.range {
        let has_atoms = chain
          .residues
          .get(position)
          .is_some_and(|residue| !residue.atoms.is_empty());
        if has_atoms {
          start.get_or_insert(position);
        } else if let Some(start) = start.take() {
          selectable.push(ResidueSpan {
            chain: span.chain,
            range: start..position,
          });
        }
      }
      if let Some(start) = start {
        selectable.push(ResidueSpan {
          chain: span.chain,
          range: start..chain.residues.len().min(end),
        });
      }
    }
    selectable
  }

  fn render_row(&self, index: usize, theme: ThemeColor, cx: &Context<Self>) -> gpui::AnyElement {
    #[cfg(test)]
    self.row_render_count.set(self.row_render_count.get() + 1);
    let Some(span) = self.layout.row(index) else {
      return div().h(px(ROW_HEIGHT)).into_any_element();
    };
    let chain = &self.data.chains[span.chain];
    let handle = &self.horizontal[span.chain];
    let mounted = if self.wrap {
      span.range.clone()
    } else {
      visible_columns(chain.residues.len(), handle.offset().x, sequence_width(self.width))
    };
    let style = WorkbenchStyle::new(theme);
    let strip_width = px(span.range.len() as f32 * CELL_WIDTH);
    let mut letters = div().flex().flex_none().w(strip_width).h(px(CELL_HEIGHT));
    if !self.wrap {
      letters = letters.child(div().flex_none().w(px(mounted.start as f32 * CELL_WIDTH)));
    }
    for position in mounted.clone() {
      let residue = &chain.residues[position];
      let selected = match &self.preview {
        Some(spans) => spans
          .iter()
          .any(|selected_span| selected_span.chain == span.chain && selected_span.range.contains(&position)),
        None => self.selected.contains(&residue.id),
      };
      let select_view = cx.weak_entity();
      let selection = ResidueSpan {
        chain: span.chain,
        range: position..position + 1,
      };
      letters = letters.child(
        Button::new(format!("sequence-{}-{}", chain.id.index(), residue.id.index()))
          .with_variant(ButtonVariant::Ghost)
          .disabled(residue.atoms.is_empty())
          .selected(selected)
          .when(selected, |cell| cell.bg(style.raised_hover_background))
          .tooltip(residue.tooltip.clone())
          .w(px(CELL_WIDTH))
          .h(px(CELL_HEIGHT))
          .flex_none()
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
            let _ = select_view.update(cx, |view, cx| view.commit(vec![selection.clone()], cx));
          }),
      );
    }
    if !self.wrap {
      letters = letters.child(
        div()
          .flex_none()
          .w(px((chain.residues.len() - mounted.end) as f32 * CELL_WIDTH)),
      );
    }
    let geometry = self.geometry.clone();
    let measured_span = span.clone();
    let letters = letters.on_prepaint(move |bounds, window, _| {
      geometry.borrow_mut().insert(
        index,
        RowGeometry {
          span: measured_span,
          origin: bounds.origin,
          clip: bounds.intersect(&window.content_mask().bounds),
        },
      );
    });
    let sequence = if self.wrap {
      div().flex_1().min_w_0().child(letters).into_any_element()
    } else {
      div()
        .relative()
        .flex_1()
        .min_w_0()
        .child(
          div()
            .id(format!(
              "sequence-horizontal-scroll-{}-{}",
              self.target.map(|(id, _)| id).unwrap_or_default(),
              chain.id.index()
            ))
            .w_full()
            .overflow_hidden()
            .track_scroll(handle)
            .child(letters),
        )
        // Kit consumes horizontal gestures before UniformList can reinterpret
        // their deltas as vertical scrolling; vertical gestures reach the list.
        .child(
          ScrollableMask::new(Axis::Horizontal, handle).id(format!("sequence-horizontal-mask-{}", chain.id.index())),
        )
        .into_any_element()
    };
    let label = if span.range.start == 0 {
      div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .flex_none()
        .w(px(LABEL_WIDTH))
        .h(px(CELL_HEIGHT))
        .rounded_md()
        .bg(style.raised_background)
        .child(div().text_sm().child(chain.label.clone()))
        .child(
          div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(chain.residues.len().to_string()),
        )
        .into_any_element()
    } else {
      div().flex_none().w(px(LABEL_WIDTH)).into_any_element()
    };
    div()
      .flex()
      .items_start()
      .h(px(ROW_HEIGHT))
      .w_full()
      .min_w_0()
      .px(px(SIDE_PADDING))
      .gap(px(ROW_GAP))
      .child(label)
      .child(sequence)
      .into_any_element()
  }

  fn render_rows(&self, range: std::ops::Range<usize>, theme: ThemeColor, cx: &Context<Self>) -> Vec<gpui::AnyElement> {
    // UniformList can update its mounted rows without rendering this view.
    // Drop departed rows before their old screen positions can enter a gesture;
    // prepaint refreshes the retained rows' positions before input dispatch.
    self.geometry.borrow_mut().retain(|index, _| range.contains(index));
    range.map(|index| self.render_row(index, theme, cx)).collect()
  }
}

impl Render for SequenceViewport {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    #[cfg(test)]
    {
      self.render_count += 1;
    }
    // A cache hit keeps the existing hit geometry. A real rebuild replaces
    // measurements as the virtual list prepaints its visible rows.
    self.geometry.borrow_mut().clear();
    let theme = Theme::global(cx).colors;
    let view = cx.entity().clone();
    let content = if self.layout.count == 0 {
      div()
        .p_2()
        .text_sm()
        .text_color(theme.muted_foreground)
        .child(if self.target.is_some() {
          "No polymer sequence in the active structure."
        } else {
          "Open a molecular structure to inspect its sequence."
        })
        .into_any_element()
    } else {
      uniform_list("sequence-virtual-rows", self.layout.count, move |range, _, cx| {
        view.update(cx, |view, cx| view.render_rows(range, theme, cx))
      })
      .track_scroll(&self.vertical)
      .flex_1()
      .min_h_0()
      .min_w_0()
      .into_any_element()
    };
    let event_view = cx.weak_entity();
    let measure_view = cx.weak_entity();
    // Width is observed after layout so wrapped rows can be recalculated.
    div()
      .flex()
      .flex_col()
      .flex_1()
      .size_full()
      .min_h_0()
      .min_w_0()
      .on_prepaint(move |bounds, _, cx| {
        let _ = measure_view.update(cx, |view, cx| view.resize(bounds.size.width, cx));
      })
      .child(Marquee::new(self.marquee.clone(), content, move |event, _, cx| {
        let _ = event_view.update(cx, |view, cx| view.marquee_event(event, cx));
      }))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::workbench::documents::state::{WgpuDocumentView, WgpuDocumentViewFactory};
  use chitin_bio::structure::{PdbParser, StructureScene, StructureSceneOptions};
  use gpui::{MouseButton, VisualTestContext};
  use std::sync::Arc;

  struct Probe;
  impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
      div()
    }
  }

  fn view(scene: Arc<StructureScene>, cx: &mut gpui::App) -> WgpuDocumentView {
    WgpuDocumentView::with_representation_layers(cx.new(|_| Probe), Default::default(), |_, _| {}).with_selection(
      scene,
      |_, _| {},
      |_, _| {},
    )
  }

  fn mount(
    cx: &mut gpui::TestAppContext,
    chains: usize,
    residues: usize,
  ) -> (Entity<ChitinApp>, &mut VisualTestContext) {
    cx.update(chitin_ui::init);
    let mut input = String::new();
    for chain in 0..chains {
      for residue in 0..residues {
        let serial = chain * residues + residue + 1;
        let label = char::from(b'A' + chain as u8);
        let number = residue + 1;
        let x = residue as f32 * 4.0;
        let y = chain as f32 * 10.0;
        input.push_str(&format!(
          "ATOM  {serial:>5}  CA  ALA {label}{number:>4}    {x:>8.3}{y:>8.3}{:>8.3}  1.00 10.00           C  \n",
          0.0
        ));
      }
    }
    let parsed = PdbParser::new()
      .parse_bytes(input.as_bytes())
      .unwrap_or_else(|error| panic!("fixture: {error}"));
    let scene = Arc::new(
      StructureScene::from_model_with_options(
        &parsed.structure,
        parsed.structure.models()[0].id,
        StructureSceneOptions {
          infer_missing_bonds: false,
          ..Default::default()
        },
      )
      .unwrap_or_else(|error| panic!("scene: {error}")),
    );
    let (app, cx) = cx.add_window_view(move |_, cx| {
      let mut app = ChitinApp::new_with_wgpu_document_panel(
        Some("/tmp".into()),
        cx.focus_handle(),
        "sequence.pdb",
        view(scene.clone(), cx),
        WgpuDocumentViewFactory::new(move |_, cx| view(scene.clone(), cx)),
      );
      app.project_sidebar_visible = false;
      app.bottom_dock.show(super::super::SEQUENCE_DOCK_ITEM_ID);
      app
    });
    cx.simulate_resize(gpui::size(px(1_000.0), px(720.0)));
    cx.run_until_parked();
    (app, cx)
  }

  fn viewport(app: &Entity<ChitinApp>, cx: &mut VisualTestContext) -> Entity<SequenceViewport> {
    cx.update(|window, cx| {
      window.draw(cx).clear();
      app
        .read(cx)
        .sequence_panel
        .viewport
        .clone()
        .unwrap_or_else(|| panic!("sequence viewport"))
    })
  }

  fn row(viewport: &Entity<SequenceViewport>, index: usize, cx: &mut VisualTestContext) -> RowGeometry {
    cx.update(|window, cx| {
      window.draw(cx).clear();
      let view = viewport.read(cx);
      let geometry = view.geometry.borrow();
      geometry.get(&index).cloned().unwrap_or_else(|| {
        panic!(
          "visible row {index}; keys={:?}; count={}; vertical={:?}; horizontal={:?}; renders={}; rows={}",
          geometry.keys().collect::<Vec<_>>(),
          view.layout.count,
          view.vertical.0.borrow().base_handle.offset(),
          view.horizontal[0].offset(),
          view.render_count,
          view.row_render_count.get()
        )
      })
    })
  }

  #[gpui::test]
  fn static_sequence_should_reuse_rows_during_workbench_frames(cx: &mut gpui::TestAppContext) {
    let (app, cx) = mount(cx, 2, 100);
    let viewport = viewport(&app, cx);
    let _ = row(&viewport, 0, cx);
    let before = cx.update(|_, cx| {
      let view = viewport.read(cx);
      (view.render_count, view.row_render_count.get())
    });
    // Mimic an animated sibling's repeated window frames while Sequence is idle.
    for _ in 0..120 {
      app.update(cx, |_, cx| cx.notify());
      cx.update(|window, cx| window.draw(cx).clear());
    }
    let after = cx.update(|_, cx| {
      let view = viewport.read(cx);
      (view.render_count, view.row_render_count.get())
    });
    assert_eq!(
      after, before,
      "unchanged Sequence must not rebuild views or rows on sibling frames"
    );
  }

  #[gpui::test]
  fn virtual_row_updates_should_exclude_offscreen_residues_from_selection(cx: &mut gpui::TestAppContext) {
    let (app, cx) = mount(cx, 12, 100);
    let viewport = viewport(&app, cx);
    let previous = row(&viewport, 0, cx);
    let point = previous.clip.origin + gpui::point(px(3.0), px(20.0));
    let rectangle = gpui::Bounds::new(point, Default::default());
    let expected = viewport.update(cx, |view, cx| {
      // A scrolled-in row occupies the old row's screen position. Exercise the
      // retained virtual-list callback without rebuilding the enclosing view.
      let mut visible = previous.clone();
      visible.span = view.layout.row(5).unwrap_or_else(|| panic!("virtual row 5"));
      let expected = view.data.chains[5].residues[0].atoms.clone();
      view.geometry.borrow_mut().insert(5, visible);
      let renders = view.render_count;
      let theme = Theme::global(cx).colors;
      drop(view.render_rows(5..6, theme, cx));
      assert_eq!(view.render_count, renders);
      view.marquee_event(MarqueeEvent::Commit(rectangle), cx);
      expected
    });
    cx.update(|_, cx| {
      let control = app.read(cx).document_panels.selection_control(1);
      assert!(control.is_some_and(|control| control.atoms.as_ref() == expected.as_ref()));
    });
  }

  #[gpui::test]
  fn long_chains_mount_visible_rows_and_scroll_to_new_columns(cx: &mut gpui::TestAppContext) {
    let (app, cx) = mount(cx, 12, 500);
    let viewport = viewport(&app, cx);
    let first = row(&viewport, 0, cx);
    cx.update(|_, cx| {
      let view = viewport.read(cx);
      assert!(view.geometry.borrow().len() < view.data.chains.len());
      assert!(visible_columns(500, view.horizontal[0].offset().x, sequence_width(view.width)).len() < 100);
    });
    cx.simulate_event(gpui::ScrollWheelEvent {
      position: first.clip.center(),
      delta: gpui::ScrollDelta::Pixels(gpui::point(px(-400.0), px(0.0))),
      ..Default::default()
    });
    cx.run_until_parked();
    let scrolled = row(&viewport, 0, cx);
    cx.update(|_, cx| {
      let view = viewport.read(cx);
      assert!(view.horizontal[0].offset().x < px(-100.0));
      assert!(visible_columns(500, view.horizontal[0].offset().x, sequence_width(view.width)).start > 0);
    });
    assert!(scrolled.origin.x < first.origin.x);
    assert!(
      scrolled
        .hit(gpui::Bounds::new(scrolled.clip.center(), Default::default()))
        .is_some_and(|span| span.range.start > 10)
    );
  }

  #[gpui::test]
  fn wrapped_drag_previews_residue_spans_and_commits_one_undoable_selection(cx: &mut gpui::TestAppContext) {
    let (app, cx) = mount(cx, 2, 100);
    app.update(cx, |app, cx| {
      app.sequence_panel.wrap = true;
      cx.notify();
    });
    cx.run_until_parked();
    let viewport = viewport(&app, cx);
    let first = row(&viewport, 0, cx);
    let second = row(&viewport, 1, cx);
    assert!(second.span.range.start > 0);
    let start = first.clip.origin + gpui::point(px(3.0), px(20.0));
    let end = second.clip.origin + gpui::point(px(53.0), px(20.0));
    cx.simulate_mouse_move(start, None, Default::default());
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Default::default());
    // The GPUI test platform does not dispatch animation frames. Drive the
    // coalesced frame preview explicitly; release still uses native mouse input.
    viewport.update(cx, |view, cx| {
      view.marquee_event(MarqueeEvent::Preview(gpui::Bounds::from_corners(start, end)), cx)
    });
    cx.update(|_, cx| {
      assert!(
        app
          .read(cx)
          .document_panels
          .selection_control(1)
          .is_some_and(|control| control.atoms.is_empty())
      );
      assert!(
        viewport.read(cx).preview.as_ref().is_some_and(|spans| spans.len() == 2),
        "preview={:?}; rows={:?}; start={start:?}; end={end:?}",
        viewport.read(cx).preview,
        viewport.read(cx).geometry.borrow().keys().collect::<Vec<_>>()
      );
    });
    cx.simulate_mouse_up(end, MouseButton::Left, Default::default());
    cx.update(|_, cx| {
      app.update(cx, |app, cx| {
        assert!(
          app
            .document_panels
            .selection_control(1)
            .is_some_and(|control| control.atoms.len() > 1)
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
