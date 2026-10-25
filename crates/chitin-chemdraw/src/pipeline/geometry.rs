//! Immutable geometric admission rules, separate from contact ranking.
//!
//! Bond-length conventions permit bounded local exceptions in complex ring
//! blocks and at high-coordinate centers. Chemistry, measured chain/small-ring
//! angles and declared alkene relations remain independently protected.

use chitin_chem::{AtomId, BondOrder, DoubleBondRelation, Element, MolecularGraph, perception::analyze_rings};

use crate::{
  depict::DepictError,
  drawing_rules::{LengthLimit, LengthRules},
  geometry::Point,
  layout::Layout2d,
};

/// One angular error bound captured at stage entry, never a moving target.
struct AngleLimit {
  /// Ligand, center, ligand in the source graph namespace.
  atoms: [AtomId; 3],
  /// Reference angle for rigid centers or ideal angle for measured flexible ones.
  target: f64,
  /// Maximum absolute deviation accepted for this individual constraint.
  error: f64,
}

/// Feasibility snapshot independent of the current contact-score champion.
pub(super) struct GeometryGuard {
  /// Source-local limits retain projected references without cumulative drift.
  bonds: Vec<([AtomId; 2], LengthLimit)>,
  /// Local multiple-bond geometry and explicitly scored chain/ring angles.
  angles: Vec<AngleLimit>,
  /// Explicit relative sides, rechecked independently of coordinate reflection.
  alkenes: Vec<([AtomId; 4], DoubleBondRelation)>,
}

impl GeometryGuard {
  /// Captures hard geometry and nonworsening angular errors from a drawn baseline.
  ///
  /// # Parameters
  /// - `graph`: immutable source facts, including declared alkene relations.
  /// - `baseline`: complete drawn geometry defining every feasibility bound.
  ///
  /// # Returns
  /// Perspective drawings retain their actual lengths and local multiple-bond
  /// angles, but are not judged against ideal planar chain/ring targets.
  ///
  /// # Errors
  /// Missing source positions and inconsistent topology return typed errors.
  pub(super) fn new(graph: &MolecularGraph, baseline: &Layout2d) -> Result<Self, DepictError> {
    let topology = analyze_rings(graph)?;
    let lengths = LengthRules::from_topology(graph, &topology)?;
    let planar = baseline.bond_crossings().is_empty() && baseline.drawing_depths().is_empty();
    let point = |atom| baseline.position(atom).ok_or(DepictError::MissingPosition(atom));
    let bonds = graph
      .bonds()
      .map(|(id, bond)| {
        let atoms = bond.atoms();
        let limit = lengths.limit(id, point(atoms[0])?.distance(point(atoms[1])?));
        Ok((atoms, limit))
      })
      .collect::<Result<Vec<_>, DepictError>>()?;
    let mut angles = Vec::new();
    let mut add = |atoms: [AtomId; 3], target: Option<f64>| -> Result<(), DepictError> {
      let actual = angle([point(atoms[0])?, point(atoms[1])?, point(atoms[2])?]);
      let target = target.unwrap_or(actual);
      angles.push(AngleLimit {
        atoms,
        target,
        error: (actual - target).abs(),
      });
      Ok(())
    };
    for (center, atom) in graph.atoms() {
      let neighbors: Vec<_> = graph.neighbors(center)?.collect();
      let orders: Vec<_> = neighbors
        .iter()
        .filter_map(|(_, bond)| graph.bond(*bond).map(|bond| bond.order()))
        .collect();
      if orders.iter().any(|order| *order != BondOrder::Single) {
        for (index, &(a, _)) in neighbors.iter().enumerate() {
          for &(b, _) in &neighbors[index + 1..] {
            add([a, center, b], None)?;
          }
        }
      }
      if planar
        && atom.element() == Element::Carbon
        && neighbors.len() == 2
        && neighbors.iter().all(|(_, bond)| topology.bridges().contains(bond))
      {
        let target = if orders.contains(&BondOrder::Triple) || orders == [BondOrder::Double, BondOrder::Double] {
          Some(std::f64::consts::PI)
        } else if orders == [BondOrder::Single, BondOrder::Single] {
          Some(2. * std::f64::consts::PI / 3.)
        } else {
          None
        };
        if let Some(target) = target {
          add([neighbors[0].0, center, neighbors[1].0], Some(target))?;
        }
      }
    }
    if planar {
      for block in topology
        .blocks()
        .iter()
        .filter(|block| block.cycle_rank() == 1 && (3..=8).contains(&block.atoms().len()))
      {
        let target = (block.atoms().len() - 2) as f64 * std::f64::consts::PI / block.atoms().len() as f64;
        for &center in block.atoms() {
          let neighbors: Vec<_> = graph
            .neighbors(center)?
            .filter(|(_, bond)| block.bonds().contains(bond))
            .collect();
          if let [(a, _), (b, _)] = neighbors.as_slice() {
            add([*a, center, *b], Some(target))?;
          }
        }
      }
    }
    let alkenes = graph
      .double_bond_stereos()
      .map(|(_, stereo)| {
        let [a, b] = stereo.ends();
        let [ra, rb] = stereo.references();
        ([a, b, ra, rb], stereo.relation())
      })
      .collect();
    Ok(Self { bonds, angles, alkenes })
  }

  /// Tests a complete proposal against stage entry, before expensive re-depiction.
  /// Length limits use source-local drawing exceptions and relative numeric
  /// tolerance; angle errors use an absolute radian tolerance. A false result
  /// rejects this trial, not the original molecule.
  pub(super) fn permits(&self, candidate: &Layout2d) -> bool {
    let points = candidate.positions();
    self
      .bonds
      .iter()
      .all(|(atoms, length)| match (points.get(&atoms[0]), points.get(&atoms[1])) {
        (Some(a), Some(b)) => length.permits(a.distance(*b)),
        _ => false,
      })
      && self.angles.iter().all(|constraint| {
        let [Some(a), Some(c), Some(b)] = constraint.atoms.map(|id| points.get(&id).copied()) else {
          return false;
        };
        let actual = angle([a, c, b]);
        actual.is_finite() && (actual - constraint.target).abs() <= constraint.error + 1e-7
      })
      && self.alkenes.iter().all(|(atoms, relation)| {
        let [Some(a), Some(b), Some(ra), Some(rb)] = atoms.map(|id| points.get(&id).copied()) else {
          return false;
        };
        crate::stereo_geometry::relation([a, b, ra, rb]) == Some(*relation)
      })
  }
}

/// Unsigned angle in radians; collapsed rays remain nonfinite and ineligible.
fn angle([a, c, b]: [Point; 3]) -> f64 {
  let divisor = c.distance(a) * c.distance(b);
  if divisor <= 0. {
    return f64::NAN;
  }
  (((a.x - c.x) * (b.x - c.x) + (a.y - c.y) * (b.y - c.y)) / divisor)
    .clamp(-1., 1.)
    .acos()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::layout::{self, LayoutOptions};
  use chitin_chem::io::smiles::read_smiles;

  type TestResult = Result<(), Box<dyn std::error::Error>>;

  /// Builds authored geometry independently of any automatic generator.
  fn coordinates(graph: &MolecularGraph, points: &[(f64, f64)]) -> Result<Layout2d, crate::geometry::GeometryError> {
    Layout2d::from_positions(
      graph
        .atoms()
        .map(|(id, _)| id)
        .zip(points.iter().map(|&(x, y)| Point::new(x, y)))
        .collect(),
    )
  }

  #[test]
  fn source_bond_lengths_should_not_be_normalized_or_stretched() -> TestResult {
    let graph = read_smiles("CC")?.into_graph();
    let original = coordinates(&graph, &[(0., 0.), (1.8, 0.)])?;
    let guard = GeometryGuard::new(&graph, &original)?;
    assert!(guard.permits(&original));
    assert!(!guard.permits(&coordinates(&graph, &[(0., 0.), (1., 0.)])?));
    assert!(!guard.permits(&coordinates(&graph, &[(0., 0.), (2., 0.)])?));
    Ok(())
  }

  #[test]
  fn complex_ring_lengths_should_have_local_nonaccumulating_bounds() -> TestResult {
    let graph = read_smiles("C1CC2C3CCC1C23")?.into_graph();
    let baseline = layout::generate(&graph, &LayoutOptions::default())?;
    let guard = GeometryGuard::new(&graph, &baseline)?;
    let scaled = |factor: f64| {
      Layout2d::from_positions(
        baseline
          .positions()
          .iter()
          .map(|(&id, p)| (id, Point::new(p.x * factor, p.y * factor)))
          .collect(),
      )
    };
    assert!(guard.permits(&scaled(1.1)?));
    assert!(!guard.permits(&scaled(1.21)?));
    assert!(guard.permits(&baseline));
    Ok(())
  }

  #[test]
  fn high_coordination_should_allow_only_bounded_length_changes() -> TestResult {
    let graph = read_smiles("[Fe](O)(O)(O)(O)(O)O")?.into_graph();
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    let mut points = std::collections::BTreeMap::from([(ids[0], Point::new(0., 0.))]);
    for (i, &id) in ids[1..].iter().enumerate() {
      let angle = i as f64 * std::f64::consts::TAU / 6.;
      points.insert(id, Point::new(angle.cos(), angle.sin()));
    }
    let baseline = Layout2d::from_positions(points.clone())?;
    let guard = GeometryGuard::new(&graph, &baseline)?;
    points.insert(ids[1], Point::new(1.15, 0.));
    assert!(guard.permits(&Layout2d::from_positions(points.clone())?));
    points.insert(ids[1], Point::new(1.3, 0.));
    assert!(!guard.permits(&Layout2d::from_positions(points)?));
    Ok(())
  }

  #[test]
  fn chain_angles_may_improve_but_should_not_worsen() -> TestResult {
    let graph = read_smiles("CCC")?.into_graph();
    let right = coordinates(&graph, &[(1., 0.), (0., 0.), (0., 1.)])?;
    let ideal = coordinates(&graph, &[(1., 0.), (0., 0.), (-0.5, 3_f64.sqrt() / 2.)])?;
    assert!(GeometryGuard::new(&graph, &right)?.permits(&ideal));
    assert!(!GeometryGuard::new(&graph, &ideal)?.permits(&right));
    Ok(())
  }

  #[test]
  fn a_small_ring_should_not_be_sheared_while_preserving_edge_lengths() -> TestResult {
    let graph = read_smiles("C1CCC1")?.into_graph();
    let square = coordinates(&graph, &[(0., 0.), (1., 0.), (1., 1.), (0., 1.)])?;
    let rhombus = coordinates(
      &graph,
      &[(0., 0.), (1., 0.), (1.5, 3_f64.sqrt() / 2.), (0.5, 3_f64.sqrt() / 2.)],
    )?;
    assert!(!GeometryGuard::new(&graph, &square)?.permits(&rhombus));
    Ok(())
  }

  #[test]
  fn carbonyl_angles_should_not_rotate_to_clear_a_contact() -> TestResult {
    let graph = read_smiles("CC(=O)C")?.into_graph();
    let original = layout::generate(&graph, &LayoutOptions::default())?;
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    let mut points = original.positions().clone();
    let center = points[&ids[1]];
    let oxygen = points[&ids[2]];
    let (sin, cos) = (std::f64::consts::PI / 6.).sin_cos();
    points.insert(
      ids[2],
      center.translated(
        (oxygen.x - center.x) * cos - (oxygen.y - center.y) * sin,
        (oxygen.x - center.x) * sin + (oxygen.y - center.y) * cos,
      ),
    );
    assert!(!GeometryGuard::new(&graph, &original)?.permits(&Layout2d::from_positions(points)?));
    Ok(())
  }

  #[test]
  fn rigid_reflection_should_preserve_alkenes_but_one_sided_reflection_should_fail() -> TestResult {
    let graph = read_smiles("F/C=C/F")?.into_graph();
    let original = layout::generate(&graph, &LayoutOptions::default())?;
    let guard = GeometryGuard::new(&graph, &original)?;
    let reflected = Layout2d::from_positions(
      original
        .positions()
        .iter()
        .map(|(&id, p)| (id, Point::new(-p.x, p.y)))
        .collect(),
    )?;
    assert!(guard.permits(&reflected));
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    let mut points = original.positions().clone();
    let [a, b] = [points[&ids[1]], points[&ids[2]]];
    let p = points[&ids[3]];
    let (ux, uy) = ((b.x - a.x) / a.distance(b), (b.y - a.y) / a.distance(b));
    let along = (p.x - a.x) * ux + (p.y - a.y) * uy;
    points.insert(
      ids[3],
      a.translated(2. * along * ux - (p.x - a.x), 2. * along * uy - (p.y - a.y)),
    );
    assert!(!guard.permits(&Layout2d::from_positions(points)?));
    Ok(())
  }
}
