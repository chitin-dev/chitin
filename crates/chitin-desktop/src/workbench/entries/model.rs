//! Cached scientific hierarchy and rendering-view-scoped selection bookmarks.
//!
//! Node indices are local to one immutable scene. Selection and visibility
//! remain renderer-owned; this module only projects their snapshots for the UI.

use crate::features::molecule::settings::MolecularSettingsControls;
use chitin_bio::structure::{
  AtomId, ChainId, ElementCategory, ResidueId, ResidueKind, StructureScene, selection::AtomSelection,
};
use chitin_command::RenderLayer;
use gpui::{EntityId, ListAlignment, ListState, px};
use std::{
  collections::{BTreeMap, HashMap, HashSet},
  sync::Arc,
};

/// Scientific roles used to group tree nodes and choose their presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NodeKind {
  /// Root representing all coordinate-bearing atoms in an entry.
  Structure,
  /// Container for the entry-wide rendering layers, independent of topology.
  Representations,
  /// One entry-wide rendering layer inspected without selecting its atoms.
  Representation(RenderLayer),
  /// Container for the entry's coordinate-bearing chains.
  Chains,
  /// One chain, containing polymer, ligand, and water groups.
  Chain,
  /// Polymer residue group within a chain.
  Residues,
  /// Non-polymer group or an individual non-water ligand.
  Ligands,
  /// Water group or an individual water residue.
  Waters,
  /// Individual polymer residue.
  Residue,
}

/// Cached topology node; child indices address the owning entry's node vector.
#[derive(Clone)]
pub(super) struct EntryNode {
  /// Number of residue leaves represented by this node.
  pub residue_count: usize,
  /// Display label; the structure root is refreshed from the current tab title.
  pub label: String,
  /// Grouping and icon role; ligand and water roles also describe leaves.
  pub kind: NodeKind,
  /// Chain identity when the entire node belongs to a single chain.
  pub chain: Option<ChainId>,
  /// Whether this node represents polymer atoms for layer color previews.
  pub polymer: bool,
  /// Coordinate-bearing atom identities used by selection and visibility commands.
  pub atoms: Arc<Vec<AtomId>>,
  /// Bounded element/chain samples used to preview renderer colors.
  pub samples: Vec<(ElementCategory, usize)>,
  /// Direct child indices in stable display order.
  pub children: Vec<usize>,
}

impl NodeKind {
  /// Returns the bundled scientific icon for this node role.
  pub fn icon(self) -> &'static str {
    match self {
      Self::Structure => "icons/entry-structure.svg",
      Self::Representations => "icons/atom-ball-and-stick.svg",
      Self::Representation(RenderLayer::Atom) => "icons/atom-stick.svg",
      Self::Representation(RenderLayer::Polymer) => "icons/atom-cartoon.svg",
      Self::Representation(RenderLayer::Surface) => "icons/surface-solid.svg",
      Self::Chains | Self::Chain => "icons/entry-chains.svg",
      Self::Residues | Self::Residue => "icons/entry-residues.svg",
      Self::Ligands => "icons/entry-ligands.svg",
      Self::Waters => "icons/entry-water.svg",
    }
  }
}

/// Named atom snapshot valid only for the rendering view that owns it.
pub(super) struct SavedSelection {
  /// Monotonic entry-local identity used by UI elements and removal actions.
  pub id: u64,
  /// User-visible name, unique among this entry's bookmarks.
  pub name: String,
  /// Canonical atom identities captured when the bookmark was saved.
  pub atoms: Arc<AtomSelection>,
}

/// Scene-local hierarchy, UI state, and projections of canonical renderer state.
///
/// Replacing the rendering view or scene invalidates this entire cache,
/// including bookmarks whose atom identities belonged to the old scene.
pub(super) struct EntryState {
  /// Rendering entity identity guarding delayed callbacks and bookmarks.
  pub view: EntityId,
  /// Immutable scene backing all node and atom identities.
  pub scene: Arc<StructureScene>,
  /// Indexed hierarchy; structure and chain-container roots occupy slots 0 and 1.
  pub nodes: Vec<EntryNode>,
  /// Nodes whose children appear in the flattened tree projection.
  pub expanded: HashSet<usize>,
  /// Node inspected by the properties panel, independent of atom selection.
  pub selected: usize,
  /// At most one entry-local representation editor is expanded at a time.
  pub open_layer: Option<RenderLayer>,
  /// Lazily created editors bound only to this entry's rendering view.
  pub settings: Option<MolecularSettingsControls>,
  /// Entry-local selection bookmarks; these are not persisted to disk.
  pub saved: Vec<SavedSelection>,
  /// Next bookmark identity and default-name suffix.
  pub next_saved: u64,
  /// Last canonical hidden-atom snapshot used to compute node counts.
  hidden: Arc<AtomSelection>,
  /// Hidden-atom counts parallel to `nodes`, including partial visibility.
  pub hidden_counts: Vec<usize>,
  /// Last canonical selection snapshot used to compute the status summary.
  selected_atoms: Arc<AtomSelection>,
  /// Selected atom, residue, and chain counts, respectively.
  pub selection_counts: (usize, usize, usize),
}

impl EntryState {
  /// Builds entry-wide representation nodes and the scientific topology hierarchy.
  ///
  /// # Parameters
  ///
  /// * `view` identifies the rendering entity whose callbacks may use this cache.
  /// * `scene` supplies coordinate-bearing atoms and source topology metadata.
  ///
  /// # Returns
  ///
  /// An entry with structure, representation, and chain-container roots expanded, empty
  /// bookmarks, and no cached hidden or selected atoms. Residues without
  /// coordinates are excluded because they cannot be targeted in the viewport.
  pub fn new(view: EntityId, scene: Arc<StructureScene>) -> Self {
    let mut nodes = vec![EntryNode {
      residue_count: 0,
      label: "Structure".into(),
      kind: NodeKind::Structure,
      chain: None,
      polymer: false,
      atoms: Arc::new(scene.atoms.iter().map(|atom| atom.atom_id).collect()),
      samples: Vec::new(),
      children: vec![1],
    }];
    nodes.push(EntryNode {
      label: "Chains".into(),
      kind: NodeKind::Chains,
      children: Vec::new(),
      ..nodes[0].clone()
    });
    let mut residues: BTreeMap<ChainId, BTreeMap<ResidueId, Vec<AtomId>>> = BTreeMap::new();
    let mut samples = HashMap::new();
    for atom in &scene.atoms {
      residues
        .entry(atom.chain_id)
        .or_default()
        .entry(atom.residue_id)
        .or_default()
        .push(atom.atom_id);
      samples.insert(atom.atom_id, (atom.element, atom.chain_id.index()));
    }
    for (chain, residues) in residues {
      let chain_index = nodes.len();
      nodes[1].children.push(chain_index);
      let name = scene
        .selection
        .chain(chain)
        .and_then(|chain| chain.auth_id.as_ref().or(chain.label_id.as_ref()))
        .map(String::as_str)
        .unwrap_or("(blank)");
      nodes.push(EntryNode {
        label: format!("Chain {name}"),
        kind: NodeKind::Chain,
        chain: Some(chain),
        atoms: Arc::new(residues.values().flatten().copied().collect()),
        children: Vec::new(),
        ..nodes[0].clone()
      });
      for kind in [NodeKind::Residues, NodeKind::Ligands, NodeKind::Waters] {
        let group_index = nodes.len();
        nodes[chain_index].children.push(group_index);
        let mut group_atoms = Vec::new();
        nodes.push(EntryNode {
          residue_count: 0,
          label: format!("{kind:?}"),
          kind,
          chain: Some(chain),
          polymer: kind == NodeKind::Residues,
          atoms: Arc::default(),
          children: Vec::new(),
          samples: Vec::new(),
        });
        for (residue, atoms) in &residues {
          let Some(metadata) = scene.selection.residue(*residue) else {
            continue;
          };
          // Water names take precedence: non-polymer topology also includes ligands.
          let category = if matches!(
            metadata.name.to_ascii_uppercase().as_str(),
            "HOH" | "WAT" | "H2O" | "DOD"
          ) {
            NodeKind::Waters
          } else if metadata.kind == ResidueKind::Polymer {
            NodeKind::Residues
          } else {
            NodeKind::Ligands
          };
          if category != kind {
            continue;
          }
          let leaf_index = nodes.len();
          nodes[group_index].children.push(leaf_index);
          group_atoms.extend(atoms);
          nodes.push(EntryNode {
            residue_count: 1,
            label: format!(
              "{} {}{}",
              metadata.name,
              metadata.sequence_number,
              metadata.insertion_code.map(|c| c.to_string()).unwrap_or_default()
            ),
            kind: if category == NodeKind::Residues {
              NodeKind::Residue
            } else {
              category
            },
            chain: Some(chain),
            polymer: metadata.kind == ResidueKind::Polymer,
            atoms: Arc::new(atoms.clone()),
            samples: Vec::new(),
            children: Vec::new(),
          });
        }
        nodes[group_index].atoms = Arc::new(group_atoms);
        nodes[group_index].label = format!("{kind:?} · {}", nodes[group_index].children.len());
      }
    }
    nodes[1].label = format!("Chains · {}", nodes[1].children.len());
    // Parents precede their children, so reverse order accumulates leaf counts.
    for index in (0..nodes.len()).rev() {
      if !nodes[index].children.is_empty() {
        nodes[index].residue_count = nodes[index]
          .children
          .iter()
          .map(|child| nodes[*child].residue_count)
          .sum();
      }
    }
    // Keep color previews bounded; rendering rows must not revisit every atom.
    for node in &mut nodes {
      for atom in node.atoms.iter() {
        if let Some(sample) = samples.get(atom) {
          if !node.samples.contains(sample) {
            node.samples.push(*sample);
          }
          if node.samples.len() == 6 {
            break;
          }
        }
      }
    }
    // Representation nodes carry no atom scope: inspecting or toggling a layer
    // must not select atoms, hide topology, or duplicate scientific counts.
    let representations = nodes.len();
    nodes[0].children.insert(0, representations);
    nodes.push(EntryNode {
      residue_count: 0,
      label: "Representations".into(),
      kind: NodeKind::Representations,
      chain: None,
      polymer: false,
      atoms: Arc::default(),
      samples: Vec::new(),
      children: Vec::new(),
    });
    for layer in [RenderLayer::Atom, RenderLayer::Polymer, RenderLayer::Surface] {
      let index = nodes.len();
      nodes[representations].children.push(index);
      nodes.push(EntryNode {
        label: format!("{layer:?}"),
        kind: NodeKind::Representation(layer),
        samples: nodes[0].samples.clone(),
        children: Vec::new(),
        ..nodes[representations].clone()
      });
    }
    let hidden_counts = vec![0; nodes.len()];
    Self {
      view,
      scene,
      nodes,
      expanded: [0, 1, representations].into(),
      selected: 0,
      open_layer: None,
      settings: None,
      saved: Vec::new(),
      next_saved: 1,
      hidden: Arc::default(),
      hidden_counts,
      selected_atoms: Arc::default(),
      selection_counts: (0, 0, 0),
    }
  }

  /// Expands a layer or collapses it when already open, without selecting atoms.
  pub fn toggle_layer(&mut self, layer: RenderLayer) {
    self.open_layer = if self.open_layer == Some(layer) {
      None
    } else {
      Some(layer)
    };
  }

  /// Recomputes node visibility counts when the canonical hidden snapshot changes.
  pub fn sync_hidden(&mut self, hidden: &Arc<AtomSelection>) {
    // Renderer snapshots are immutable; shared identity avoids work on every frame.
    if Arc::ptr_eq(&self.hidden, hidden) {
      return;
    }
    for (node, count) in self.nodes.iter().zip(&mut self.hidden_counts) {
      *count = node.atoms.iter().filter(|atom| hidden.contains(atom)).count();
    }
    self.hidden = hidden.clone();
  }
  /// Refreshes selected atom/residue/chain totals when the canonical snapshot changes.
  pub fn sync_selection(&mut self, selected: &Arc<AtomSelection>) {
    if Arc::ptr_eq(&self.selected_atoms, selected) {
      return;
    }
    self.selection_counts = self.scene.selection.counts(&self.scene, selected);
    self.selected_atoms = selected.clone();
  }

  /// Flattens expanded nodes into `(node index, depth, label)` display rows.
  ///
  /// `title` replaces the root label with the current document title. Children
  /// retain construction order, and collapsed descendants are not visited.
  pub fn rows(&self, title: &str) -> Vec<(usize, usize, String)> {
    let mut rows = Vec::new();
    let mut stack = vec![(0, 0)];
    while let Some((node, depth)) = stack.pop() {
      rows.push((
        node,
        depth,
        if node == 0 {
          title.to_owned()
        } else {
          self.nodes[node].label.clone()
        },
      ));
      if self.expanded.contains(&node) {
        // A LIFO stack needs reverse insertion to preserve sibling display order.
        stack.extend(self.nodes[node].children.iter().rev().map(|child| (*child, depth + 1)));
      }
    }
    rows
  }
}

/// App-owned Entries state retained across sidebar switches.
pub(crate) struct EntriesSidebarState {
  /// Scene caches keyed by document tab identity, not panel position.
  pub(super) entries: HashMap<u64, EntryState>,
  /// Document tab targeted by selection controls and bookmarks.
  pub(super) active: Option<u64>,
  /// Lazily mounted inputs whose values survive sidebar switches.
  pub(super) controls: Option<super::EntriesControls>,
  /// Retained input subscriptions; dropping them would disconnect Enter actions.
  pub(super) subscriptions: Vec<gpui::Subscription>,
  /// Latest parse, command, or bookmark validation error shown inline.
  pub(super) error: Option<String>,
  /// Variable-height virtualized tree geometry and retained scroll position.
  pub(super) list: ListState,
  /// Last row identities and height-affecting editor state measured by the list.
  layout: Vec<RowLayoutKey>,
}

/// Identity and editor structure affecting a virtualized row's measured height.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct RowLayoutKey {
  /// Stable document identity, independent of the current docking position.
  pub tab: u64,
  /// Rendering revision invalidating cached node identities when replaced.
  pub view: Option<EntityId>,
  /// Local tree identity whose header remains a fixed-height row.
  pub node: usize,
  /// Presence of an editor, uniform-color field, and advanced backend selector.
  pub editor: Option<(bool, bool)>,
}

impl Default for EntriesSidebarState {
  fn default() -> Self {
    let list = ListState::new(0, ListAlignment::Top, px(80.0));
    // Variable-height editors must follow native wheel deltas without an
    // interpolated offset painting rows outside the measured visible range.
    list.set_smooth_scroll_enabled(false);
    Self {
      entries: HashMap::new(),
      active: None,
      controls: None,
      subscriptions: Vec::new(),
      error: None,
      list,
      layout: Vec::new(),
    }
  }
}

impl EntriesSidebarState {
  /// Invalidates only changed rows, preserving unaffected measurements and scroll anchors.
  pub(super) fn sync_layout(&mut self, next: Vec<RowLayoutKey>, focus: &[Option<gpui::FocusHandle>]) {
    if self.layout.len() == next.len() {
      for (index, (old, new)) in self.layout.iter().zip(&next).enumerate() {
        if old != new {
          self.list.splice_focusable(index..index + 1, [focus[index].clone()]);
        }
      }
    } else {
      let prefix = self
        .layout
        .iter()
        .zip(&next)
        .take_while(|(old, new)| old == new)
        .count();
      let suffix = self.layout[prefix..]
        .iter()
        .rev()
        .zip(next[prefix..].iter().rev())
        .take_while(|(old, new)| old == new)
        .count();
      self.list.splice_focusable(
        prefix..self.layout.len() - suffix,
        focus[prefix..next.len() - suffix].iter().cloned(),
      );
    }
    self.layout = next;
  }

  /// Disconnects delayed editor events when the Entries view is hidden.
  pub(crate) fn unbind_editors(&self) {
    for entry in self.entries.values() {
      if let Some(settings) = &entry.settings {
        settings.bind_entry(None);
      }
    }
  }
  /// Sets or clears the inspector's latest command validation error.
  pub(crate) fn set_error(&mut self, error: Option<String>) {
    self.error = error;
  }

  /// Drops caches and bookmarks for closed or replaced rendering views.
  pub(crate) fn prune(&mut self, panels: &crate::workbench::documents::DocumentPanelState) {
    self
      .entries
      .retain(|id, state| panels.rendering_view_id(*id) == Some(state.view));
    if self.active.is_some_and(|id| !self.entries.contains_key(&id)) {
      self.active = None;
    }
  }
}
