//! Exact core-template matching and atomic drawing placement for small cages.
//!
//! Templates contain only topology and drawing coordinates. Atom elements,
//! labels, isotope/charge knowledge and supplied stereo remain source facts.
//! Perspective crossings are explicit presentation; no chemical stereo is assigned.

use std::{
  collections::{BTreeMap, BTreeSet},
  f64::consts::{PI, TAU},
};

use chitin_chem::{AtomId, BondId, BondOrder, Element, MolecularGraph, perception::RingBlock};

use super::{BondCrossing, Error, Placement, Point, Work, score};

mod matching;
mod templates;
use templates::Template;

/// One exactly recognized core and all of its source-identity mappings.
pub(super) struct Cage {
  /// Authored adjacency with flat fallback and perspective projections.
  template: Template,
  /// Template vertices mapped to global layout rows, never fabricated atom IDs.
  mappings: Vec<Vec<usize>>,
  /// External-bond/label cost discourages mapping decorated atoms inside a face.
  decoration: BTreeMap<usize, f64>,
  /// Original core edges, keyed by ordered global endpoint rows.
  edges: BTreeMap<[usize; 2], BondId>,
  /// Conservative carbon or symmetry-preserving undecorated heterocage projection.
  perspective: bool,
}

impl Cage {
  /// Recognizes complete supported saturated cores, independent of SMILES order.
  ///
  /// # Parameters
  /// - `graph`, `block`, `indices`: one source namespace and its dense layout rows.
  /// - `work`: shared ring allowance, including complete isomorphism enumeration.
  ///
  /// # Returns
  /// All exact core mappings, or `None` for other topology or core bond orders.
  /// External substituents are not part of matching. Heteroatom replacements are
  /// allowed without inventing H counts, charge, aromaticity or stereochemistry.
  ///
  /// # Errors
  /// Invalid source references and work exhaustion fail before any pose exists.
  pub(super) fn new(
    graph: &MolecularGraph,
    block: &RingBlock,
    indices: &BTreeMap<AtomId, usize>,
    work: &mut Work,
  ) -> Result<Option<Self>, Error> {
    let Some(template) = Template::candidate(block.atoms().len(), block.bonds().len(), block.cycle_rank()) else {
      return Ok(None);
    };
    work.charge(block.atoms().len() + block.bonds().len())?;
    let local: BTreeMap<_, _> = block.atoms().iter().enumerate().map(|(row, &id)| (id, row)).collect();
    let mut rows = vec![Vec::new(); template.len()];
    let mut edges = BTreeMap::new();
    for &id in block.bonds() {
      let bond = graph.bond(id).ok_or(chitin_chem::GraphError::MissingBond(id))?;
      if bond.order() != BondOrder::Single {
        return Ok(None);
      }
      let [a, b] = bond.atoms();
      let mut ends = [indices[&a], indices[&b]];
      ends.sort_unstable();
      edges.insert(ends, id);
      let (a, b) = (local[&a], local[&b]);
      rows[a].push(b);
      rows[b].push(a);
    }
    let mappings = matching::enumerate(&template.rows(), &rows, work)?;
    if mappings.is_empty() {
      return Ok(None);
    }
    let symmetric_atoms = preserves_atom_symmetry(graph, block, &mappings, work)?;
    work.charge(mappings.len() * template.len() + template.len())?;
    let mappings = mappings
      .into_iter()
      .map(|map| map.into_iter().map(|row| indices[&block.atoms()[row]]).collect())
      .collect();
    let mut decoration = BTreeMap::new();
    let mut perspective = !template.crossings().is_empty();
    let mut carbon_core = true;
    let mut decorated_sites = 0;
    for (row, &id) in block.atoms().iter().enumerate() {
      let atom = graph.atom(id).ok_or(chitin_chem::GraphError::MissingAtom(id))?;
      let outside = graph.neighbors(id)?.len() - rows[row].len();
      decorated_sites += usize::from(outside > 0);
      carbon_core &= atom.element() == Element::Carbon;
      perspective &= atom.isotope().is_none()
        && atom.formal_charge() == Some(0)
        && !atom.is_aromatic()
        && atom.unpaired_electrons().is_none_or(|count| count == 0)
        && graph.tetrahedral(id).is_none();
      let label = !matches!(atom.element(), Element::Carbon | Element::Hydrogen);
      decoration.insert(indices[&id], outside as f64 * 10. + f64::from(label));
    }
    Ok(Some(Self {
      template,
      mappings,
      decoration,
      edges,
      perspective: perspective
        && if carbon_core {
          decorated_sites <= 1
        } else {
          decorated_sites == 0 && symmetric_atoms
        }
        && !component_has_alkene_stereo(graph, block.atoms()[0], work)?,
    }))
  }

  /// Returns the template's complete vertex count for system seeding priority.
  pub(super) fn len(&self) -> usize {
    self.template.len()
  }

  /// Places a complete core while retaining at most one existing articulation.
  ///
  /// # Parameters
  /// - `atoms`, `old`: complete block rows and immutable preexisting coordinates.
  /// - `component`: only this connected component participates in overlap cost.
  /// - `heading`: incoming attachment direction at an existing articulation.
  /// - `work`, `source`: shared allowance and original block error locator.
  ///
  /// # Returns
  /// Finite, exact source coverage and outward attachment headings. Exact core
  /// mappings compete using decoration and overlap cost. Flat poses try six
  /// rotations; eligible perspective projections preserve their upright view.
  /// The authored planar pose is never stretched to accommodate an anchor; all
  /// geometry remains private until the complete block can be published.
  ///
  /// # Errors
  /// Multiple entry anchors require a future constrained-template fitter. Work
  /// exhaustion and violated source/coordinate invariants return typed failures.
  pub(super) fn embed(
    &self,
    atoms: &[usize],
    old: &[Option<Point>],
    component: &[usize],
    heading: f64,
    work: &mut Work,
    source: AtomId,
  ) -> Result<Placement, Error> {
    work.charge(atoms.len() + self.len())?;
    let anchors: Vec<_> = atoms.iter().copied().filter(|&row| old[row].is_some()).collect();
    let unsupported = || Error::RingSystem {
      atom: source,
      cycles: self.template.rank(),
    };
    if anchors.len() > 1 {
      return Err(unsupported());
    }
    let local = if self.perspective {
      self.template.perspective().ok_or_else(unsupported)?
    } else {
      self.template.points()
    };
    let center = Point::new(
      local.iter().map(|p| p.x).sum::<f64>() / local.len() as f64,
      local.iter().map(|p| p.y).sum::<f64>() / local.len() as f64,
    );
    let mut best = None;
    let mut best_cost = f64::INFINITY;
    for (index, mapping) in self.mappings.iter().enumerate() {
      work.charge(mapping.len())?;
      let penalty = mapping
        .iter()
        .enumerate()
        .filter(|&(vertex, _)| {
          if self.perspective {
            self.template.perspective_interior(vertex)
          } else {
            self.template.interior(vertex)
          }
        })
        .map(|(_, row)| self.decoration[row])
        .sum::<f64>();
      let anchor = anchors.first().copied();
      let origin = anchor
        .and_then(|row| mapping.iter().position(|&other| other == row))
        .map_or(Point::new(0., 0.), |vertex| local[vertex]);
      let target = anchor.and_then(|row| old[row]).unwrap_or(Point::new(0., 0.));
      let base = if anchor.is_some() && !self.perspective {
        heading - (center.y - origin.y).atan2(center.x - origin.x)
      } else {
        0.
      };
      let rotations: &[f64] = if self.perspective {
        &[0.]
      } else {
        &[0., PI / 3., -PI / 3., 2. * PI / 3., -2. * PI / 3., PI]
      };
      for &rotation in rotations {
        work.charge(old.len() + mapping.len())?;
        let (sin, cos) = (base + rotation).sin_cos();
        let mut points = old.to_vec();
        for (&row, p) in mapping.iter().zip(&local) {
          let (x, y) = (p.x - origin.x, p.y - origin.y);
          points[row] = Some(old[row].unwrap_or(target.translated(x * cos - y * sin, x * sin + y * cos)));
        }
        let cost = penalty + score(atoms, &points, old, component, work)?;
        if cost + 1e-10 < best_cost {
          best_cost = cost;
          best = Some((index, points));
        }
      }
    }
    let (mapping, points) = best.ok_or_else(unsupported)?;
    let mapping = &self.mappings[mapping];
    let atoms = self.placements(mapping, &points, source, work)?;
    let mut crossings = Vec::new();
    if self.perspective {
      let edge = |ends: [usize; 2]| {
        let mut rows = ends.map(|vertex| mapping[vertex]);
        rows.sort_unstable();
        self.edges[&rows]
      };
      for &[front, back] in self.template.crossings() {
        crossings.push(BondCrossing {
          front: edge(front),
          back: edge(back),
        });
      }
    }
    Ok(Placement { atoms, crossings })
  }

  /// Resolves every source location and a largest free angular attachment sector.
  ///
  /// This is local clearance, not a guarantee that an interior cage vertex can
  /// attach outside the outer face without a later crossing diagnostic.
  fn placements(
    &self,
    mapping: &[usize],
    points: &[Option<Point>],
    source: AtomId,
    work: &mut Work,
  ) -> Result<Vec<(usize, Point, f64)>, Error> {
    work.charge(mapping.len() + self.template.edges().len() * 4)?;
    let rows = self.template.rows();
    mapping
      .iter()
      .enumerate()
      .map(|(vertex, &row)| {
        let origin = points[row].ok_or(Error::Invariant(source))?;
        let mut angles = rows[vertex]
          .iter()
          .map(|&neighbor| {
            let p = points[mapping[neighbor]].ok_or(Error::Invariant(source))?;
            Ok((p.y - origin.y).atan2(p.x - origin.x))
          })
          .collect::<Result<Vec<_>, Error>>()?;
        angles.sort_by(f64::total_cmp);
        let mut outward = (0., 0.);
        for i in 0..angles.len() {
          let gap = (angles[(i + 1) % angles.len()] - angles[i]).rem_euclid(TAU);
          if gap > outward.0 + 1e-10 {
            outward = (gap, angles[i] + gap / 2.);
          }
        }
        Ok((row, origin, outward.1))
      })
      .collect()
  }
}

/// Checks whether all core automorphisms preserve the supplied atomic facts.
///
/// # Parameters
/// - `graph`, `block`: the source core without external substitution.
/// - `mappings`: every complete template-to-block isomorphism, in local rows.
/// - `work`: shared allowance for comparisons.
///
/// # Returns
/// Whether each template vertex sees identical atomic attributes in every
/// mapping. Equal degree alone is insufficient: labels must preserve the actual
/// automorphism orbits. This is a sufficient symmetry guard, not CIP analysis;
/// callers separately exclude external substituents and supplied core stereo.
///
/// # Errors
/// Missing source atoms or exhausted shared work return typed layout errors.
fn preserves_atom_symmetry(
  graph: &MolecularGraph,
  block: &RingBlock,
  mappings: &[Vec<usize>],
  work: &mut Work,
) -> Result<bool, Error> {
  let Some(reference) = mappings.first() else {
    return Ok(false);
  };
  for mapping in mappings {
    work.charge(mapping.len())?;
    for (&a, &b) in reference.iter().zip(mapping) {
      let atom = |row: usize| {
        let id = block.atoms()[row];
        graph.atom(id).ok_or(chitin_chem::GraphError::MissingAtom(id))
      };
      if atom(a)? != atom(b)? {
        return Ok(false);
      }
    }
  }
  Ok(true)
}

/// Avoids perspective orientation conflicts with required external alkene flips.
///
/// Only the cage's own component is examined; an unrelated fragment must not
/// change its drawing policy. Traversal is iterative and uses the ring budget.
fn component_has_alkene_stereo(graph: &MolecularGraph, start: AtomId, work: &mut Work) -> Result<bool, Error> {
  if graph.double_bond_stereos().len() == 0 {
    return Ok(false);
  }
  let mut seen = BTreeSet::from([start]);
  let mut pending = vec![start];
  while let Some(atom) = pending.pop() {
    let neighbors = graph.neighbors(atom)?;
    work.charge(1 + neighbors.len())?;
    for (other, edge) in neighbors {
      if graph.double_bond_stereo(edge).is_some() {
        return Ok(true);
      }
      if seen.insert(other) {
        pending.push(other);
      }
    }
  }
  Ok(false)
}

#[cfg(test)]
mod tests {
  use super::*;

  type TestResult = Result<(), Box<dyn std::error::Error>>;

  /// Builds detached template metadata without keeping ownership of the graph.
  fn model(source: &str) -> Result<(Cage, Vec<usize>, AtomId), Box<dyn std::error::Error>> {
    let graph = chitin_chem::io::smiles::read_smiles(source)?.into_graph();
    let report = chitin_chem::perception::analyze_rings(&graph)?;
    let block = report
      .blocks()
      .iter()
      .find(|block| block.cycle_rank() > 0)
      .ok_or("missing core")?;
    let indices: BTreeMap<_, _> = graph.atoms().enumerate().map(|(row, (id, _))| (id, row)).collect();
    let model = Cage::new(&graph, block, &indices, &mut Work::new(2_000_000))?.ok_or("unmatched core")?;
    Ok((model, (0..graph.atom_count()).collect(), block.atoms()[0]))
  }

  #[test]
  fn every_single_entry_anchor_should_be_retained_exactly() -> TestResult {
    for source in ["C1C2CC3CC1CC(C2)C3", "C12C3C4C1C5C2C3C45"] {
      let (cage, atoms, source) = model(source)?;
      for &anchor in &atoms {
        let target = Point::new(7.125, -3.25);
        let mut old = vec![None; atoms.len()];
        old[anchor] = Some(target);
        let positions = cage.embed(&atoms, &old, &atoms, 0.7, &mut Work::new(2_000_000), source)?;
        assert_eq!(positions.atoms.len(), atoms.len());
        assert_eq!(
          positions
            .atoms
            .iter()
            .find(|&&(row, _, _)| row == anchor)
            .map(|&(_, point, _)| point),
          Some(target)
        );
      }
    }
    Ok(())
  }

  #[test]
  fn multiple_anchors_and_exhausted_pose_search_should_fail_without_modifying_input() -> TestResult {
    let (cage, atoms, source) = model("C12C3C4C1C5C2C3C45")?;
    let mut old = vec![None; atoms.len()];
    old[0] = Some(Point::new(0., 0.));
    old[1] = Some(Point::new(1., 0.));
    let before = old.clone();
    assert!(matches!(
      cage.embed(&atoms, &old, &atoms, 0., &mut Work::new(2_000_000), source),
      Err(Error::RingSystem { .. })
    ));
    assert!(matches!(
      cage.embed(&atoms, &old, &atoms, 0., &mut Work::new(0), source),
      Err(Error::RingSearchBudgetExceeded)
    ));
    assert_eq!(old, before);
    Ok(())
  }
}
