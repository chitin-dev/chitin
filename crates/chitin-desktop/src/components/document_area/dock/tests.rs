//! Headless workbench and pure docking-projection regression tests.

use super::super::state::OpenedProjectDocument;
use super::super::state::{WgpuDocumentView, WgpuDocumentViewFactory};
use super::*;
use gpui_kit::component::ElementExt as _;
use std::path::Path;
use std::{cell::Cell, rc::Rc};

/// Reproduces the GPU viewport's flex sizing without requiring a GPU device.
struct ViewportLayoutProbe(Rc<Cell<Size<gpui::Pixels>>>);

impl Render for ViewportLayoutProbe {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    let measured = self.0.clone();
    div()
      .relative()
      .flex_1()
      .min_h_0()
      .overflow_hidden()
      .child(div().absolute().inset_0())
      .on_prepaint(move |bounds, _, _| measured.set(bounds.size))
  }
}

#[gpui::test]
fn gpu_viewport_fills_the_document_body(cx: &mut gpui::TestAppContext) {
  let measured = Rc::new(Cell::new(Size {
    width: px(0.0),
    height: px(0.0),
  }));
  let view = cx.new(|_| ViewportLayoutProbe(measured.clone()));
  let clone_view = view.clone();
  let content = DocumentPanelContent::wgpu_interactive(
    None,
    "GPU viewport",
    WgpuDocumentView::new(view),
    WgpuDocumentViewFactory::new(move |_, _| WgpuDocumentView::new(clone_view.clone())),
  );
  let (_, _) = mount(DocumentPanelState::with_content(content), cx);
  let size = measured.get();
  assert!(
    size.width > px(100.0) && size.height > px(100.0),
    "viewport collapsed: {size:?}"
  );
}

fn sources(count: u64) -> HashMap<PanelTabId, (PanelId, PanelTab<DocumentPanelContent>)> {
  (1..=count)
    .map(|id| {
      let tab_id = PanelTabId::new(id);
      let title = format!("document-{id}.cif");
      let content = DocumentPanelContent::project(OpenedProjectDocument::new(Path::new(&title)));
      (
        tab_id,
        (DEFAULT_DOCUMENT_PANEL_ID, PanelTab::new(tab_id, title, content)),
      )
    })
    .collect()
}

fn tabs(ids: &[u64], active: usize) -> PanelState {
  PanelState {
    panel_name: "TabPanel".into(),
    children: ids
      .iter()
      .map(|id| PanelState {
        panel_name: "chitin.document".to_string(),
        info: PanelInfo::panel((*id).into()),
        ..PanelState::default()
      })
      .collect(),
    info: PanelInfo::tabs(active),
  }
}

fn project(
  layout: &PanelState,
  sources: &HashMap<PanelTabId, (PanelId, PanelTab<DocumentPanelContent>)>,
) -> PanelTree<DocumentPanelContent> {
  PanelTree {
    root: project_layout(layout, sources, &mut PanelId::new(2), &mut HashSet::new()),
  }
}

#[test]
fn reordered_tabs_keep_identity_and_content() {
  let sources = sources(3);
  let tree = project(&tabs(&[3, 1, 2], 2), &sources);
  let Some(leaf) = tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
    panic!("existing leaf must survive")
  };
  assert_eq!(
    leaf.tabs.iter().map(|tab| tab.id.value()).collect::<Vec<_>>(),
    [3, 1, 2]
  );
  assert_eq!(leaf.active_tab, Some(PanelTabId::new(2)));
  assert_eq!(leaf.tabs[0].payload, sources[&PanelTabId::new(3)].1.payload);
}

#[test]
fn closed_document_does_not_survive_in_the_projection() {
  let tree = project(&tabs(&[2], 0), &sources(2));
  assert_eq!(tree.tabs_in_order(), [(DEFAULT_DOCUMENT_PANEL_ID, PanelTabId::new(2))]);
}

#[test]
fn closing_last_document_restores_canonical_empty_center() {
  let tree = project(&tabs(&[], 0), &sources(1));
  assert_eq!(tree, PanelTree::single_leaf(PanelLeaf::new(DEFAULT_DOCUMENT_PANEL_ID)));
}

#[test]
fn drag_to_split_allocates_unique_leaf_ids() {
  let layout = PanelState {
    panel_name: "StackPanel".into(),
    children: vec![tabs(&[1], 0), tabs(&[2], 0)],
    info: PanelInfo::stack(vec![px(200.0), px(600.0)], gpui::Axis::Horizontal),
  };
  let tree = project(&layout, &sources(2));
  assert_eq!(tree.leaf_count(), 2);
  assert_eq!(
    tree.tabs_in_order(),
    [
      (PanelId::new(1), PanelTabId::new(1)),
      (PanelId::new(2), PanelTabId::new(2))
    ]
  );
  let PanelNode::Split(split) = &tree.root else {
    panic!("split must remain")
  };
  assert_eq!(split.ratio, 0.25);
}

#[test]
fn consecutive_layout_events_keep_new_group_identity() {
  let layout = PanelState {
    panel_name: "StackPanel".into(),
    children: vec![tabs(&[1], 0), tabs(&[2], 0)],
    info: PanelInfo::stack(vec![px(200.0), px(600.0)], gpui::Axis::Vertical),
  };
  let tree = project(&layout, &sources(2));
  let mut entries = Vec::new();
  collect_tabs(&tree.root, &mut entries);
  let sources = entries
    .into_iter()
    .map(|(panel, tab)| (tab.id, (panel, tab.clone())))
    .collect();
  assert_eq!(project(&layout, &sources), tree);
}

#[test]
fn n_ary_split_keeps_shares_smaller_than_command_resize_clamp() {
  let layout = PanelState {
    panel_name: "StackPanel".into(),
    children: (1..=12).map(|id| tabs(&[id], 0)).collect(),
    info: PanelInfo::stack(vec![px(100.0); 12], gpui::Axis::Horizontal),
  };
  let tree = project(&layout, &sources(12));
  let PanelNode::Split(split) = &tree.root else {
    panic!("split must remain")
  };
  assert!((split.ratio - 1.0 / 12.0).abs() < 1e-6);
  let PanelNode::Split(second) = split.second.as_ref() else {
    panic!("remaining siblings must remain")
  };
  assert!((second.ratio - 1.0 / 11.0).abs() < 1e-6);
  assert_eq!(tree.leaf_count(), 12);
}

#[test]
fn missing_or_invalid_slot_sizes_produce_finite_ratios() {
  let layout = PanelState {
    panel_name: "StackPanel".into(),
    children: vec![tabs(&[1], 0), tabs(&[2], 0), tabs(&[3], 0)],
    info: PanelInfo::stack(vec![px(f32::NAN), px(0.0)], gpui::Axis::Horizontal),
  };
  let tree = project(&layout, &sources(3));
  let PanelNode::Split(split) = tree.root else {
    panic!("split must remain")
  };
  assert!((split.ratio - 1.0 / 3.0).abs() < 1e-6);
}

#[test]
fn nested_splits_preserve_axes_and_selection() {
  let nested = PanelState {
    panel_name: "StackPanel".into(),
    children: vec![tabs(&[2], 0), tabs(&[3, 4], 1)],
    info: PanelInfo::stack(vec![px(400.0), px(100.0)], gpui::Axis::Vertical),
  };
  let layout = PanelState {
    panel_name: "StackPanel".into(),
    children: vec![tabs(&[1], 0), nested],
    info: PanelInfo::stack(vec![px(300.0), px(300.0)], gpui::Axis::Horizontal),
  };
  let tree = project(&layout, &sources(4));
  let PanelNode::Split(split) = tree.root else {
    panic!("outer split must remain")
  };
  assert_eq!(split.axis, PanelSplitAxis::Horizontal);
  let PanelNode::Split(nested) = split.second.as_ref() else {
    panic!("inner split must remain")
  };
  assert_eq!(nested.axis, PanelSplitAxis::Vertical);
  assert_eq!(nested.ratio, 0.8);
  let PanelNode::Leaf(leaf) = nested.second.as_ref() else {
    panic!("tab group must remain")
  };
  assert_eq!(leaf.active_tab, Some(PanelTabId::new(4)));
}

/// Mounts the real workbench inside Kit's root, without a display server.
fn mount(
  state: DocumentPanelState,
  cx: &mut gpui::TestAppContext,
) -> (Entity<ChitinApp>, &mut gpui::VisualTestContext) {
  cx.update(chitin_ui::init);
  let mut mounted = None;
  let (_, visual) = cx.add_window_view(|window, cx| {
    window.activate_window();
    let app = cx.new(|_| {
      let mut app = ChitinApp::new(None);
      app.workspace = None;
      app.project_sidebar_visible = false;
      app.document_panels = state;
      app
    });
    mounted = Some(app.clone());
    gpui_kit::component::Root::new(app, window, cx)
  });
  let Some(app) = mounted else {
    panic!("workbench must be mounted")
  };
  visual.run_until_parked();
  visual.update(|window, cx| window.draw(cx).clear());
  (app, visual)
}

fn two_documents() -> DocumentPanelState {
  let mut state = DocumentPanelState::new(OpenedProjectDocument::new(Path::new("first.cif")));
  state.open_document_as_tab(OpenedProjectDocument::new(Path::new("second.cif")));
  state.activate_tab(DEFAULT_DOCUMENT_PANEL_ID, PanelTabId::new(1));
  state
}

#[gpui::test]
fn ordinary_redraws_do_not_remount_document_entities(cx: &mut gpui::TestAppContext) {
  let (app, cx) = mount(two_documents(), cx);
  let ids = cx.update(|_, cx| {
    let Some(dock) = app.read(cx).document_dock.as_ref() else {
      panic!("dock must exist")
    };
    assert!(dock.area.read(cx).bounds().size.height > px(100.0));
    (
      dock.area.entity_id(),
      dock
        .tabs
        .iter()
        .map(|(id, entity)| (*id, entity.entity_id()))
        .collect::<HashMap<_, _>>(),
    )
  });
  for _ in 0..3 {
    cx.update(|_, cx| app.update(cx, |_, cx| cx.notify()));
    cx.run_until_parked();
  }
  cx.update(|_, cx| {
    let Some(dock) = app.read(cx).document_dock.as_ref() else {
      panic!("dock must exist")
    };
    assert_eq!(dock.area.entity_id(), ids.0);
    assert_eq!(
      dock
        .tabs
        .iter()
        .map(|(id, entity)| (*id, entity.entity_id()))
        .collect::<HashMap<_, _>>(),
      ids.1
    );
  });
}

#[gpui::test]
fn kit_selection_and_close_update_the_command_model(cx: &mut gpui::TestAppContext) {
  let (app, cx) = mount(two_documents(), cx);
  let (area, second) = cx.update(|_, cx| {
    let Some(dock) = app.read(cx).document_dock.as_ref() else {
      panic!("dock must exist")
    };
    (dock.area.clone(), dock.tabs[&PanelTabId::new(2)].clone())
  });
  cx.update(|window, cx| {
    area.update(cx, |area, cx| {
      area.select_panel(gpui_kit::component::dock::PanelId::from(second.entity_id()), window, cx)
    })
  });
  cx.run_until_parked();
  cx.update(|_, cx| {
    assert_eq!(
      app
        .read(cx)
        .document_panels
        .active_document()
        .map(|doc| doc.title.as_str()),
      Some("second.cif")
    )
  });
  cx.update(|window, cx| area.update(cx, |area, cx| area.remove_panel(second, window, cx)));
  cx.run_until_parked();
  cx.update(|_, cx| assert_eq!(app.read(cx).document_panels.tree.tabs_in_order().len(), 1));
}

#[gpui::test]
fn kit_drag_to_split_retains_panel_entity_and_projects_focus(cx: &mut gpui::TestAppContext) {
  let (app, cx) = mount(two_documents(), cx);
  let (area, second, node) = cx.update(|_, cx| {
    let Some(dock) = app.read(cx).document_dock.as_ref() else {
      panic!("dock must exist")
    };
    let Some(layout) = dock
      .area
      .read(cx)
      .layout(gpui_kit::component::dock::DockPlacement::Center)
    else {
      panic!("center must exist")
    };
    (
      dock.area.clone(),
      dock.tabs[&PanelTabId::new(2)].clone(),
      layout.root().id(),
    )
  });
  cx.update(|window, cx| {
    area.update(cx, |area, cx| {
      area.move_panel(
        gpui_kit::component::dock::PanelId::from(second.entity_id()),
        gpui_kit::component::dock::InsertTarget::Split {
          node,
          placement: gpui_kit::component::Placement::Right,
          size: None,
        },
        window,
        cx,
      )
    })
  });
  cx.run_until_parked();
  cx.update(|window, cx| {
    window.draw(cx).clear();
    let Some(dock) = app.read(cx).document_dock.as_ref() else {
      panic!("dock must exist")
    };
    assert_eq!(app.read(cx).document_panels.tree.leaf_count(), 2);
    assert_eq!(dock.tabs[&PanelTabId::new(2)].entity_id(), second.entity_id());
    window.focus(&second.read(cx).focus.clone(), cx);
    window.draw(cx).clear();
  });
  cx.run_until_parked();
  cx.update(|_, cx| {
    let app = app.read(cx);
    let Some((leaf, _)) = app
      .document_panels
      .tree
      .tabs_in_order()
      .into_iter()
      .find(|(_, tab)| *tab == PanelTabId::new(2))
    else {
      panic!("moved document must exist")
    };
    assert_eq!(app.document_panels.focused_panel_id, leaf);
  });
}

#[gpui::test]
fn last_document_close_button_restores_empty_center(cx: &mut gpui::TestAppContext) {
  let state = DocumentPanelState::new(OpenedProjectDocument::new(Path::new("last.cif")));
  let (app, cx) = mount(state, cx);
  let Some(bounds) = cx.debug_bounds("document-last-close") else {
    panic!("last document must have a close button")
  };
  cx.simulate_click(bounds.center(), gpui::Modifiers::default());
  cx.run_until_parked();
  cx.update(|_, cx| {
    assert!(app.read(cx).document_panels.tree.tabs_in_order().is_empty());
    let Some(dock) = app.read(cx).document_dock.as_ref() else {
      panic!("empty dock must survive")
    };
    assert!(dock.tabs.is_empty());
  });
}

#[gpui::test]
fn kit_command_cancel_does_not_recursively_update_palette(cx: &mut gpui::TestAppContext) {
  let (app, cx) = mount(two_documents(), cx);
  cx.update(|window, cx| {
    app.update(cx, |app, cx| {
      app.command_panel.open(window, cx);
      cx.notify();
    })
  });
  cx.run_until_parked();
  cx.simulate_input("download");
  cx.run_until_parked();
  cx.update(|_, cx| assert_eq!(app.read(cx).command_panel.query(), "download"));
  cx.simulate_keystrokes("escape");
  cx.run_until_parked();
  cx.simulate_keystrokes("escape");
  cx.run_until_parked();
  cx.update(|_, cx| assert!(!app.read(cx).command_panel.is_open()));
}

#[gpui::test]
fn cross_group_move_keeps_destination_group_identity(cx: &mut gpui::TestAppContext) {
  let (app, cx) = mount(two_documents(), cx);
  cx.update(|window, cx| {
    app.update(cx, |app, cx| {
      app.split_document_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal, window, cx);
      cx.notify();
    })
  });
  cx.run_until_parked();
  cx.update(|window, cx| window.draw(cx).clear());
  let (area, second, destination, leaf_id) = cx.update(|_, cx| {
    let Some(dock) = app.read(cx).document_dock.as_ref() else {
      panic!("dock must exist")
    };
    let Some(layout) = dock.area.read(cx).layout(DockPlacement::Center) else {
      panic!("center must exist")
    };
    let second = dock.tabs[&PanelTabId::new(2)].clone();
    let third = dock.tabs[&PanelTabId::new(3)].clone();
    let kit_id = gpui_kit::component::dock::PanelId::from(third.entity_id());
    let Some(node) = layout.find_panel_node(kit_id) else {
      panic!("destination must exist")
    };
    (dock.area.clone(), second, node, third.read(cx).panel_id)
  });
  cx.update(|window, cx| {
    area.update(cx, |area, cx| {
      area.move_panel(
        gpui_kit::component::dock::PanelId::from(second.entity_id()),
        gpui_kit::component::dock::InsertTarget::Tabs {
          node: destination,
          ix: Some(0),
          activate: true,
        },
        window,
        cx,
      )
    })
  });
  cx.run_until_parked();
  cx.update(|window, cx| {
    window.draw(cx).clear();
    let app = app.read(cx);
    let Some(leaf) = app.document_panels.tree.leaf(leaf_id) else {
      panic!("destination identity must survive")
    };
    assert_eq!(leaf.tabs[0].id, PanelTabId::new(2));
    assert_eq!(leaf.active_tab, Some(PanelTabId::new(2)));
    let Some(dock) = app.document_dock.as_ref() else {
      panic!("dock must exist")
    };
    assert_eq!(dock.tabs[&PanelTabId::new(2)].entity_id(), second.entity_id());
  });
}
