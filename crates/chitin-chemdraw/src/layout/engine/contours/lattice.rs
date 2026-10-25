//! Integer cell geometry; detached values cannot retain dangling cell references.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use std::collections::{BTreeMap, BTreeSet};

use super::{Error, Point, Work};

/// Axial coordinates; insertion order is retained separately for stable growth ties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Cell(pub i32, pub i32);

impl Cell {
  /// Visits neighbors clockwise, starting along the positive first axis.
  pub(super) fn neighbors(self) -> [Self; 6] {
    [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)].map(|(x, y)| Self(self.0 + x, self.1 + y))
  }

  /// Returns exact hexagonal distance from the origin.
  pub(super) fn radius(self) -> i32 {
    self.0.abs().max(self.1.abs()).max((self.0 + self.1).abs())
  }

  /// Rotates one lattice sector, not thirty Cartesian degrees.
  fn rotate(self) -> Self {
    Self(self.0 + self.1, -self.0)
  }

  /// Returns oriented corners on the bipartite honeycomb vertex lattice.
  fn corners(self) -> [Vertex; 6] {
    let z = -self.0 - self.1;
    [(1, 0, 0), (0, -1, 0), (0, 0, 1), (-1, 0, 0), (0, 1, 0), (0, 0, -1)]
      .map(|(x, y, dz)| Vertex(self.0 + x, self.1 + y, z + dz))
  }
}

/// Three integer coordinates whose sum is plus or minus one at a real vertex.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Vertex(i32, i32, i32);

impl Vertex {
  /// Converts exact lattice geometry to nominal unit-length Cartesian bonds.
  pub(super) fn point(self) -> Point {
    Point::new(
      3_f64.sqrt() / 2. * f64::from(self.0 - self.2),
      f64::from(self.1) - 0.5 * f64::from(self.0 + self.2),
    )
  }

  /// Lists the only three cells incident to this vertex, occupied or empty.
  fn incident(self) -> [Cell; 3] {
    let direction = self.0 + self.1 + self.2;
    [
      Cell(self.0 - direction, self.1),
      Cell(self.0, self.1 - direction),
      Cell(self.0, self.1),
    ]
  }
}

/// One connected, hole-free shape whose boundary is validated before publication.
#[derive(Debug, Clone)]
pub(super) struct Shape {
  cells: Vec<Cell>,
  occupied: BTreeSet<Cell>,
  omitted: Option<Vertex>,
}

impl Shape {
  /// Starts an empty provisional shape; only a validated contour is drawable.
  pub(super) fn new() -> Self {
    Self {
      cells: Vec::new(),
      occupied: BTreeSet::new(),
      omitted: None,
    }
  }

  /// Adds a cell once without allocating a coordinate-sized rectangular grid.
  pub(super) fn insert(&mut self, cell: Cell) {
    if self.occupied.insert(cell) {
      self.cells.push(cell);
    }
  }

  /// Returns the exact number of occupied cells.
  pub(super) fn len(&self) -> usize {
    self.cells.len()
  }

  /// Counts occupied cells along the six sides of an empty or occupied cell.
  pub(super) fn neighbors(&self, cell: Cell) -> usize {
    cell
      .neighbors()
      .iter()
      .filter(|cell| self.occupied.contains(cell))
      .count()
  }

  /// Visits unoccupied frontier cells in insertion-based order, without duplicates.
  pub(super) fn frontier(&self, work: &mut Work) -> Result<Vec<Cell>, Error> {
    work.charge(6 * self.len())?;
    let mut visited = BTreeSet::new();
    Ok(
      self
        .cells
        .iter()
        .flat_map(|cell| cell.neighbors())
        .filter(|cell| !self.occupied.contains(cell) && visited.insert(*cell))
        .collect(),
    )
  }

  /// Canonicalizes translations and six rotations; reflections remain distinct.
  ///
  /// # Returns
  /// A stable key suitable for indexed deduplication rather than pairwise shape
  /// comparison. Reflection is not a chemical stereochemistry operation here.
  ///
  /// # Errors
  /// Charges sorting and every transformed cell to the shared search allowance.
  pub(super) fn key(&self, work: &mut Work) -> Result<Vec<Cell>, Error> {
    let n = self.len();
    work.charge(6 * n * (2 + n.checked_ilog2().unwrap_or(0) as usize))?;
    let mut rotated = self.cells.clone();
    let mut best = None;
    for _ in 0..6 {
      let min_x = rotated.iter().map(|cell| cell.0).min().unwrap_or(0);
      let min_y = rotated.iter().map(|cell| cell.1).min().unwrap_or(0);
      let mut key: Vec<_> = rotated
        .iter()
        .map(|cell| Cell(cell.0 - min_x, cell.1 - min_y))
        .collect();
      key.sort_unstable();
      if best.as_ref().is_none_or(|old| key < *old) {
        best = Some(key);
      }
      for cell in &mut rotated {
        *cell = cell.rotate();
      }
    }
    Ok(best.unwrap_or_default())
  }

  /// Removes each cell with three consecutive neighbors in independent snapshots.
  ///
  /// Such a removal preserves perimeter length. The consumer still validates
  /// connectedness, boundary simplicity and absence of holes before scoring.
  pub(super) fn equivalents(&self, work: &mut Work) -> Result<Vec<Self>, Error> {
    let mut result = Vec::new();
    for &cell in &self.cells {
      work.charge(6)?;
      let present = cell.neighbors().map(|neighbor| self.occupied.contains(&neighbor));
      if present.iter().filter(|&&yes| yes).count() != 3
        || !(0..6).any(|i| present[i] && present[(i + 1) % 6] && present[(i + 2) % 6])
      {
        continue;
      }
      work.charge(self.len())?;
      let mut shape = self.clone();
      shape.omitted = None;
      shape.cells.retain(|&other| other != cell);
      shape.occupied.remove(&cell);
      if self.omitted.is_some() {
        shape.mark_pentagon(work)?;
      }
      result.push(shape);
    }
    Ok(result)
  }

  /// Cancels shared edges and proves that every remaining edge lies on one loop.
  ///
  /// # Returns
  /// An exact simple contour, or `None` for holes, pinches or disconnected pieces.
  /// No arbitrary loop iteration limit can silently discard a second boundary.
  ///
  /// # Errors
  /// Work exhaustion rejects the provisional shape without publishing vertices.
  pub(super) fn contour(&self, work: &mut Work) -> Result<Option<Vec<Vertex>>, Error> {
    work.charge(12 * self.len())?;
    let mut edges = BTreeSet::new();
    for cell in &self.cells {
      let corners = cell.corners();
      for i in 0..6 {
        let edge = (corners[i], corners[(i + 1) % 6]);
        if !edges.remove(&(edge.1, edge.0)) {
          edges.insert(edge);
        }
      }
    }
    let count = edges.len();
    let mut next = BTreeMap::new();
    for (a, b) in edges {
      if next.insert(a, b).is_some() {
        return Ok(None);
      }
    }
    let Some(start) = self
      .cells
      .iter()
      .map(|cell| cell.corners()[0])
      .find(|&vertex| self.multiplicity(vertex) == 1)
    else {
      return Ok(None);
    };
    work.charge(count)?;
    let mut result = Vec::with_capacity(count);
    let mut current = start;
    for _ in 0..count {
      result.push(current);
      let Some(other) = next.remove(&current) else {
        return Ok(None);
      };
      current = other;
    }
    if current != start || !next.is_empty() {
      return Ok(None);
    }
    result.retain(|vertex| Some(*vertex) != self.omitted);
    Ok(Some(result))
  }

  /// Counts occupied cells meeting at a boundary vertex: one convex, two concave.
  pub(super) fn multiplicity(&self, vertex: Vertex) -> usize {
    vertex
      .incident()
      .iter()
      .filter(|cell| self.occupied.contains(cell))
      .count()
  }

  /// Collapses one alternating corner; the resulting long edge requires relaxation.
  pub(super) fn mark_pentagon(&mut self, work: &mut Work) -> Result<(), Error> {
    self.omitted = None;
    let Some(path) = self.contour(work)? else {
      return Ok(());
    };
    let n = path.len();
    for pattern in [[2, 1, 2], [1, 2, 1]] {
      for i in 0..n {
        work.charge(1)?;
        let actual = [path[(i + n - 1) % n], path[i], path[(i + 1) % n]].map(|v| self.multiplicity(v));
        if actual == pattern {
          self.omitted = Some(path[i]);
          return Ok(());
        }
      }
    }
    Ok(())
  }

  /// Empty incident cells delimit permitted shared-face placements.
  pub(super) fn free_cells(&self, vertex: Vertex) -> Vec<Cell> {
    vertex
      .incident()
      .into_iter()
      .filter(|cell| !self.occupied.contains(cell))
      .collect()
  }

  /// Exact prospective attachment position, independent of floating-point angles.
  pub(super) fn substituent(&self, vertex: Vertex) -> Option<Vertex> {
    let parents: Vec<_> = vertex
      .incident()
      .into_iter()
      .filter(|cell| self.occupied.contains(cell))
      .collect();
    let to_vertex = |cell: Cell| Vertex(cell.0, cell.1, -cell.0 - cell.1);
    let parent = to_vertex(*parents.first()?);
    let mut delta = Vertex(vertex.0 - parent.0, vertex.1 - parent.1, vertex.2 - parent.2);
    match parents.len() {
      1 => {
        let d = if delta.0 + delta.1 + delta.2 > 0 { 1 } else { -1 };
        if delta.0 == 0 {
          delta.0 -= d;
        }
        if delta.1 == 0 {
          delta.1 -= d;
        }
        if delta.2 == 0 {
          delta.2 -= d;
        }
        Some(Vertex(parent.0 + delta.0, parent.1 + delta.1, parent.2 + delta.2))
      }
      2 => {
        let other = to_vertex(parents[1]);
        Some(Vertex(other.0 - delta.0, other.1 - delta.1, other.2 - delta.2))
      }
      _ => None,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn contour_should_reject_holes_and_disconnected_cells() -> TestResult {
    let mut hole = Shape::new();
    for cell in Cell(0, 0).neighbors() {
      hole.insert(cell);
    }
    assert!(hole.contour(&mut Work::new(10000))?.is_none());
    let mut separated = Shape::new();
    separated.insert(Cell(0, 0));
    separated.insert(Cell(5, 0));
    assert!(separated.contour(&mut Work::new(10000))?.is_none());
    Ok(())
  }

  #[test]
  fn key_should_ignore_translation_and_rotation_but_not_reflection() -> TestResult {
    let mut shape = Shape::new();
    for cell in [Cell(0, 0), Cell(1, 0), Cell(2, 0), Cell(0, 1)] {
      shape.insert(cell);
    }
    let mut moved = Shape::new();
    let mut mirrored = Shape::new();
    for &cell in &shape.cells {
      let cell2 = cell.rotate();
      moved.insert(Cell(cell2.0 + 8, cell2.1 - 4));
      mirrored.insert(Cell(cell.1, cell.0));
    }
    let mut work = Work::new(10000);
    assert_eq!(shape.key(&mut work)?, moved.key(&mut work)?);
    assert_ne!(shape.key(&mut work)?, mirrored.key(&mut work)?);
    Ok(())
  }
}
