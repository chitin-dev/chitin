//! Whole-block seeds and closure for rigid islands joined by folded ring paths.
//!
//! Short-cycle chords and explicit path ownership avoid assuming that faces
//! share just one edge. Distance seeds place junctions before discrete folds
//! search flexible paths. Every source edge participates in exact closure. This is a
//! drawing objective, not a force field, stereo inference, or energy estimate.

use super::{Error, Point, Work, constraints::Constraints, folding::Plan};
use chitin_chem::{AtomId, BondOrder, MolecularGraph, perception::RingBlock};
use std::{
  collections::{BTreeMap, BTreeSet, VecDeque},
  f64::consts::{PI, TAU},
};

/// A complete local system with global rows retained only at the boundary.
pub(super) struct General {
  /// Global dense rows in stable source order.
  atoms: Vec<usize>,
  /// All source edges, using block-local rows.
  edges: Vec<[usize; 2]>,
  /// Pairwise target distances and relative drawing weights.
  pairs: Vec<Vec<(f64, f64)>>,
  /// Complete shortest-path distances for deterministic spectral seeding.
  distances: Vec<Vec<f64>>,
  /// Largest selected cycle, used only for seed priority.
  largest: usize,
  /// Complete unit-closed candidates, prepared once under the shared allowance.
  poses: Vec<Vec<Point>>,
  /// Number of leading poses produced by compact folding.
  folded: usize,
  /// Single outer boundary when cycle-space cancellation yields a simple walk.
  perimeter: Vec<usize>,
  /// Outside neighbor counts reserve space for crowded geminal substituents.
  attachments: Vec<usize>,
  /// Rigid aromatic attachments, explicit H and geminal groups reserve space.
  reserve_space: Vec<bool>,
  /// Triple/cumulated-double centers retain linear local geometry.
  linear: Vec<bool>,
  /// Supplied aromatic small rings keep regular chords, not flattened zigzags.
  rigid_chords: Vec<([usize; 2], f64)>,
  /// Source-edge ownership and rigid-island guards for explicit path folding.
  folding: Plan,
}

/// Projects one distance equation symmetrically.
fn project(points: &mut [Point], [a, b]: [usize; 2], target: f64) {
  let length = points[a].distance(points[b]);
  if !length.is_finite() || length < 1e-12 {
    return;
  }
  let correction = (length - target) / length * 0.5;
  let (dx, dy) = (
    (points[b].x - points[a].x) * correction,
    (points[b].y - points[a].y) * correction,
  );
  points[a] = points[a].translated(dx, dy);
  points[b] = points[b].translated(-dx, -dy);
}

impl General {
  /// Recognizes mixed multi-ring blocks containing an eight-or-larger cycle.
  ///
  /// # Parameters
  /// - `graph`, `block`, `indices`: immutable source topology and global rows.
  /// - `source`: supplied aromatic markers before integer-bond resolution.
  /// - `work`: shared cycle-discovery, matrix, and search allowance.
  ///
  /// # Returns
  /// A whole-block drawing model for rank three or greater. Simple rings,
  /// known cages and small regular fused systems retain their existing paths.
  /// Cyclic E/Z constraints remain with their dedicated supported models.
  ///
  /// # Errors
  /// Source-reference errors and budget exhaustion propagate before placement.
  pub(super) fn new(
    graph: &MolecularGraph,
    source: &MolecularGraph,
    block: &RingBlock,
    indices: &BTreeMap<AtomId, usize>,
    work: &mut Work,
  ) -> Result<Option<Self>, Error> {
    if block.cycle_rank() < 3 {
      return Ok(None);
    }
    let cycles = Constraints::cycles(graph, block, indices, work)?;
    let largest = cycles.iter().map(Vec::len).max().unwrap_or(0);
    if largest < 8
      || graph
        .double_bond_stereos()
        .any(|(_, stereo)| block.atoms().contains(&stereo.ends()[0]))
    {
      return Ok(None);
    }
    let atoms: Vec<_> = block.atoms().iter().map(|id| indices[id]).collect();
    let local: BTreeMap<_, _> = atoms.iter().enumerate().map(|(i, &row)| (row, i)).collect();
    let n = atoms.len();
    let mut attachments = Vec::with_capacity(n);
    let mut reserve_space = Vec::with_capacity(n);
    for &id in block.atoms() {
      let neighbors = graph.neighbors(id)?;
      work.charge(neighbors.len())?;
      let outside: Vec<_> = neighbors.filter(|(other, _)| !block.atoms().contains(other)).collect();
      reserve_space.push(
        outside.len() > 1
          || (!outside.is_empty() && source.atom(id).is_some_and(|atom| atom.is_aromatic()))
          || outside.iter().any(|&(id, _)| {
            graph
              .atom(id)
              .is_some_and(|atom| atom.element() == chitin_chem::Element::Hydrogen)
          }),
      );
      attachments.push(outside.len());
    }
    work.charge(n * n)?;
    let mut rows = vec![Vec::new(); n];
    let mut edges = Vec::new();
    for &id in block.bonds() {
      let edge = graph.bond(id).ok_or(chitin_chem::GraphError::MissingBond(id))?;
      let [a, b] = edge.atoms().map(|id| local[&indices[&id]]);
      rows[a].push(b);
      rows[b].push(a);
      edges.push([a, b]);
    }
    let mut distances = vec![vec![f64::INFINITY; n]; n];
    for (start, distance) in distances.iter_mut().enumerate() {
      distance[start] = 0.;
      let mut queue = VecDeque::from([start]);
      while let Some(node) = queue.pop_front() {
        work.charge(1 + rows[node].len())?;
        for &other in &rows[node] {
          if !distance[other].is_finite() {
            distance[other] = distance[node] + 1.;
            queue.push_back(other);
          }
        }
      }
    }
    let mut pairs: Vec<Vec<_>> = distances
      .iter()
      .map(|row| {
        row
          .iter()
          .map(|&d| {
            if d == 0. {
              (0., 0.)
            } else if d == 1. {
              (1., 64.)
            } else {
              (d * 1.1, 0.5 / (d * d))
            }
          })
          .collect()
      })
      .collect();
    let mut rigid_chords = Vec::new();
    for cycle in cycles.iter().filter(|cycle| cycle.len() <= 6) {
      // Integer-bond resolution removes aromatic flags, but preserves handles.
      // Protect only the supplied aromatic islands; layout never infers them.
      let aromatic = cycle.iter().all(|row| {
        source
          .atom(block.atoms()[local[row]])
          .is_some_and(|atom| atom.is_aromatic())
      });
      for (i, &a) in cycle.iter().enumerate() {
        for (j, &b) in cycle.iter().enumerate().skip(i + 1) {
          work.charge(1)?;
          let k = (j - i).min(cycle.len() - (j - i));
          let target = (PI * k as f64 / cycle.len() as f64).sin() / (PI / cycle.len() as f64).sin();
          let (a, b) = (local[&a], local[&b]);
          if distances[a][b] > 1. {
            pairs[a][b] = (target, 16.);
            pairs[b][a] = (target, 16.);
            if aromatic {
              rigid_chords.push(([a, b], target));
            }
          }
        }
      }
    }
    let perimeter = perimeter(&cycles).into_iter().map(|row| local[&row]).collect();
    let mut linear = Vec::with_capacity(n);
    for &id in block.atoms() {
      let neighbors = graph.neighbors(id)?;
      work.charge(neighbors.len())?;
      let orders: Vec<_> = neighbors
        .map(|(_, id)| {
          graph
            .bond(id)
            .map(|bond| bond.order())
            .ok_or(chitin_chem::GraphError::MissingBond(id))
        })
        .collect::<Result<_, _>>()?;
      linear.push(
        orders.len() == 2
          && (orders.contains(&BondOrder::Triple) || orders.iter().all(|&order| order == BondOrder::Double)),
      );
    }
    let local_cycles: Vec<_> = cycles
      .iter()
      .map(|cycle| cycle.iter().map(|row| local[row]).collect())
      .collect();
    let folding_linear = linear.clone();
    let folding = Plan::new(&edges, &local_cycles, linear, work)?;
    let mut model = Self {
      atoms,
      edges,
      pairs,
      distances,
      largest,
      poses: Vec::new(),
      folded: 0,
      perimeter,
      attachments,
      reserve_space,
      linear: folding_linear,
      rigid_chords,
      folding,
    };
    model.poses = model.candidates(work, block.atoms()[0], block.cycle_rank())?;
    // Folding is attempted only with a reserved remainder, so an ambitious
    // trial can never starve the mandatory placement that follows. Several
    // block seeds are folded because the best block-only pose is not
    // necessarily the one that leaves room for external branches.
    if work.remaining() >= 120_000 {
      let bases: Vec<Vec<Point>> = model.poses.iter().take(4).cloned().collect();
      let mut folded = Vec::new();
      for base in bases {
        let before = model.pose_cost(&base);
        match model.fold(&base, 0.999, work) {
          Ok(Some(points)) => {
            let after = model.pose_cost(&points);
            if after + 1e-9 < before {
              folded.push((after, points));
            }
          }
          Ok(None) | Err(Error::RingSearchBudgetExceeded) => {}
          Err(error) => return Err(error),
        }
      }
      folded.sort_by(|a, b| a.0.total_cmp(&b.0));
      model.folded = folded.len();
      for (_, points) in folded.into_iter().rev() {
        model.poses.insert(0, points);
      }
    }
    Ok(Some(model))
  }

  /// Returns the largest basis cycle for placement priority.
  pub(super) fn len(&self) -> usize {
    self.largest
  }

  /// Returns the finite number of whole-block seed alternatives.
  pub(super) fn variants(&self) -> usize {
    self.poses.len()
  }

  /// Returns how many leading alternatives are compact folded poses.
  pub(super) fn folded_variants(&self) -> usize {
    self.folded
  }

  /// Searches whole-block snapshots and preserves an existing articulation.
  ///
  /// # Returns
  /// Complete coordinates and largest free angular sectors for attached branches.
  /// Bond projection verifies every source edge; no closure is dropped and no
  /// crossing receives an invented front/back assignment. Final inspection
  /// continues to report any unresolved crossing or crowding.
  ///
  /// # Errors
  /// Multiple entry anchors, failed finite unit closure, or exhausted work are
  /// explicit failures, never permission to return a partial or stretched ring.
  pub(super) fn embed(
    &self,
    old: &[Option<Point>],
    work: &mut Work,
    source: AtomId,
    rank: usize,
    variant: usize,
  ) -> Result<Vec<(usize, Point, f64)>, Error> {
    let unsupported = || Error::RingSystem {
      atom: source,
      cycles: rank,
    };
    let anchors: Vec<_> = self
      .atoms
      .iter()
      .enumerate()
      .filter_map(|(i, &row)| old[row].map(|p| (i, p)))
      .collect();
    if anchors.len() > 1 {
      return Err(unsupported());
    }
    work.charge(self.atoms.len())?;
    let mut points = self
      .poses
      .get(variant % self.poses.len())
      .ok_or_else(unsupported)?
      .clone();
    let n = points.len();
    if let Some(&(anchor, target)) = anchors.first() {
      let origin = points[anchor];
      for point in &mut points {
        *point = point.translated(target.x - origin.x, target.y - origin.y);
      }
      points[anchor] = target;
    }
    let mut rows = vec![Vec::new(); n];
    for &[a, b] in &self.edges {
      rows[a].push(b);
      rows[b].push(a);
    }
    Ok(
      self
        .atoms
        .iter()
        .enumerate()
        .map(|(i, &row)| {
          let origin = points[i];
          let mut angles: Vec<_> = rows[i]
            .iter()
            .map(|&j| (points[j].y - origin.y).atan2(points[j].x - origin.x))
            .collect();
          angles.sort_by(f64::total_cmp);
          let mut gap = (0., 0.);
          for j in 0..angles.len() {
            let size = (angles[(j + 1) % angles.len()] - angles[j]).rem_euclid(TAU);
            if size > gap.0 {
              gap = (size, angles[j] + size / 2.);
            }
          }
          (row, origin, gap.1)
        })
        .collect(),
    )
  }

  /// Builds and ranks finite, unit-closed seeds before external branches exist.
  fn candidates(&self, work: &mut Work, source: AtomId, rank: usize) -> Result<Vec<Vec<Point>>, Error> {
    let seed = self.seed(work)?;
    let n = seed.len();
    let mut candidates = Vec::new();
    for attempt in 0..if n > 32 { 4 } else { 8 } {
      let mut points: Vec<_> = seed
        .iter()
        .enumerate()
        .map(|(i, p)| {
          let angle = TAU * i as f64 / n as f64;
          match attempt {
            0 => *p,
            1 => Point::new(angle.cos() * (n as f64).sqrt(), angle.sin() * (n as f64).sqrt()),
            _ => p.translated(
              (attempt as f64 * 0.5) * (angle * 3.).sin(),
              (attempt as f64 * 0.5) * (angle * 5.).cos(),
            ),
          }
        })
        .collect();
      if attempt == 1 && !self.perimeter.is_empty() {
        let m = self.perimeter.len();
        let radius = 1. / (2. * (PI / m as f64).sin());
        for (i, point) in points.iter_mut().enumerate() {
          *point = seed[i].translated(0.07 * (i as f64).cos(), 0.07 * (i as f64).sin());
        }
        for (i, &row) in self.perimeter.iter().enumerate() {
          let angle = TAU * i as f64 / m as f64;
          points[row] = Point::new(radius * angle.cos(), radius * angle.sin());
        }
      }
      self.relax(&mut points, work)?;
      if !self.close(&mut points, work)? {
        continue;
      }
      let quality = work.measure(&points, &self.edges)?;
      let actual = quality.cost + self.folding.turn_cost(&points);
      let folded = self.folding.fold(&points, work)?;
      if folded != points {
        let quality = work.measure(&folded, &self.edges)?;
        candidates.push((quality.cost + self.folding.turn_cost(&folded), folded));
      }
      candidates.push((actual, points));
    }
    if candidates.is_empty() {
      return Err(Error::RingSystem {
        atom: source,
        cycles: rank,
      });
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(candidates.into_iter().map(|(_, points)| points).collect())
  }

  /// Initializes two principal axes from the centered squared-distance matrix.
  fn seed(&self, work: &mut Work) -> Result<Vec<Point>, Error> {
    let n = self.atoms.len();
    work.charge(n * n)?;
    let means: Vec<_> = self
      .distances
      .iter()
      .map(|row| row.iter().map(|d| d * d).sum::<f64>() / n as f64)
      .collect();
    let mean = means.iter().sum::<f64>() / n as f64;
    let matrix: Vec<Vec<_>> = self
      .distances
      .iter()
      .enumerate()
      .map(|(i, row)| {
        row
          .iter()
          .enumerate()
          .map(|(j, d)| -0.5 * (d * d - means[i] - means[j] + mean))
          .collect()
      })
      .collect();
    let mut axes: Vec<Vec<f64>> = Vec::new();
    for axis in 0..2 {
      let mut vector: Vec<_> = (0..n)
        .map(|i| ((i + 1) as f64 * (axis + 1) as f64 * 1.618).sin())
        .collect();
      for _ in 0..40 {
        work.charge(n * n)?;
        let mut next: Vec<_> = matrix
          .iter()
          .map(|row| row.iter().zip(&vector).map(|(a, b)| a * b).sum::<f64>())
          .collect();
        if let Some(first) = axes.first() {
          let dot = next.iter().zip(first).map(|(a, b)| a * b).sum::<f64>() / first.iter().map(|v| v * v).sum::<f64>();
          for (v, u) in next.iter_mut().zip(first) {
            *v -= dot * u;
          }
        }
        let norm = next.iter().map(|v| v * v).sum::<f64>().sqrt().max(1e-10);
        for v in &mut next {
          *v /= norm;
        }
        vector = next;
      }
      let value = matrix
        .iter()
        .zip(&vector)
        .map(|(row, v)| v * row.iter().zip(&vector).map(|(a, b)| a * b).sum::<f64>())
        .sum::<f64>()
        .max(1e-8)
        .sqrt();
      axes.push(vector.iter().map(|v| v * value).collect());
    }
    Ok((0..n).map(|i| Point::new(axes[0][i], axes[1][i])).collect())
  }

  /// Bounded weighted stress sweeps protect short-ring chords and separate centers.
  fn relax(&self, points: &mut [Point], work: &mut Work) -> Result<(), Error> {
    let n = points.len();
    for _ in 0..90 {
      work.charge(n * n)?;
      for i in 0..n {
        let origin = points[i];
        let mut sum = Point::new(0., 0.);
        let mut total = 0.;
        for (j, &point) in points.iter().enumerate() {
          if i == j {
            continue;
          }
          let (mut target, mut weight) = self.pairs[i][j];
          let distance = origin.distance(point).max(1e-8);
          if self.distances[i][j] > 1. && distance < 0.95 {
            target = 0.95;
            weight = weight.max(32.);
          }
          sum.x += weight * (point.x + target * (origin.x - point.x) / distance);
          sum.y += weight * (point.y + target * (origin.y - point.y) / distance);
          total += weight;
        }
        points[i] = Point::new(sum.x / total, sum.y / total);
      }
      self.clear_attachments(points, work)?;
    }
    Ok(())
  }

  /// Reserves first-shell space for attachments with constrained exit directions.
  ///
  /// Proxies affect the seed only; they never become atoms or bonds. Final
  /// candidate selection evaluates the actual expanded graph, not these hints.
  fn clear_attachments(&self, points: &mut [Point], work: &mut Work) -> Result<(), Error> {
    if !self.reserve_space.iter().any(|&reserve| reserve) {
      return Ok(());
    }
    work.charge(points.len() + self.edges.len())?;
    let mut angles = vec![Vec::new(); points.len()];
    for &[a, b] in &self.edges {
      angles[a].push((points[b].y - points[a].y).atan2(points[b].x - points[a].x));
      angles[b].push((points[a].y - points[b].y).atan2(points[a].x - points[b].x));
    }
    let mut ghosts = Vec::new();
    for (i, &count) in self.attachments.iter().enumerate() {
      angles[i].sort_by(f64::total_cmp);
      let mut gap = (0., 0.);
      for j in 0..angles[i].len() {
        let size = (angles[i][(j + 1) % angles[i].len()] - angles[i][j]).rem_euclid(TAU);
        if size > gap.0 {
          gap = (size, angles[i][j] + size / 2.);
        }
      }
      for j in 0..count {
        let angle = gap.1 + (j as f64 - (count - 1) as f64 / 2.) * PI / 3.;
        ghosts.push((i, points[i].translated(angle.cos(), angle.sin())));
      }
    }
    let mut shifts = vec![Point::new(0., 0.); points.len()];
    for &(host, p) in &ghosts {
      if !self.reserve_space[host] {
        continue;
      }
      work.charge(points.len() + ghosts.len())?;
      for (other, q) in points.iter().copied().enumerate().chain(ghosts.iter().copied()) {
        if other == host {
          continue;
        }
        let distance = p.distance(q).max(1e-8);
        if distance < 1.1 {
          let scale = 0.08 * (1.1 - distance) / distance;
          let delta = Point::new((p.x - q.x) * scale, (p.y - q.y) * scale);
          shifts[host] = shifts[host].translated(delta.x, delta.y);
          shifts[other] = shifts[other].translated(-delta.x, -delta.y);
        }
      }
    }
    for (point, shift) in points.iter_mut().zip(shifts) {
      *point = point.translated(shift.x, shift.y);
    }
    Ok(())
  }

  /// Folds a complete block into a compact, zigzag-like pose by projection.
  ///
  /// Bonds and every rigid-island pair stay exact distance equations. Flexible
  /// centers additionally prefer a sixty-degree chord between their neighbors,
  /// which is the same statement as a 120-degree bond angle once bonds are unit.
  /// A weak contraction couples the equations: the closed ring cannot shrink, so
  /// the only way to satisfy contraction plus angles is to fold its flexible runs
  /// into alternating turns. Contacts are resisted and the finished snapshot is
  /// validated; infeasible or tangled trials are discarded.
  ///
  /// # Parameters
  /// - `base`: a complete, unit-closed pose of this block.
  /// - `pressure`: per-iteration contraction factor applied before projection.
  /// - `work`: shared allowance for contraction, projection, and inspection.
  ///
  /// # Returns
  /// A complete pose with every rigid island rigid and every source bond unit, or
  /// `None` for a trial that crowded, crossed, or failed exact closure.
  ///
  /// # Errors
  /// Propagates work exhaustion before any trial can be accepted.
  fn fold(&self, base: &[Point], pressure: f64, work: &mut Work) -> Result<Option<Vec<Point>>, Error> {
    let n = base.len();
    let mut distances: Vec<([usize; 2], f64)> = self.edges.iter().map(|&ends| (ends, 1.)).collect();
    for atoms in &self.folding.fragments {
      for (offset, &a) in atoms.iter().enumerate() {
        for &b in &atoms[offset + 1..] {
          distances.push(([a, b], base[a].distance(base[b])));
        }
      }
    }
    let mut rows = vec![Vec::new(); n];
    for &[a, b] in &self.edges {
      rows[a].push(b);
      rows[b].push(a);
    }
    let mut chords: Vec<([usize; 2], f64)> = Vec::new();
    for (center, row) in rows.iter().enumerate() {
      if self.linear.get(center).copied().unwrap_or(false) {
        continue;
      }
      for (offset, &a) in row.iter().enumerate() {
        for &b in &row[offset + 1..] {
          chords.push(([a, b], 3_f64.sqrt()));
        }
      }
    }
    let repel = 0.8;
    let mut points = base.to_vec();
    for _ in 0..240 {
      work.charge(n + distances.len() + chords.len())?;
      let count = n.max(1) as f64;
      let centroid = Point::new(
        points.iter().map(|point| point.x).sum::<f64>() / count,
        points.iter().map(|point| point.y).sum::<f64>() / count,
      );
      for point in &mut points {
        *point = centroid.translated(pressure * (point.x - centroid.x), pressure * (point.y - centroid.y));
      }
      for a in 0..n {
        for b in a + 1..n {
          let dx = points[b].x - points[a].x;
          let dy = points[b].y - points[a].y;
          let distance = dx.hypot(dy);
          if distance >= repel || distance < 1e-9 {
            continue;
          }
          let push = (repel - distance) / distance * 0.3;
          points[a] = points[a].translated(-dx * push, -dy * push);
          points[b] = points[b].translated(dx * push, dy * push);
        }
      }
      for _ in 0..4 {
        for &(ends, target) in &distances {
          project(&mut points, ends, target);
        }
        for &(ends, target) in &chords {
          project(&mut points, ends, target);
        }
      }
    }
    if !self.close(&mut points, work)? {
      return Ok(None);
    }
    for &[a, b] in &self.edges {
      if !points[a].is_finite() || !points[b].is_finite() || (points[a].distance(points[b]) - 1.).abs() > 1e-9 {
        return Ok(None);
      }
    }
    for &([a, b], target) in &self.rigid_chords {
      if (points[a].distance(points[b]) - target).abs() > 1e-9 {
        return Ok(None);
      }
    }
    let quality = work.measure(&points, &self.edges)?;
    if !quality.crossings.is_empty() || !quality.crowds.is_empty() {
      return Ok(None);
    }
    Ok(Some(points))
  }

  /// Returns the drawing cost used to compare folded and spectral poses.
  ///
  /// The cost combines shared crowding/crossing penalties, open-path turns, a
  /// straight-angle penalty for every degree-two center, and a weak spread term
  /// that prefers a folded ring without rewarding overlap.
  fn pose_cost(&self, points: &[Point]) -> f64 {
    let mut rows = vec![Vec::new(); points.len()];
    for &[a, b] in &self.edges {
      rows[a].push(b);
      rows[b].push(a);
    }
    let mut cost = self.folding.turn_cost(points);
    for (index, row) in rows.iter().enumerate() {
      if row.len() != 2 {
        continue;
      }
      let [a, b] = [points[row[0]], points[row[1]]];
      let center = points[index];
      let scale = a.distance(center) * b.distance(center);
      if scale < 1e-9 {
        continue;
      }
      let cosine = ((a.x - center.x) * (b.x - center.x) + (a.y - center.y) * (b.y - center.y)) / scale;
      if cosine > 0.55 {
        cost += 2. * (cosine - 0.55).powi(2);
      } else if cosine < -0.9 {
        cost += 8. * (cosine + 0.9).powi(2);
      } else {
        cost += 0.1 * (cosine + 0.5).powi(2);
      }
    }
    let count = points.len().max(1) as f64;
    let centroid = Point::new(
      points.iter().map(|point| point.x).sum::<f64>() / count,
      points.iter().map(|point| point.y).sum::<f64>() / count,
    );
    let spread = points.iter().map(|point| point.distance(centroid)).fold(0., f64::max);
    cost + 0.05 * spread
  }

  /// Projects source edges and protected aromatic chords, verifying both residuals.
  fn close(&self, points: &mut [Point], work: &mut Work) -> Result<bool, Error> {
    for _ in 0..800 {
      work.charge((self.edges.len() + self.rigid_chords.len()) * 2)?;
      for ([a, b], target) in self
        .edges
        .iter()
        .map(|&ends| (ends, 1.))
        .chain(self.rigid_chords.iter().copied())
      {
        let length = points[a].distance(points[b]);
        if !length.is_finite() || length < 1e-10 {
          return Ok(false);
        }
        let correction = (length - target) / (2. * length);
        let delta = Point::new(
          (points[b].x - points[a].x) * correction,
          (points[b].y - points[a].y) * correction,
        );
        points[a] = points[a].translated(delta.x, delta.y);
        points[b] = points[b].translated(-delta.x, -delta.y);
      }
      if self
        .edges
        .iter()
        .all(|&[a, b]| (points[a].distance(points[b]) - 1.).abs() < 1e-10)
        && self
          .rigid_chords
          .iter()
          .all(|&([a, b], target)| (points[a].distance(points[b]) - target).abs() < 1e-10)
      {
        return Ok(true);
      }
    }
    Ok(false)
  }
}

/// Cancels shared cycle edges and accepts only one complete simple boundary.
///
/// A cycle basis need not describe planar faces. Multiple loops, branches, and
/// uncovered components therefore give no boundary seed rather than an assumed
/// embedding. All source edges still participate in the subsequent objective.
fn perimeter(cycles: &[Vec<usize>]) -> Vec<usize> {
  let mut edges = BTreeSet::new();
  for cycle in cycles {
    for (&a, &b) in cycle.iter().zip(cycle.iter().cycle().skip(1)) {
      let pair = (a.min(b), a.max(b));
      if !edges.remove(&pair) {
        edges.insert(pair);
      }
    }
  }
  let mut rows = BTreeMap::<usize, Vec<usize>>::new();
  for &(a, b) in &edges {
    rows.entry(a).or_default().push(b);
    rows.entry(b).or_default().push(a);
  }
  if rows.values().any(|row| row.len() != 2) {
    return Vec::new();
  }
  let Some((&start, _)) = rows.first_key_value() else {
    return Vec::new();
  };
  let mut result = Vec::new();
  let mut previous = usize::MAX;
  let mut node = start;
  loop {
    result.push(node);
    let next = if rows[&node][0] == previous {
      rows[&node][1]
    } else {
      rows[&node][0]
    };
    previous = node;
    node = next;
    if node == start {
      break;
    }
    if result.len() > rows.len() {
      return Vec::new();
    }
  }
  if result.len() == rows.len() { result } else { Vec::new() }
}
