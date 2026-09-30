//! Unit coverage for document-area state transitions and panel tree behavior.

use std::path::Path;

use crate::workbench::documents::layout::{PanelLeaf, PanelSplitAxis, PanelTab, PanelTabId, PanelTree};
use gpui::SharedString;

use super::{
  DocumentPanelState, OpenedProjectDocument,
  state::{
    DEFAULT_DOCUMENT_PANEL_ID, DEFAULT_DOCUMENT_TAB_ID, DocumentPanelContent, FIRST_DYNAMIC_DOCUMENT_PANEL_ID,
    FIRST_DYNAMIC_DOCUMENT_TAB_ID,
  },
};

/// Creates an opened test document.
///
/// # Parameters
///
/// * `name` is the file name used for the document path and title.
///
/// # Returns
///
/// An [`OpenedProjectDocument`] rooted under a stable test directory.
fn test_document(name: &str) -> OpenedProjectDocument {
  OpenedProjectDocument::new(Path::new("/tmp/chitin-document-panel-test").join(name).as_path())
}

/// Creates project-document content for a test tab.
///
/// # Parameters
///
/// * `name` is the file name used for the document path and title.
///
/// # Returns
///
/// A [`DocumentPanelContent`] value wrapping the test document.
fn test_content(name: &str) -> DocumentPanelContent {
  DocumentPanelContent::project(test_document(name))
}

/// Creates document panel state containing two tabs in the root panel.
fn two_tab_document_panel_state() -> DocumentPanelState {
  DocumentPanelState {
    tree: PanelTree::single_leaf(
      PanelLeaf::new(DEFAULT_DOCUMENT_PANEL_ID)
        .tab(PanelTab::new(
          DEFAULT_DOCUMENT_TAB_ID,
          SharedString::from("alpha.rs"),
          test_content("alpha.rs"),
        ))
        .tab(PanelTab::new(
          PanelTabId::new(2),
          SharedString::from("beta.rs"),
          test_content("beta.rs"),
        )),
    ),
    focused_panel_id: DEFAULT_DOCUMENT_PANEL_ID,
    next_panel_id: FIRST_DYNAMIC_DOCUMENT_PANEL_ID,
    next_tab_id: PanelTabId::new(3),
    options_menu_panel_id: None,
  }
}

/// Verifies that splitting a document panel copies only its active tab.
#[test]
fn split_panel_should_copy_only_active_tab() {
  let mut state = two_tab_document_panel_state();

  let Some(new_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal) else {
    panic!("document panel should split");
  };
  let Some(source_leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
    panic!("source panel should still exist");
  };
  let Some(new_leaf) = state.tree.leaf(new_panel_id) else {
    panic!("new panel should exist");
  };

  assert_eq!(source_leaf.tabs.len(), 2);
  assert_eq!(source_leaf.active_tab, Some(DEFAULT_DOCUMENT_TAB_ID));
  assert_eq!(new_leaf.tabs.len(), 1);
  assert_eq!(new_leaf.tabs[0].payload.title(), "alpha.rs");
  assert_eq!(new_leaf.active_tab, Some(PanelTabId::new(3)));
}

/// Verifies that splitting copies the active tab at split time.
#[test]
fn split_panel_should_copy_changed_active_tab() {
  let mut state = two_tab_document_panel_state();
  assert!(state.activate_tab(DEFAULT_DOCUMENT_PANEL_ID, PanelTabId::new(2)));

  let Some(new_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Vertical) else {
    panic!("document panel should split");
  };
  let Some(new_leaf) = state.tree.leaf(new_panel_id) else {
    panic!("new panel should exist");
  };

  assert_eq!(new_leaf.tabs.len(), 1);
  assert_eq!(new_leaf.tabs[0].payload.title(), "beta.rs");
  assert_eq!(new_leaf.active_tab, Some(PanelTabId::new(3)));
}

/// Verifies that repeated splits do not copy inactive source tabs.
#[test]
fn split_panel_should_not_accumulate_all_source_tabs_on_repeated_splits() {
  let mut state = two_tab_document_panel_state();

  let Some(first_new_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal) else {
    panic!("first split should succeed");
  };
  let Some(second_new_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Vertical) else {
    panic!("second split should succeed");
  };

  let Some(first_new_leaf) = state.tree.leaf(first_new_panel_id) else {
    panic!("first new panel should exist");
  };
  let Some(second_new_leaf) = state.tree.leaf(second_new_panel_id) else {
    panic!("second new panel should exist");
  };

  assert_eq!(first_new_leaf.tabs.len(), 1);
  assert_eq!(second_new_leaf.tabs.len(), 1);
}

/// Verifies that closing a split panel's last tab removes that panel.
#[test]
fn close_tab_should_remove_empty_split_panel_and_keep_source_panel() {
  let mut state = two_tab_document_panel_state();

  let Some(new_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Vertical) else {
    panic!("document panel should split");
  };

  assert!(state.activate_tab(DEFAULT_DOCUMENT_PANEL_ID, PanelTabId::new(2)));
  assert!(state.close_tab(new_panel_id, PanelTabId::new(3)));

  let Some(source_leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
    panic!("source panel should still exist");
  };

  assert_eq!(state.tree.leaf_count(), 1);
  assert!(state.tree.leaf(new_panel_id).is_none());
  assert_eq!(source_leaf.tabs.len(), 2);
  assert_eq!(source_leaf.active_tab, Some(PanelTabId::new(2)));
  assert_eq!(state.focused_panel_id, DEFAULT_DOCUMENT_PANEL_ID);
}

/// Verifies next-tab focus wraps from the final tab to the first tab.
#[test]
fn focus_next_tab_should_wrap_to_first_tab() {
  let mut state = two_tab_document_panel_state();
  assert!(state.activate_tab(DEFAULT_DOCUMENT_PANEL_ID, PanelTabId::new(2)));

  assert!(state.focus_next_tab());

  let Some(leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
    panic!("focused panel should exist");
  };
  assert_eq!(leaf.active_tab, Some(DEFAULT_DOCUMENT_TAB_ID));
}

/// Verifies previous-tab focus wraps from the first tab to the final tab.
#[test]
fn focus_previous_tab_should_wrap_to_last_tab() {
  let mut state = two_tab_document_panel_state();

  assert!(state.focus_previous_tab());

  let Some(leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
    panic!("focused panel should exist");
  };
  assert_eq!(leaf.active_tab, Some(PanelTabId::new(2)));
}

/// Verifies closing the focused active tab selects its right neighbor.
#[test]
fn close_focused_tab_should_focus_right_neighbor() {
  let mut state = two_tab_document_panel_state();

  assert!(state.close_focused_tab());

  let Some(leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
    panic!("focused panel should exist");
  };
  assert_eq!(leaf.active_tab, Some(PanelTabId::new(2)));
  assert_eq!(state.focused_panel_id, DEFAULT_DOCUMENT_PANEL_ID);
}

/// Verifies closing the final focused tab selects its left neighbor.
#[test]
fn close_focused_tab_should_fall_back_to_left_neighbor() {
  let mut state = two_tab_document_panel_state();
  assert!(state.activate_tab(DEFAULT_DOCUMENT_PANEL_ID, PanelTabId::new(2)));

  assert!(state.close_focused_tab());

  let Some(leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
    panic!("focused panel should exist");
  };
  assert_eq!(leaf.active_tab, Some(DEFAULT_DOCUMENT_TAB_ID));
}

/// Verifies next-tab focus advances from one panel to the next panel.
#[test]
fn focus_next_tab_should_cross_panel_boundary() {
  let mut state = two_tab_document_panel_state();
  let Some(next_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal) else {
    panic!("document panel should split");
  };
  assert!(state.activate_tab(DEFAULT_DOCUMENT_PANEL_ID, PanelTabId::new(2)));

  assert!(state.focus_next_tab());

  let Some(next_leaf) = state.tree.leaf(next_panel_id) else {
    panic!("next panel should exist");
  };
  assert_eq!(state.focused_panel_id, next_panel_id);
  assert_eq!(next_leaf.active_tab, Some(PanelTabId::new(3)));
}

/// Verifies previous-tab focus moves from one panel to the previous panel.
#[test]
fn focus_previous_tab_should_cross_panel_boundary() {
  let mut state = two_tab_document_panel_state();
  let Some(next_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal) else {
    panic!("document panel should split");
  };
  assert!(state.activate_tab(next_panel_id, PanelTabId::new(3)));

  assert!(state.focus_previous_tab());

  let Some(previous_leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
    panic!("previous panel should exist");
  };
  assert_eq!(state.focused_panel_id, DEFAULT_DOCUMENT_PANEL_ID);
  assert_eq!(previous_leaf.active_tab, Some(PanelTabId::new(2)));
}

/// Verifies container navigation wraps from the final panel to the first tab.
#[test]
fn focus_next_tab_should_wrap_across_panels_to_first_tab() {
  let mut state = two_tab_document_panel_state();
  let Some(next_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal) else {
    panic!("document panel should split");
  };
  assert!(state.activate_tab(next_panel_id, PanelTabId::new(3)));

  assert!(state.focus_next_tab());

  let Some(first_leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
    panic!("first panel should exist");
  };
  assert_eq!(state.focused_panel_id, DEFAULT_DOCUMENT_PANEL_ID);
  assert_eq!(first_leaf.active_tab, Some(DEFAULT_DOCUMENT_TAB_ID));
}

/// Verifies container navigation wraps from the first tab to the final panel.
#[test]
fn focus_previous_tab_should_wrap_across_panels_to_last_tab() {
  let mut state = two_tab_document_panel_state();
  let Some(next_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal) else {
    panic!("document panel should split");
  };
  assert!(state.activate_tab(DEFAULT_DOCUMENT_PANEL_ID, DEFAULT_DOCUMENT_TAB_ID));

  assert!(state.focus_previous_tab());

  let Some(next_leaf) = state.tree.leaf(next_panel_id) else {
    panic!("next panel should exist");
  };
  assert_eq!(state.focused_panel_id, next_panel_id);
  assert_eq!(next_leaf.active_tab, Some(PanelTabId::new(3)));
}

/// Verifies that split views receive container-unique tab identifiers.
#[test]
fn split_panel_should_allocate_unique_tab_id_for_copied_active_tab() {
  let mut state = two_tab_document_panel_state();

  let Some(new_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal) else {
    panic!("document panel should split");
  };
  let Some(new_leaf) = state.tree.leaf(new_panel_id) else {
    panic!("new panel should exist");
  };

  assert_eq!(new_leaf.tabs[0].id, PanelTabId::new(3));
  assert_eq!(new_leaf.active_tab, Some(PanelTabId::new(3)));
}

/// Verifies that splitting an empty panel creates an empty new panel.
#[test]
fn split_panel_should_create_empty_leaf_when_source_has_no_active_tab() {
  let mut state = DocumentPanelState {
    tree: PanelTree::single_leaf(PanelLeaf::new(DEFAULT_DOCUMENT_PANEL_ID)),
    focused_panel_id: DEFAULT_DOCUMENT_PANEL_ID,
    next_panel_id: FIRST_DYNAMIC_DOCUMENT_PANEL_ID,
    next_tab_id: FIRST_DYNAMIC_DOCUMENT_TAB_ID,
    options_menu_panel_id: None,
  };

  let Some(new_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal) else {
    panic!("empty document panel should split");
  };
  let Some(new_leaf) = state.tree.leaf(new_panel_id) else {
    panic!("new panel should exist");
  };

  assert!(new_leaf.tabs.is_empty());
  assert_eq!(new_leaf.active_tab, None);
}

/// Verifies that split callers can provide an independent payload for the new leaf.
#[test]
fn split_panel_with_content_should_use_caller_payload() {
  let mut state = DocumentPanelState::with_content(test_content("alpha.rs"));
  let split_content = test_content("independent.rs");

  let Some(new_panel_id) = state.split_panel_with_content(
    DEFAULT_DOCUMENT_PANEL_ID,
    PanelSplitAxis::Horizontal,
    Some(split_content),
  ) else {
    panic!("document panel should split");
  };
  let Some(source_leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
    panic!("source panel should still exist");
  };
  let Some(new_leaf) = state.tree.leaf(new_panel_id) else {
    panic!("new panel should exist");
  };

  assert_eq!(source_leaf.tabs.len(), 1);
  assert_eq!(source_leaf.active_tab, Some(DEFAULT_DOCUMENT_TAB_ID));
  assert_eq!(source_leaf.tabs[0].payload.title(), "alpha.rs");
  assert_eq!(new_leaf.tabs.len(), 1);
  assert_eq!(new_leaf.tabs[0].payload.title(), "independent.rs");
  assert_eq!(new_leaf.active_tab, Some(FIRST_DYNAMIC_DOCUMENT_TAB_ID));
}

/// Verifies that closing a non-final tab keeps its panel in the layout.
#[test]
fn close_tab_should_keep_panel_when_tabs_remain() {
  let mut state = two_tab_document_panel_state();
  assert!(
    state
      .split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal)
      .is_some()
  );

  assert!(state.close_tab(DEFAULT_DOCUMENT_PANEL_ID, PanelTabId::new(2)));

  let Some(source_leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
    panic!("source panel should still exist");
  };
  assert_eq!(state.tree.leaf_count(), 2);
  assert_eq!(source_leaf.tabs.len(), 1);
  assert_eq!(source_leaf.active_tab, Some(DEFAULT_DOCUMENT_TAB_ID));
  assert_eq!(state.focused_panel_id, DEFAULT_DOCUMENT_PANEL_ID);
}

/// Verifies that closing the final tab in the only panel keeps an empty panel.
#[test]
fn close_tab_should_keep_empty_state_when_only_panel_remains() {
  let mut state = DocumentPanelState::new(test_document("alpha.rs"));

  assert!(state.close_tab(DEFAULT_DOCUMENT_PANEL_ID, DEFAULT_DOCUMENT_TAB_ID));

  let Some(leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
    panic!("final panel should remain");
  };
  assert_eq!(state.tree.leaf_count(), 1);
  assert!(leaf.tabs.is_empty());
  assert_eq!(leaf.active_tab, None);
  assert_eq!(state.focused_panel_id, DEFAULT_DOCUMENT_PANEL_ID);
}

/// Verifies that repeated split closes keep the panel tree valid.
#[test]
fn close_tab_should_keep_panel_tree_valid_after_repeated_splits_and_closes() {
  let mut state = two_tab_document_panel_state();

  let Some(first_new_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal) else {
    panic!("first split should succeed");
  };
  let Some(second_new_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Vertical) else {
    panic!("second split should succeed");
  };

  assert!(state.close_tab(first_new_panel_id, PanelTabId::new(3)));
  assert!(state.close_tab(second_new_panel_id, PanelTabId::new(4)));

  assert_eq!(state.tree.leaf_count(), 1);
  assert!(state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID).is_some());
  assert_eq!(state.focused_panel_id, DEFAULT_DOCUMENT_PANEL_ID);
}

/// Verifies that opening another document appends a tab to the focused panel.
#[test]
fn open_document_as_tab_should_append_to_focused_panel() {
  let mut state = DocumentPanelState::new(test_document("alpha.rs"));

  assert!(state.open_document_as_tab(test_document("beta.rs")));

  let Some(leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
    panic!("default panel should exist");
  };

  assert_eq!(leaf.tabs.len(), 2);
  assert_eq!(leaf.active_tab, Some(PanelTabId::new(2)));
  assert_eq!(leaf.tabs[0].payload.title(), "alpha.rs");
  assert_eq!(leaf.tabs[1].payload.title(), "beta.rs");
}

/// Verifies that opening an already-open document focuses the existing tab.
#[test]
fn open_document_as_tab_should_focus_existing_tab_in_focused_panel() {
  let mut state = DocumentPanelState::new(test_document("alpha.rs"));
  assert!(state.open_document_as_tab(test_document("beta.rs")));
  assert!(state.activate_tab(DEFAULT_DOCUMENT_PANEL_ID, DEFAULT_DOCUMENT_TAB_ID));

  assert!(state.open_document_as_tab(test_document("beta.rs")));

  let Some(leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
    panic!("default panel should exist");
  };

  assert_eq!(leaf.tabs.len(), 2);
  assert_eq!(leaf.active_tab, Some(PanelTabId::new(2)));
  assert_eq!(leaf.tabs[1].payload.title(), "beta.rs");
}

/// Verifies that the structure-loading preflight activates an existing path without adding a tab.
#[test]
fn activate_path_in_focused_panel_should_reuse_existing_tab() {
  let mut state = two_tab_document_panel_state();

  assert!(state.activate_path_in_focused_panel(&test_document("beta.rs").path));

  let Some(leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
    panic!("default panel should exist");
  };
  assert_eq!((leaf.tabs.len(), leaf.active_tab), (2, Some(PanelTabId::new(2))));
}

/// Verifies that new documents open in the currently focused split panel.
#[test]
fn open_document_as_tab_should_target_focused_panel() {
  let mut state = two_tab_document_panel_state();
  let Some(new_panel_id) = state.split_panel(DEFAULT_DOCUMENT_PANEL_ID, PanelSplitAxis::Horizontal) else {
    panic!("document panel should split");
  };

  assert!(state.activate_tab(new_panel_id, PanelTabId::new(3)));
  assert!(state.open_document_as_tab(test_document("gamma.rs")));

  let Some(source_leaf) = state.tree.leaf(DEFAULT_DOCUMENT_PANEL_ID) else {
    panic!("source panel should exist");
  };
  let Some(focused_leaf) = state.tree.leaf(new_panel_id) else {
    panic!("focused panel should exist");
  };

  assert_eq!(source_leaf.tabs.len(), 2);
  assert_eq!(focused_leaf.tabs.len(), 2);
  assert_eq!(focused_leaf.active_tab, Some(PanelTabId::new(4)));
  assert_eq!(focused_leaf.tabs[1].payload.title(), "gamma.rs");
}
