//! Unit coverage for composite panel tree behavior.

use super::*;
use gpui::{Bounds, px};

/// Creates a root panel containing four stable test tabs.
///
/// # Parameters
///
/// This function takes no parameters.
///
/// # Returns
///
/// A [`PanelTree`] containing tabs A through D in one root leaf.
fn four_tab_tree() -> PanelTree<&'static str> {
  PanelTree::single_leaf(
    PanelLeaf::new(PanelId::new(1))
      .tab(PanelTab::new(PanelTabId::new(1), "A", "state-a"))
      .tab(PanelTab::new(PanelTabId::new(2), "B", "state-b"))
      .tab(PanelTab::new(PanelTabId::new(3), "C", "state-c"))
      .tab(PanelTab::new(PanelTabId::new(4), "D", "state-d")),
  )
}

/// Returns tab identifiers from a requested test panel.
///
/// # Parameters
///
/// `tree` is the test panel tree to inspect.
///
/// `panel_id` identifies the leaf whose tab order should be returned.
///
/// # Returns
///
/// Tab identifiers in their current visual order, or an empty vector when
/// the panel does not exist.
fn tab_ids(tree: &PanelTree<&'static str>, panel_id: PanelId) -> Vec<PanelTabId> {
  tree
    .leaf(panel_id)
    .map(|leaf| leaf.tabs.iter().map(|tab| tab.id).collect())
    .unwrap_or_default()
}

/// Verifies that a leaf activates its first inserted tab.
#[test]
fn panel_leaf_should_activate_first_inserted_tab() {
  let leaf = PanelLeaf::new(PanelId::new(1)).tab(PanelTab::new(PanelTabId::new(7), "Code", ()));

  assert_eq!(leaf.active_tab, Some(PanelTabId::new(7)));
}

/// Verifies that tab activation is scoped to the requested panel.
#[test]
fn panel_tree_should_activate_tab_in_target_leaf() {
  let mut tree = PanelTree::single_leaf(
    PanelLeaf::new(PanelId::new(1))
      .tab(PanelTab::new(PanelTabId::new(1), "Editor", ()))
      .tab(PanelTab::new(PanelTabId::new(2), "Preview", ())),
  );

  assert!(tree.activate_tab(PanelId::new(1), PanelTabId::new(2)));
  assert_eq!(
    tree.leaf(PanelId::new(1)).and_then(|leaf| leaf.active_tab),
    Some(PanelTabId::new(2))
  );
}

/// Verifies that closing the active tab activates the next available tab.
#[test]
fn panel_tree_should_activate_next_tab_when_closing_active_tab() {
  let mut tree = PanelTree::single_leaf(
    PanelLeaf::new(PanelId::new(1))
      .tab(PanelTab::new(PanelTabId::new(1), "Editor", ()))
      .tab(PanelTab::new(PanelTabId::new(2), "Preview", ()))
      .tab(PanelTab::new(PanelTabId::new(3), "Structure", ())),
  );
  assert!(tree.activate_tab(PanelId::new(1), PanelTabId::new(2)));

  assert!(tree.close_tab(PanelId::new(1), PanelTabId::new(2)));

  let Some(leaf) = tree.leaf(PanelId::new(1)) else {
    panic!("leaf should exist");
  };
  assert_eq!(leaf.active_tab, Some(PanelTabId::new(3)));
}

/// Verifies that closing an inactive tab preserves the active tab.
#[test]
fn panel_tree_should_preserve_active_tab_when_closing_inactive_tab() {
  let mut tree = PanelTree::single_leaf(
    PanelLeaf::new(PanelId::new(1))
      .tab(PanelTab::new(PanelTabId::new(1), "Editor", ()))
      .tab(PanelTab::new(PanelTabId::new(2), "Preview", ())),
  );

  assert!(tree.close_tab(PanelId::new(1), PanelTabId::new(2)));

  let Some(leaf) = tree.leaf(PanelId::new(1)) else {
    panic!("leaf should exist");
  };
  assert_eq!(leaf.active_tab, Some(PanelTabId::new(1)));
}

/// Verifies moving a middle tab to the left uses the raw strip position.
#[test]
fn panel_tree_should_move_middle_tab_left() {
  let mut tree = four_tab_tree();

  assert!(tree.move_tab(
    PanelId::new(1),
    PanelTabId::new(3),
    PanelTabDropTarget {
      panel_id: PanelId::new(1),
      insertion_index: 1,
    },
  ));

  assert_eq!(
    tab_ids(&tree, PanelId::new(1)),
    vec![
      PanelTabId::new(1),
      PanelTabId::new(3),
      PanelTabId::new(2),
      PanelTabId::new(4)
    ]
  );
}

/// Verifies moving a middle tab right normalizes its post-removal index.
#[test]
fn panel_tree_should_move_middle_tab_right() {
  let mut tree = four_tab_tree();

  assert!(tree.move_tab(
    PanelId::new(1),
    PanelTabId::new(2),
    PanelTabDropTarget {
      panel_id: PanelId::new(1),
      insertion_index: 4,
    },
  ));

  assert_eq!(
    tab_ids(&tree, PanelId::new(1)),
    vec![
      PanelTabId::new(1),
      PanelTabId::new(3),
      PanelTabId::new(4),
      PanelTabId::new(2)
    ]
  );
}

/// Verifies the first tab can move to the visual end of its strip.
#[test]
fn panel_tree_should_move_first_tab_to_end() {
  let mut tree = four_tab_tree();

  assert!(tree.move_tab(
    PanelId::new(1),
    PanelTabId::new(1),
    PanelTabDropTarget {
      panel_id: PanelId::new(1),
      insertion_index: 4,
    },
  ));

  assert_eq!(
    tab_ids(&tree, PanelId::new(1)),
    vec![
      PanelTabId::new(2),
      PanelTabId::new(3),
      PanelTabId::new(4),
      PanelTabId::new(1)
    ]
  );
}

/// Verifies the final tab can move to the beginning.
#[test]
fn panel_tree_should_move_last_tab_to_beginning() {
  let mut tree = four_tab_tree();

  assert!(tree.move_tab(
    PanelId::new(1),
    PanelTabId::new(4),
    PanelTabDropTarget {
      panel_id: PanelId::new(1),
      insertion_index: 0,
    },
  ));

  assert_eq!(
    tab_ids(&tree, PanelId::new(1)),
    vec![
      PanelTabId::new(4),
      PanelTabId::new(1),
      PanelTabId::new(2),
      PanelTabId::new(3)
    ]
  );
}

/// Verifies an effective original-position drop does not change order.
#[test]
fn panel_tree_should_keep_order_for_effective_same_position() {
  let mut tree = four_tab_tree();

  assert!(tree.move_tab(
    PanelId::new(1),
    PanelTabId::new(2),
    PanelTabDropTarget {
      panel_id: PanelId::new(1),
      insertion_index: 2,
    },
  ));

  assert_eq!(
    tab_ids(&tree, PanelId::new(1)),
    vec![
      PanelTabId::new(1),
      PanelTabId::new(2),
      PanelTabId::new(3),
      PanelTabId::new(4)
    ]
  );
}

/// Verifies dropping immediately before a tab's original position is a no-op.
#[test]
fn panel_tree_should_keep_order_when_dropped_before_original_position() {
  let mut tree = four_tab_tree();

  assert!(tree.move_tab(
    PanelId::new(1),
    PanelTabId::new(2),
    PanelTabDropTarget {
      panel_id: PanelId::new(1),
      insertion_index: 1,
    },
  ));

  assert_eq!(
    tab_ids(&tree, PanelId::new(1)),
    vec![
      PanelTabId::new(1),
      PanelTabId::new(2),
      PanelTabId::new(3),
      PanelTabId::new(4)
    ]
  );
}

/// Verifies a same-panel move leaves the moved tab active.
#[test]
fn panel_tree_should_activate_reordered_tab() {
  let mut tree = four_tab_tree();

  assert!(tree.move_tab(
    PanelId::new(1),
    PanelTabId::new(3),
    PanelTabDropTarget {
      panel_id: PanelId::new(1),
      insertion_index: 0,
    },
  ));

  assert_eq!(
    tree.leaf(PanelId::new(1)).and_then(|leaf| leaf.active_tab),
    Some(PanelTabId::new(3))
  );
}

/// Verifies cross-panel movement transfers one owned tab and repairs source focus.
#[test]
fn panel_tree_should_move_active_tab_between_panels() {
  let mut tree = four_tab_tree();
  assert!(tree.activate_tab(PanelId::new(1), PanelTabId::new(2)));
  assert!(
    tree.split_leaf(
      PanelId::new(1),
      PanelSplitAxis::Horizontal,
      PanelLeaf::new(PanelId::new(2))
        .tab(PanelTab::new(PanelTabId::new(5), "E", "state-e"))
        .tab(PanelTab::new(PanelTabId::new(6), "F", "state-f")),
      PanelSplitPlacement::After,
      0.5,
    )
  );

  assert!(tree.move_tab(
    PanelId::new(1),
    PanelTabId::new(2),
    PanelTabDropTarget {
      panel_id: PanelId::new(2),
      insertion_index: 1,
    },
  ));

  assert_eq!(
    tab_ids(&tree, PanelId::new(2)),
    vec![PanelTabId::new(5), PanelTabId::new(2), PanelTabId::new(6)]
  );
  assert_eq!(
    tree.leaf(PanelId::new(1)).and_then(|leaf| leaf.active_tab),
    Some(PanelTabId::new(3))
  );
  assert_eq!(
    tree.leaf(PanelId::new(2)).and_then(|leaf| leaf.active_tab),
    Some(PanelTabId::new(2))
  );
  assert_eq!(
    tree
      .leaf(PanelId::new(2))
      .and_then(|leaf| leaf.active_tab())
      .map(|tab| tab.payload),
    Some("state-b")
  );
}

/// Verifies an inactive move preserves the source panel's active tab.
#[test]
fn panel_tree_should_preserve_source_active_tab_when_moving_inactive_tab() {
  let mut tree = four_tab_tree();
  assert!(tree.split_leaf(
    PanelId::new(1),
    PanelSplitAxis::Horizontal,
    PanelLeaf::new(PanelId::new(2)),
    PanelSplitPlacement::After,
    0.5,
  ));

  assert!(tree.move_tab(
    PanelId::new(1),
    PanelTabId::new(3),
    PanelTabDropTarget {
      panel_id: PanelId::new(2),
      insertion_index: 0,
    },
  ));

  assert_eq!(
    tree.leaf(PanelId::new(1)).and_then(|leaf| leaf.active_tab),
    Some(PanelTabId::new(1))
  );
}

/// Verifies moving into an empty panel activates its only tab.
#[test]
fn panel_tree_should_move_tab_into_empty_panel() {
  let mut tree = four_tab_tree();
  assert!(tree.split_leaf(
    PanelId::new(1),
    PanelSplitAxis::Horizontal,
    PanelLeaf::new(PanelId::new(2)),
    PanelSplitPlacement::After,
    0.5,
  ));

  assert!(tree.move_tab(
    PanelId::new(1),
    PanelTabId::new(4),
    PanelTabDropTarget {
      panel_id: PanelId::new(2),
      insertion_index: 0,
    },
  ));

  let Some(target) = tree.leaf(PanelId::new(2)) else {
    panic!("empty target panel should survive");
  };
  assert_eq!(target.tabs.len(), 1);
  assert_eq!(target.active_tab, Some(PanelTabId::new(4)));
}

/// Verifies moving a final source tab collapses its split around the target.
#[test]
fn panel_tree_should_remove_empty_source_and_preserve_adjacent_target() {
  let mut tree =
    PanelTree::single_leaf(PanelLeaf::new(PanelId::new(1)).tab(PanelTab::new(PanelTabId::new(1), "A", "state-a")));
  assert!(tree.split_leaf(
    PanelId::new(1),
    PanelSplitAxis::Horizontal,
    PanelLeaf::new(PanelId::new(2)).tab(PanelTab::new(PanelTabId::new(2), "B", "state-b",)),
    PanelSplitPlacement::After,
    0.5,
  ));

  assert!(tree.move_tab(
    PanelId::new(1),
    PanelTabId::new(1),
    PanelTabDropTarget {
      panel_id: PanelId::new(2),
      insertion_index: 1,
    },
  ));

  assert_eq!(tree.leaf_count(), 1);
  assert!(tree.leaf(PanelId::new(1)).is_none());
  assert_eq!(
    tab_ids(&tree, PanelId::new(2)),
    vec![PanelTabId::new(2), PanelTabId::new(1)]
  );
}

/// Verifies duplicate target identities reject a move without mutation.
#[test]
fn panel_tree_should_reject_duplicate_target_tab_id_atomically() {
  let mut tree =
    PanelTree::single_leaf(PanelLeaf::new(PanelId::new(1)).tab(PanelTab::new(PanelTabId::new(1), "A", ())));
  assert!(tree.split_leaf(
    PanelId::new(1),
    PanelSplitAxis::Horizontal,
    PanelLeaf::new(PanelId::new(2)).tab(PanelTab::new(PanelTabId::new(1), "B", ())),
    PanelSplitPlacement::After,
    0.5,
  ));
  let before = tree.clone();

  assert!(!tree.move_tab(
    PanelId::new(1),
    PanelTabId::new(1),
    PanelTabDropTarget {
      panel_id: PanelId::new(2),
      insertion_index: 0,
    },
  ));

  assert_eq!(tree, before);
}

/// Verifies a stale target panel rejects a move without changing the tree.
#[test]
fn panel_tree_should_reject_stale_target_atomically() {
  let mut tree = four_tab_tree();
  let before = tree.clone();

  assert!(!tree.move_tab(
    PanelId::new(1),
    PanelTabId::new(2),
    PanelTabDropTarget {
      panel_id: PanelId::new(99),
      insertion_index: 0,
    },
  ));

  assert_eq!(tree, before);
}

/// Verifies midpoint hit testing handles variable rendered tab widths.
#[test]
fn panel_tab_insertion_index_should_use_actual_variable_width_bounds() {
  let bounds = [
    Bounds {
      origin: gpui::point(px(10.0), px(0.0)),
      size: gpui::size(px(80.0), px(34.0)),
    },
    Bounds {
      origin: gpui::point(px(90.0), px(0.0)),
      size: gpui::size(px(200.0), px(34.0)),
    },
    Bounds {
      origin: gpui::point(px(290.0), px(0.0)),
      size: gpui::size(px(72.0), px(34.0)),
    },
  ];

  assert_eq!(panel_tab_insertion_index(&bounds, px(49.0)), 0);
  assert_eq!(panel_tab_insertion_index(&bounds, px(50.0)), 1);
  assert_eq!(panel_tab_insertion_index(&bounds, px(189.0)), 1);
  assert_eq!(panel_tab_insertion_index(&bounds, px(190.0)), 2);
  assert_eq!(panel_tab_insertion_index(&bounds, px(325.0)), 2);
  assert_eq!(panel_tab_insertion_index(&bounds, px(326.0)), 3);
  assert_eq!(panel_tab_insertion_index(&[], px(100.0)), 0);
}

/// Verifies that immutable leaf lookup returns the requested panel.
#[test]
fn panel_tree_should_find_leaf_by_id() {
  let tree =
    PanelTree::single_leaf(PanelLeaf::new(PanelId::new(1)).tab(PanelTab::new(PanelTabId::new(1), "Editor", ())));

  let Some(leaf) = tree.leaf(PanelId::new(1)) else {
    panic!("leaf should exist");
  };

  assert_eq!(leaf.id, PanelId::new(1));
}

/// Verifies that mutable leaf lookup returns the requested panel.
#[test]
fn panel_tree_should_find_mutable_leaf_by_id() {
  let mut tree = PanelTree::single_leaf(PanelLeaf::new(PanelId::new(1)));

  let Some(leaf) = tree.leaf_mut(PanelId::new(1)) else {
    panic!("leaf should exist");
  };
  leaf.add_tab(PanelTab::new(PanelTabId::new(1), "Editor", ()));

  assert_eq!(tree.leaf(PanelId::new(1)).map(|leaf| leaf.tabs.len()), Some(1));
}

/// Verifies that tab order follows panel-tree visual order.
#[test]
fn panel_tree_should_return_tabs_in_visual_order() {
  let mut tree = PanelTree::single_leaf(
    PanelLeaf::new(PanelId::new(1))
      .tab(PanelTab::new(PanelTabId::new(1), "Editor", ()))
      .tab(PanelTab::new(PanelTabId::new(2), "Preview", ())),
  );
  assert!(tree.split_leaf(
    PanelId::new(1),
    PanelSplitAxis::Horizontal,
    PanelLeaf::new(PanelId::new(2)).tab(PanelTab::new(PanelTabId::new(3), "Structure", ())),
    PanelSplitPlacement::After,
    0.5,
  ));

  assert_eq!(
    tree.tabs_in_order(),
    vec![
      (PanelId::new(1), PanelTabId::new(1)),
      (PanelId::new(1), PanelTabId::new(2)),
      (PanelId::new(2), PanelTabId::new(3)),
    ]
  );
}

/// Verifies that active tab order returns one active tab per non-empty leaf.
#[test]
fn panel_tree_should_return_active_tabs_in_visual_order() {
  let mut tree = four_tab_tree();
  assert!(tree.activate_tab(PanelId::new(1), PanelTabId::new(3)));
  assert!(tree.split_leaf(
    PanelId::new(1),
    PanelSplitAxis::Horizontal,
    PanelLeaf::new(PanelId::new(2)).tab(PanelTab::new(PanelTabId::new(5), "E", "state-e")),
    PanelSplitPlacement::After,
    0.5,
  ));

  assert_eq!(
    tree.active_tabs_in_order(),
    vec![
      (PanelId::new(1), PanelTabId::new(3)),
      (PanelId::new(2), PanelTabId::new(5)),
    ]
  );
}

/// Verifies that tab lookup returns the requested tab payload.
#[test]
fn panel_tree_should_find_tab_by_panel_and_tab_id() {
  let tree = four_tab_tree();

  let Some(tab) = tree.find_tab(PanelId::new(1), PanelTabId::new(2)) else {
    panic!("tab should exist");
  };

  assert_eq!(tab.payload, "state-b");
}

/// Verifies that first active tab follows panel-tree visual order.
#[test]
fn panel_tree_should_return_first_active_tab_in_visual_order() {
  let mut tree = four_tab_tree();
  assert!(tree.activate_tab(PanelId::new(1), PanelTabId::new(3)));
  assert!(tree.split_leaf(
    PanelId::new(1),
    PanelSplitAxis::Horizontal,
    PanelLeaf::new(PanelId::new(2)).tab(PanelTab::new(PanelTabId::new(5), "E", "state-e")),
    PanelSplitPlacement::Before,
    0.5,
  ));

  let Some((panel_id, tab)) = tree.first_active_tab() else {
    panic!("active tab should exist");
  };

  assert_eq!(panel_id, PanelId::new(2));
  assert_eq!(tab.id, PanelTabId::new(5));
}

/// Verifies that splitting a leaf creates a binary split node.
#[test]
fn panel_tree_should_split_leaf_into_binary_node() {
  let mut tree =
    PanelTree::single_leaf(PanelLeaf::new(PanelId::new(1)).tab(PanelTab::new(PanelTabId::new(1), "Editor", ())));

  assert!(tree.split_leaf(
    PanelId::new(1),
    PanelSplitAxis::Horizontal,
    PanelLeaf::new(PanelId::new(2)).tab(PanelTab::new(PanelTabId::new(2), "Preview", ())),
    PanelSplitPlacement::After,
    0.65,
  ));
  assert_eq!(tree.leaf_count(), 2);
}

/// Verifies that removing a split leaf promotes its sibling.
#[test]
fn panel_tree_should_collapse_split_when_removing_leaf() {
  let mut tree = PanelTree {
    root: PanelNode::Split(PanelSplit::new(
      PanelSplitAxis::Horizontal,
      0.5,
      PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(1))),
      PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(2))),
    )),
  };

  assert_eq!(tree.remove_leaf(PanelId::new(2)), Some(PanelId::new(1)));

  assert_eq!(tree.leaf_count(), 1);
  assert!(matches!(tree.root, PanelNode::Leaf(PanelLeaf { .. })));
  assert!(tree.leaf(PanelId::new(1)).is_some());
}

/// Verifies that removing a leaf can promote an entire sibling subtree.
#[test]
fn panel_tree_should_promote_sibling_subtree_when_removing_leaf() {
  let mut tree = PanelTree {
    root: PanelNode::Split(PanelSplit::new(
      PanelSplitAxis::Horizontal,
      0.5,
      PanelNode::Split(PanelSplit::new(
        PanelSplitAxis::Vertical,
        0.5,
        PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(1))),
        PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(2))),
      )),
      PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(3))),
    )),
  };

  assert_eq!(tree.remove_leaf(PanelId::new(3)), Some(PanelId::new(1)));

  assert_eq!(tree.leaf_count(), 2);
  assert!(tree.leaf(PanelId::new(1)).is_some());
  assert!(tree.leaf(PanelId::new(2)).is_some());
  assert!(tree.leaf(PanelId::new(3)).is_none());
}

/// Verifies that the root leaf is not removable.
#[test]
fn panel_tree_should_not_remove_only_root_leaf() {
  let mut tree = PanelTree::single_leaf(PanelLeaf::<()>::new(PanelId::new(1)));

  assert_eq!(tree.remove_leaf(PanelId::new(1)), None);

  assert_eq!(tree.leaf_count(), 1);
  assert!(tree.leaf(PanelId::new(1)).is_some());
}

/// Verifies that split resizing clamps ratios to supported bounds.
#[test]
fn panel_tree_should_clamp_split_resize_ratio() {
  let mut tree = PanelTree {
    root: PanelNode::Split(PanelSplit::new(
      PanelSplitAxis::Horizontal,
      0.5,
      PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(1))),
      PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(2))),
    )),
  };

  assert!(tree.resize_split(&PanelSplitPath::root(), 2.0));
  assert!(matches!(
    tree.root,
    PanelNode::Split(PanelSplit {
      ratio: MAX_PANEL_SPLIT_RATIO,
      ..
    })
  ));
}

/// Verifies that split ratio lookup returns the target split ratio.
#[test]
fn panel_tree_should_return_split_ratio_by_path() {
  let tree = PanelTree {
    root: PanelNode::Split(PanelSplit::new(
      PanelSplitAxis::Horizontal,
      0.35,
      PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(1))),
      PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(2))),
    )),
  };

  assert_eq!(tree.split_ratio(&PanelSplitPath::root()), Some(0.35));
}

/// Verifies that nested split sizing follows split ratios.
#[test]
fn panel_tree_should_return_nested_split_axis_size() {
  let tree = PanelTree {
    root: PanelNode::Split(PanelSplit::new(
      PanelSplitAxis::Horizontal,
      0.25,
      PanelNode::Split(PanelSplit::new(
        PanelSplitAxis::Vertical,
        0.5,
        PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(1))),
        PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(2))),
      )),
      PanelNode::Leaf(PanelLeaf::<()>::new(PanelId::new(3))),
    )),
  };

  assert_eq!(
    tree.split_axis_size(
      &PanelSplitPath::root().child(PanelSplitBranch::First),
      px(800.0),
      px(600.0),
    ),
    Some(px(600.0))
  );
}
