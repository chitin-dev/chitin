//! Detached dense source topology shared by fragment assembly and interactions.

use crate::layout::{NativeFailureKind as Failure, NativeStage as Stage};

use super::{Error, Work};
use chitin_chem::{
  AtomId, BondId, BondOrder, GraphError, MolecularGraph,
  perception::{ValenceModel, analyze_rings, analyze_valence},
};
use std::collections::BTreeMap;

/// One immutable source atom; coordinates and physical forces live elsewhere.
#[derive(Clone)]
pub(super) struct Atom {
  pub id: AtomId,
  pub number: u8,
  pub hydrogens: u8,
  pub charge: i16,
  pub stereochemical: bool,
}

/// Dense endpoints retain the original bond identity and iteration order.
#[derive(Clone)]
pub(super) struct Edge {
  pub id: BondId,
  pub ends: [usize; 2],
  pub order: u8,
  pub cyclic: bool,
}

/// A checked source snapshot; dense indices are never published as graph handles.
#[derive(Clone)]
pub(super) struct Model {
  pub atoms: Vec<Atom>,
  pub edges: Vec<Edge>,
  pub neighbors: Vec<Vec<(usize, usize)>>,
  pub cycles: Vec<Vec<usize>>,
  pub cycle_edges: Vec<Vec<usize>>,
  pub components: Vec<Vec<usize>>,
  pub stereo: Vec<([usize; 4], bool)>,
}

impl Model {
  /// Canonicalizes the layout view while preserving every graph-local handle.
  ///
  /// # Errors
  /// Source construction and bounded ordering failures propagate.
  pub(super) fn canonical(graph: &MolecularGraph, work: &mut Work) -> Result<Self, Error> {
    Self::reordered(Self::new(graph, work)?, work)
  }

  /// Reorders detached topology without changing its graph-local identities.
  fn reordered(mut model: Self, work: &mut Work) -> Result<Self, Error> {
    let (atoms, edges) = super::ordering::order(&model, work)?;
    work.charge(model.atoms.len() + model.edges.len())?;
    let mut inverse = vec![0; atoms.len()];
    for (new, &old) in atoms.iter().enumerate() {
      inverse[old] = new;
    }
    let mut old_atoms: Vec<_> = std::mem::take(&mut model.atoms).into_iter().map(Some).collect();
    for old in atoms {
      model.atoms.push(
        old_atoms[old]
          .take()
          .ok_or(Error::native(Stage::Topology, Failure::InvalidTopologyRemap))?,
      );
    }
    let mut old_edges: Vec<_> = std::mem::take(&mut model.edges).into_iter().map(Some).collect();
    model.neighbors = vec![Vec::new(); model.atoms.len()];
    for old in edges {
      let mut edge = old_edges[old]
        .take()
        .ok_or(Error::native(Stage::Topology, Failure::InvalidTopologyRemap))?;
      edge.ends = edge.ends.map(|i| inverse[i]);
      let [a, b] = edge.ends;
      model.neighbors[a].push((b, model.edges.len()));
      model.neighbors[b].push((a, model.edges.len()));
      model.edges.push(edge);
    }
    (model.cycles, model.cycle_edges) = super::cycles::discover(&model, work)?
      .into_iter()
      .map(|cycle| (cycle.atoms, cycle.edges))
      .unzip();
    for nodes in &mut model.components {
      for node in nodes.iter_mut() {
        *node = inverse[*node];
      }
      nodes.sort_unstable();
    }
    model.components.sort_by_key(|nodes| nodes.first().copied());
    for (atoms, _) in &mut model.stereo {
      *atoms = atoms.map(|i| inverse[i]);
    }
    Ok(model)
  }

  /// Builds stable adjacency, block-local cycle constraints and explicit alkene sides.
  ///
  /// # Errors
  /// Rejects unsupported orders or missing handles and charges graph/cycle traversal.
  pub(super) fn new(graph: &MolecularGraph, work: &mut Work) -> Result<Self, Error> {
    work.charge(graph.atom_count() + graph.bond_count())?;
    let valence = analyze_valence(graph, ValenceModel::SmilesOrganicV1)?;
    let atoms: Vec<_> = graph
      .atoms()
      .map(|(id, atom)| Atom {
        id,
        number: atom.element().atomic_number(),
        charge: atom.formal_charge().unwrap_or(0),
        stereochemical: graph.tetrahedral(id).is_some(),
        // Unsupported perception does not invent chemical facts: this tie-breaker
        // uses only declared/fixed H when no derived report is available.
        hydrogens: valence.atom(id).and_then(|row| row.value()).map_or_else(
          || {
            atom
              .declared_hydrogens()
              .unwrap_or(0)
              .saturating_add(atom.implicit_hydrogens().unwrap_or(0))
          },
          |row| row.hydrogens.declared().saturating_add(row.hydrogens.implicit()),
        ),
      })
      .collect();
    let indices: BTreeMap<_, _> = atoms.iter().enumerate().map(|(i, atom)| (atom.id, i)).collect();
    let rings = analyze_rings(graph)?;
    let mut edges = Vec::with_capacity(graph.bond_count());
    let mut neighbors = vec![Vec::new(); atoms.len()];
    for (id, bond) in graph.bonds() {
      let order = match bond.order() {
        BondOrder::Single => 1,
        BondOrder::Double => 2,
        BondOrder::Triple => 3,
        _ => return Err(Error::Bond(id)),
      };
      let [a, b] = bond.atoms();
      let [a, b] = [a, b].map(|id| indices.get(&id).copied().ok_or(GraphError::MissingAtom(id)));
      let (a, b) = (a?, b?);
      neighbors[a].push((b, edges.len()));
      neighbors[b].push((a, edges.len()));
      edges.push(Edge {
        id,
        ends: [a, b],
        order,
        cyclic: !rings.bridges().contains(&id),
      });
    }
    let cycles = Vec::new();
    let components = graph
      .connected_components()
      .into_iter()
      .map(|component| component.into_iter().map(|id| indices[&id]).collect())
      .collect();
    let stereo = graph
      .double_bond_stereos()
      .map(|(_, stereo)| {
        let [a, b] = stereo.ends();
        let [ra, rb] = stereo.references();
        (
          [ra, a, b, rb].map(|id| indices[&id]),
          stereo.relation() == chitin_chem::DoubleBondRelation::SameSide,
        )
      })
      .collect();
    let mut model = Self {
      atoms,
      edges,
      neighbors,
      cycles,
      cycle_edges: Vec::new(),
      components,
      stereo,
    };
    (model.cycles, model.cycle_edges) = super::cycles::discover(&model, work)?
      .into_iter()
      .map(|cycle| (cycle.atoms, cycle.edges))
      .unzip();
    Ok(model)
  }

  /// Removes one cyclic layout edge and recanonicalizes the detached model.
  ///
  /// # Errors
  /// Bridges cannot be opened; traversal and coordinate namespaces remain checked.
  pub(super) fn opened(&self, edge: usize, work: &mut Work) -> Result<Self, Error> {
    let removed = self
      .edges
      .get(edge)
      .ok_or(Error::native(Stage::Topology, Failure::InvalidRingCut))?;
    if !removed.cyclic {
      return Err(Error::native(Stage::Topology, Failure::InvalidRingCut));
    }
    work.charge(self.atoms.len() + self.edges.len() + self.cycles.iter().map(Vec::len).sum::<usize>())?;
    let mut result = self.clone();
    result.edges.remove(edge);
    result
      .stereo
      .retain(|(atoms, _)| !removed.ends.contains(&atoms[1]) || !removed.ends.contains(&atoms[2]));
    result.refresh(work)?;
    Self::reordered(result, work)
  }

  /// Restores the opened edge at the tail, retaining all current dense atom rows.
  pub(super) fn restore(&mut self, edge: &Edge, original: &Self, work: &mut Work) -> Result<(), Error> {
    let indices: BTreeMap<_, _> = self.atoms.iter().enumerate().map(|(i, atom)| (atom.id, i)).collect();
    let mut edge = edge.clone();
    edge.ends = edge.ends.map(|i| indices[&original.atoms[i].id]);
    self.edges.push(edge);
    self.stereo = original
      .stereo
      .iter()
      .map(|(atoms, same)| (atoms.map(|i| indices[&original.atoms[i].id]), *same))
      .collect();
    self.refresh(work)
  }

  /// Rebuilds adjacency and cyclic flags after a temporary edge edit.
  fn refresh(&mut self, work: &mut Work) -> Result<(), Error> {
    work.charge(self.atoms.len() + self.edges.len())?;
    self.neighbors = vec![Vec::new(); self.atoms.len()];
    for (i, edge) in self.edges.iter_mut().enumerate() {
      edge.cyclic = false;
      let [a, b] = edge.ends;
      self.neighbors[a].push((b, i));
      self.neighbors[b].push((a, i));
    }
    (self.cycles, self.cycle_edges) = super::cycles::discover(self, work)?
      .into_iter()
      .map(|cycle| (cycle.atoms, cycle.edges))
      .unzip();
    for edges in &self.cycle_edges {
      for &edge in edges {
        self.edges[edge].cyclic = true;
      }
    }
    Ok(())
  }

  /// Returns a source-order endpoint other than the supplied incident vertex.
  pub(super) fn other(&self, edge: usize, atom: usize) -> usize {
    let [a, b] = self.edges[edge].ends;
    if a == atom { b } else { a }
  }
}
