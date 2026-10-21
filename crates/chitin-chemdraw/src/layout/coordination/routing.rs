//! Anchored covalent-backbone walks around immutable coordination connections.
//!
//! Ring junctions attached to the remaining complex are anchors. Pendant groups
//! belong to their backbone atom and follow its local frame, so rerouting an
//! amide moves its carbonyl oxygen as well as its skeleton vertex. Every proposal
//! is checked against the complete original distance graph before publication.

use super::{Budget, CoordinationError, CoordinationLayoutError, Point, closed, composition, quality};
use crate::layout::{LayoutError, rings::Work, walks};
use chitin_chem::{AtomId, BondOrder, MolecularGraph, perception::analyze_rings};
use std::collections::{BTreeMap, BTreeSet};

/// One covalent ring with its complete off-ring ownership and fixed junctions.
struct Backbone {
  /// Consecutive dense source rows; the closing vertex is not repeated.
  cycle: Vec<usize>,
  /// A closed covalent ring or an open connector between rigid junctions.
  closed: bool,
  /// Complete rigid pendant groups indexed by their owning source row.
  pendants: Vec<Vec<usize>>,
  /// Junctions connected to the rest of the complex cannot be path interiors.
  anchors: Vec<bool>,
}

/// Immutable source topology shared by all bounded routing trials.
pub(super) struct Model {
  /// Covalent macrocycles and open linkers, never rings manufactured by metal edges.
  backbones: Vec<Backbone>,
  /// Full source skeleton, including every explicitly declared attachment.
  edges: Vec<[usize; 2]>,
  /// Triple/cumulated-double centers cannot receive a bent closure.
  linear: Vec<bool>,
  /// Local bends at covalent multiple-bond centers are hard drawing constraints.
  protected: Vec<bool>,
  /// Metal position is never moved or assigned a stereochemical configuration.
  center: usize,
}

impl Model {
  /// Captures movable pendant ownership while retaining multiply anchored groups.
  ///
  /// Ring discovery uses the detached covalent graph. Off-ring components are
  /// explored in the full source graph: reaching the center or another ring
  /// junction fixes that attachment instead of silently cutting a chelate.
  ///
  /// # Parameters
  /// - `graph`, `ligand`: full source and detached covalent topology with shared handles.
  /// - `ids`, `center`: dense source order and the fixed acceptor row.
  /// - `budget`: independent optional discovery/search allowance.
  ///
  /// # Returns
  /// Complete discovered regions; exhaustion declines undiscovered regions.
  /// The calling profile has already rejected supplied stereochemistry.
  ///
  /// # Errors
  /// Propagates inconsistent source handles or an invalid simple-ring traversal.
  pub(super) fn new(
    graph: &MolecularGraph,
    ligand: &MolecularGraph,
    ids: &[AtomId],
    center: usize,
    budget: &mut Budget,
  ) -> Result<Self, CoordinationLayoutError> {
    let mut model = Self {
      backbones: Vec::new(),
      edges: Vec::new(),
      linear: vec![false; ids.len()],
      protected: vec![false; ids.len()],
      center,
    };
    if !budget.charge(ids.len() + graph.bond_count()) {
      return Ok(model);
    }
    let rows: BTreeMap<_, _> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut neighbors = vec![Vec::new(); ids.len()];
    let mut doubles = vec![0; ids.len()];
    for (_, bond) in graph.bonds() {
      let [a, b] = bond.atoms().map(|id| rows[&id]);
      neighbors[a].push(b);
      neighbors[b].push(a);
      model.edges.push([a, b]);
      for row in [a, b] {
        if a != center && b != center {
          model.protected[row] |= bond.order() != BondOrder::Single;
        }
        model.linear[row] |= bond.order() == BondOrder::Triple;
        doubles[row] += usize::from(bond.order() == BondOrder::Double);
      }
    }
    for (row, &count) in doubles.iter().enumerate() {
      model.linear[row] |= count == 2;
    }
    let topology = analyze_rings(ligand).map_err(CoordinationError::from)?;
    let ring_rows: BTreeSet<_> = topology
      .blocks()
      .iter()
      .filter(|block| block.cycle_rank() > 0)
      .flat_map(|block| block.atoms().iter().map(|id| rows[id]))
      .collect();
    let junctions: Vec<_> = neighbors
      .iter()
      .enumerate()
      .map(|(i, row)| row.len() != 2 || ring_rows.contains(&i) || model.protected[i] || i == center)
      .collect();
    let mut seen_edges = BTreeSet::new();
    for start in 0..ids.len() {
      if !junctions[start] || start == center {
        continue;
      }
      for &next in &neighbors[start] {
        if ring_rows.contains(&start) && ring_rows.contains(&next) {
          continue;
        }
        if seen_edges.contains(&(start.min(next), start.max(next))) {
          continue;
        }
        let mut path = vec![start];
        let (mut previous, mut row) = (start, next);
        loop {
          if !budget.charge(1 + neighbors[row].len()) {
            return Ok(model);
          }
          seen_edges.insert((previous.min(row), previous.max(row)));
          path.push(row);
          if junctions[row] {
            break;
          }
          let Some(&other) = neighbors[row].iter().find(|&&other| other != previous) else {
            break;
          };
          (previous, row) = (row, other);
        }
        if (4..=24).contains(&path.len()) && row != center && row != start {
          let Some(backbone) = Backbone::new(path, false, &neighbors, center, budget) else {
            return Ok(model);
          };
          model.backbones.push(backbone);
        }
      }
    }
    for block in topology
      .blocks()
      .iter()
      .filter(|block| block.cycle_rank() == 1 && (9..=96).contains(&block.atoms().len()))
    {
      if !budget.charge(ids.len() + block.atoms().len()) {
        break;
      }
      let ring: BTreeSet<_> = block.atoms().iter().map(|id| rows[id]).collect();
      let root = rows[&block.atoms()[0]];
      let mut cycle = vec![root];
      while cycle.len() < ring.len() {
        let row = cycle[cycle.len() - 1];
        let Some(&next) = neighbors[row]
          .iter()
          .find(|&&other| ring.contains(&other) && !cycle.contains(&other))
        else {
          return Err(LayoutError::CoordinateRange.into());
        };
        cycle.push(next);
      }
      let Some(backbone) = Backbone::new(cycle, true, &neighbors, center, budget) else {
        break;
      };
      model.backbones.push(backbone);
    }
    Ok(model)
  }

  /// Replaces only fully closed, contact-safe backbone/pendant proposals.
  ///
  /// All external junctions stay fixed. Exhaustion or an infeasible discrete
  /// walk retains the complete best-so-far input; no partial walk is committed.
  ///
  /// # Parameters
  /// - `points`: complete mutable source geometry, changed only after validation.
  /// - `constraints`: all source distances and protected covalent angle chords.
  /// - `spacing`, `budget`: contact threshold and independent optional work limit.
  ///
  /// # Returns
  /// Success with the best complete pose found, including when work is exhausted.
  ///
  /// # Errors
  /// Propagates non-budget walk-generation failures without committing that trial.
  pub(super) fn improve(
    &self,
    points: &mut Vec<Point>,
    constraints: &[([usize; 2], f64)],
    spacing: f64,
    budget: &mut Budget,
  ) -> Result<(), CoordinationLayoutError> {
    if self.backbones.is_empty() || budget.0 == 0 {
      return Ok(());
    }
    let allowance = budget.0 / 3;
    let mut coupled = Budget(allowance);
    self.compact(points, constraints, spacing, &mut coupled);
    budget.0 -= allowance - coupled.0;
    let Some(mut current) = quality::measure(points, &self.edges, spacing, budget) else {
      return Ok(());
    };
    let mut cost = self.score(points, &current);
    for backbone in &self.backbones {
      if !budget.charge(backbone.cycle.len() * 24 * 24) {
        return Ok(());
      }
      let mut paths = backbone.paths();
      paths.sort_by_key(|path| {
        (
          usize::from(!(backbone.anchors[path[0]] && backbone.anchors[path[path.len() - 1]])),
          usize::from(self.protected[path[path.len() - 2]]),
          std::cmp::Reverse(path.len()),
        )
      });
      for _ in 0..2 {
        let mut changed = false;
        for path in &paths {
          for contraction in [0.4, 0.7, 1.] {
            let allowance = budget.0.min(400_000);
            if allowance == 0 {
              return Ok(());
            }
            let mut work = Work::new(allowance);
            let mut moving = vec![false; points.len()];
            for &row in &path[1..path.len() - 1] {
              moving[row] = true;
              for &other in &backbone.pendants[row] {
                moving[other] = true;
              }
            }
            let mean = Point::new(
              points.iter().map(|p| p.x).sum::<f64>() / points.len() as f64,
              points.iter().map(|p| p.y).sum::<f64>() / points.len() as f64,
            );
            let guide: Vec<_> = points
              .iter()
              .map(|p| {
                Point::new(
                  mean.x + contraction * (p.x - mean.x),
                  mean.y + contraction * (p.y - mean.y),
                )
              })
              .collect();
            // Every proposal in this family owns the same reference frame,
            // even after an earlier alternative has improved the current pose.
            let snapshot = points.clone();
            let trials = walks::guided(
              path,
              &snapshot,
              &self.edges,
              &self.linear,
              walks::Guidance {
                moving: &moving,
                guide: &guide,
                width: 32,
                protected: &self.protected,
                free_heading: true,
              },
              &mut work,
            );
            budget.0 -= allowance - work.remaining();
            let trials = match trials {
              Ok(trials) => trials,
              Err(LayoutError::RingSearchBudgetExceeded) => continue,
              Err(error) => return Err(error.into()),
            };
            for mut trial in trials {
              if !budget.charge(points.len() + constraints.len() + self.edges.len()) {
                return Ok(());
              }
              backbone.follow(path, &snapshot, &mut trial);
              if !closed(&trial, constraints) {
                continue;
              }
              let Some(q) = quality::measure(&trial, &self.edges, spacing, budget) else {
                return Ok(());
              };
              if !budget.charge(q.crowds.len().saturating_mul(current.crowds.len())) {
                return Ok(());
              }
              if !q.crossings.is_subset(&current.crossings)
                || !q.crowds.iter().all(|pair| current.crowds.contains(pair))
              {
                continue;
              }
              let next = self.score(&trial, &q);
              if next + 1e-8 < cost {
                *points = trial;
                current = q;
                cost = next;
                changed = true;
              }
            }
          }
        }
        if !changed {
          break;
        }
      }
    }
    Ok(())
  }

  /// Uses the same complete-geometry objective as coordination pose selection.
  fn score(&self, points: &[Point], q: &quality::Quality) -> f64 {
    q.cost
      + 100. * q.crossings.len() as f64
      + 10. * q.crowds.len() as f64
      + composition(points, &self.edges, self.center)
  }

  /// Relaxes junctions together using only validated, contact-safe closed steps.
  ///
  /// A final contracted state may collide even when earlier states were useful.
  /// Backtracking retains those intermediate improvements rather than throwing
  /// away an entire trajectory, and leaves a separate allowance for rerouting.
  fn compact(&self, points: &mut Vec<Point>, constraints: &[([usize; 2], f64)], spacing: f64, budget: &mut Budget) {
    let mut fixed = vec![false; points.len()];
    fixed[self.center] = true;
    let Some(mut current) = quality::measure(points, &self.edges, spacing, budget) else {
      return;
    };
    let mut cost = self.score(points, &current);
    for _ in 0..64 {
      if !budget.charge(points.len() * (points.len() + self.edges.len())) {
        return;
      }
      let proposed = self.descent(points);
      let mut accepted = false;
      for scale in [1., 0.5, 0.25] {
        let mut trial: Vec<_> = points
          .iter()
          .zip(&proposed)
          .map(|(a, b)| a.translated(scale * (b.x - a.x), scale * (b.y - a.y)))
          .collect();
        if !super::project(&mut trial, constraints, &fixed, 256, budget) {
          return;
        }
        if !closed(&trial, constraints) {
          continue;
        }
        let Some(q) = quality::measure(&trial, &self.edges, spacing, budget) else {
          return;
        };
        if !budget.charge(q.crowds.len().saturating_mul(current.crowds.len())) {
          return;
        }
        if !q.crossings.is_subset(&current.crossings) || !q.crowds.iter().all(|pair| current.crowds.contains(pair)) {
          continue;
        }
        let next = self.score(&trial, &q);
        if next + 1e-8 < cost {
          *points = trial;
          current = q;
          cost = next;
          accepted = true;
          break;
        }
      }
      if !accepted {
        break;
      }
    }
  }

  /// Forms a simultaneous descent direction for covalent angles and mean extent.
  ///
  /// The angle derivative includes both endpoint forces and their balancing
  /// center force. Using this objective's derivative avoids a separate spring
  /// ideal that can oppose compactness despite a lower complete drawing score.
  fn descent(&self, points: &[Point]) -> Vec<Point> {
    let n = points.len() as f64;
    let mean = Point::new(
      points.iter().map(|p| p.x).sum::<f64>() / n,
      points.iter().map(|p| p.y).sum::<f64>() / n,
    );
    let mut shifts: Vec<_> = points
      .iter()
      .map(|p| {
        Point::new(
          2. * super::EXTENT_WEIGHT / n * (mean.x - p.x),
          2. * super::EXTENT_WEIGHT / n * (mean.y - p.y),
        )
      })
      .collect();
    let mut neighbors = vec![Vec::new(); points.len()];
    for &[a, b] in &self.edges {
      if a != self.center && b != self.center {
        neighbors[a].push(b);
        neighbors[b].push(a);
      }
    }
    for (row, adjacent) in neighbors.iter().enumerate() {
      if !(2..=3).contains(&adjacent.len()) {
        continue;
      }
      for (i, &a) in adjacent.iter().enumerate() {
        for &b in &adjacent[i + 1..] {
          let u = Point::new(points[a].x - points[row].x, points[a].y - points[row].y);
          let v = Point::new(points[b].x - points[row].x, points[b].y - points[row].y);
          let ru = u.x.hypot(u.y);
          let rv = v.x.hypot(v.y);
          if ru < 1e-8 || rv < 1e-8 {
            continue;
          }
          let cosine = ((u.x * v.x + u.y * v.y) / (ru * rv)).clamp(-1., 1.);
          let sine = (1. - cosine * cosine).sqrt();
          if sine < 1e-6 {
            continue;
          }
          let scale = 2. * super::ANGLE_WEIGHT * (cosine.acos() - std::f64::consts::TAU / 3.) / sine;
          let da = Point::new(
            scale * (v.x / (ru * rv) - cosine * u.x / (ru * ru)),
            scale * (v.y / (ru * rv) - cosine * u.y / (ru * ru)),
          );
          let db = Point::new(
            scale * (u.x / (ru * rv) - cosine * v.x / (rv * rv)),
            scale * (u.y / (ru * rv) - cosine * v.y / (rv * rv)),
          );
          shifts[a] = shifts[a].translated(da.x, da.y);
          shifts[b] = shifts[b].translated(db.x, db.y);
          shifts[row] = shifts[row].translated(-da.x - db.x, -da.y - db.y);
        }
      }
    }
    points
      .iter()
      .zip(shifts)
      .enumerate()
      .map(|(i, (p, d))| if i == self.center { *p } else { p.translated(d.x, d.y) })
      .collect()
  }
}

impl Backbone {
  /// Assigns a pendant only if its complete component has exactly one ring owner.
  fn new(
    cycle: Vec<usize>,
    closed: bool,
    neighbors: &[Vec<usize>],
    center: usize,
    budget: &mut Budget,
  ) -> Option<Self> {
    if !budget.charge(neighbors.len() + cycle.len()) {
      return None;
    }
    let mut seen = vec![false; neighbors.len()];
    let mut pendants = vec![Vec::new(); neighbors.len()];
    let mut anchors = vec![false; neighbors.len()];
    for &row in &cycle {
      seen[row] = true;
    }
    let on_path = seen.clone();
    for root in 0..neighbors.len() {
      if seen[root] {
        continue;
      }
      let mut stack = vec![root];
      let mut component = Vec::new();
      let mut touches = BTreeSet::new();
      seen[root] = true;
      while let Some(row) = stack.pop() {
        if !budget.charge(1 + neighbors[row].len()) {
          return None;
        }
        component.push(row);
        for &other in &neighbors[row] {
          if on_path[other] {
            touches.insert(other);
          }
          if !seen[other] {
            seen[other] = true;
            stack.push(other);
          }
        }
      }
      if touches.len() == 1 && !component.contains(&center) {
        if let Some(&owner) = touches.first() {
          pendants[owner].extend(component);
        }
      } else {
        for row in touches {
          anchors[row] = true;
        }
      }
    }
    Some(Self {
      cycle,
      closed,
      pendants,
      anchors,
    })
  }

  /// Prioritizes complete anchor-to-anchor arcs before smaller overlapping windows.
  fn paths(&self) -> Vec<Vec<usize>> {
    if !self.closed {
      let mut reverse = self.cycle.clone();
      reverse.reverse();
      return vec![self.cycle.clone(), reverse];
    }
    let mut paths = Vec::new();
    for start in 0..self.cycle.len() {
      for length in (4..=self.cycle.len().min(24)).rev() {
        let path: Vec<_> = (0..length)
          .map(|i| self.cycle[(start + i) % self.cycle.len()])
          .collect();
        if path[1..length - 1].iter().any(|&row| self.anchors[row]) {
          continue;
        }
        paths.push(path);
        let mut reverse = paths[paths.len() - 1].clone();
        reverse.reverse();
        paths.push(reverse);
      }
    }
    paths.sort_by_key(|path| {
      (
        std::cmp::Reverse(usize::from(self.anchors[path[0]] && self.anchors[path[path.len() - 1]])),
        std::cmp::Reverse(path.len()),
      )
    });
    paths
  }

  /// Carries each pendant through the old/new local backbone frame isometry.
  fn follow(&self, path: &[usize], old: &[Point], new: &mut [Point]) {
    for triple in path.windows(3) {
      let [a, row, b] = [triple[0], triple[1], triple[2]];
      let from = Point::new(old[a].x - old[row].x, old[a].y - old[row].y);
      let to = Point::new(new[a].x - new[row].x, new[a].y - new[row].y);
      let angle = to.y.atan2(to.x) - from.y.atan2(from.x);
      let (sin, cos) = angle.sin_cos();
      let old_side = from.x * (old[b].y - old[row].y) - from.y * (old[b].x - old[row].x);
      let new_side = to.x * (new[b].y - new[row].y) - to.y * (new[b].x - new[row].x);
      let norm = from.x.hypot(from.y);
      for &other in &self.pendants[row] {
        let mut p = Point::new(old[other].x - old[row].x, old[other].y - old[row].y);
        if old_side * new_side < 0. && norm > 1e-8 {
          let dot = (p.x * from.x + p.y * from.y) / (norm * norm);
          p = Point::new(2. * dot * from.x - p.x, 2. * dot * from.y - p.y);
        }
        new[other] = new[row].translated(p.x * cos - p.y * sin, p.x * sin + p.y * cos);
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use chitin_chem::{Element, io::smiles::read_smiles};
  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn peptide_topology_should_keep_three_metal_connected_anchors_and_three_open_linkers() -> TestResult {
    let graph =
      read_smiles(include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/iron_chelate.smi")).trim())?
        .into_graph();
    let original = graph.clone();
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    let center = graph
      .atoms()
      .position(|(_, atom)| atom.element() == Element::Iron)
      .ok_or("missing center")?;
    let mut ligand = graph.clone();
    ligand.remove_atom(ids[center])?;
    let model = Model::new(&graph, &ligand, &ids, center, &mut Budget(1_000_000))?;
    assert_eq!(model.backbones.iter().filter(|region| !region.closed).count(), 3);
    let ring = model
      .backbones
      .iter()
      .find(|region| region.closed)
      .ok_or("missing macrocycle")?;
    assert_eq!(ring.anchors.iter().filter(|&&fixed| fixed).count(), 3);
    assert!(ring.pendants.iter().all(|branch| !branch.contains(&center)));
    assert!(
      ring
        .paths()
        .iter()
        .all(|path| path[1..path.len() - 1].iter().all(|&row| !ring.anchors[row]))
    );
    assert_eq!(graph, original);
    Ok(())
  }

  #[test]
  fn routing_should_leave_complete_geometry_unchanged_when_disabled_or_exhausted() -> TestResult {
    let graph = read_smiles("O1CCCCCCCCCCCO[Fe]1O")?.into_graph();
    let ids: Vec<_> = graph.atoms().map(|(id, _)| id).collect();
    let center = graph
      .atoms()
      .position(|(_, atom)| atom.element() == Element::Iron)
      .ok_or("missing center")?;
    let mut ligand = graph.clone();
    ligand.remove_atom(ids[center])?;
    let model = Model::new(&graph, &ligand, &ids, center, &mut Budget(100_000))?;
    let before: Vec<_> = (0..ids.len()).map(|i| Point::new(i as f64, 0.)).collect();
    for allowance in [0, 1] {
      let mut points = before.clone();
      model.improve(&mut points, &[], 0.55, &mut Budget(allowance))?;
      assert_eq!(points, before);
    }
    Ok(())
  }

  #[test]
  fn composition_descent_should_match_its_finite_difference_gradient() {
    let model = Model {
      backbones: Vec::new(),
      edges: vec![[0, 1], [1, 2]],
      linear: vec![false; 4],
      protected: vec![false; 4],
      center: 3,
    };
    let points = vec![
      Point::new(0., 0.),
      Point::new(1., 0.),
      Point::new(1.4, 0.9),
      Point::new(0., 3.),
    ];
    let proposal = model.descent(&points);
    let epsilon = 1e-6;
    for row in 0..3 {
      for axis in 0..2 {
        let mut plus = points.clone();
        let mut minus = points.clone();
        if axis == 0 {
          plus[row].x += epsilon;
          minus[row].x -= epsilon;
        } else {
          plus[row].y += epsilon;
          minus[row].y -= epsilon;
        }
        let derivative = (composition(&plus, &model.edges, model.center)
          - composition(&minus, &model.edges, model.center))
          / (2. * epsilon);
        let displacement = if axis == 0 {
          proposal[row].x - points[row].x
        } else {
          proposal[row].y - points[row].y
        };
        assert!(
          (displacement + derivative).abs() < 1e-6,
          "row {row}, axis {axis}: {displacement} != {}",
          -derivative
        );
      }
    }
    assert_eq!(proposal[3], points[3]);
  }
}
