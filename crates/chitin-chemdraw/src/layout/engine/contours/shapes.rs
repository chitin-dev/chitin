//! Compact and staggered cell families followed by perimeter-preserving peeling.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{
  Error, Work,
  lattice::{Cell, Shape},
};
#[cfg(test)]
use std::collections::BTreeSet;

/// Reproducible rectangular family in axial, rather than Cartesian, coordinates.
#[derive(Clone, Copy)]
enum BoxKind {
  Skewed,
  Staggered,
  Narrow,
  Wide,
}

/// Builds one rectangular or alternating-row shape without chemical state.
fn rectangle(x: usize, y: usize, kind: BoxKind, work: &mut Work) -> Result<Shape, Error> {
  work.charge((x + 1) * y)?;
  let mut shape = Shape::new();
  for row in 0..y {
    let offset = if matches!(kind, BoxKind::Skewed) {
      0
    } else {
      -(row as i32 / 2)
    };
    let width = match (kind, row % 2) {
      (BoxKind::Narrow, 1) => x - 1,
      (BoxKind::Wide, 1) => x + 1,
      _ => x,
    };
    let offset = offset - i32::from(matches!(kind, BoxKind::Wide) && row % 2 == 1);
    for column in 0..width {
      shape.insert(Cell(offset + column as i32, row as i32));
    }
  }
  Ok(shape)
}

/// Expands a two-cell seed by two perimeter vertices, filling three-sided pockets.
pub(super) fn compact(n: usize, work: &mut Work) -> Result<Shape, Error> {
  let mut shape = Shape::new();
  shape.insert(Cell(0, 0));
  shape.insert(Cell(1, 0));
  let mut perimeter = 10;
  while perimeter < n {
    let frontier = shape.frontier(work)?;
    work.charge(6 * frontier.len())?;
    let Some(&best) = frontier
      .iter()
      .filter(|&&cell| shape.neighbors(cell) == 2)
      .min_by_key(|&&cell| cell.radius())
    else {
      break;
    };
    shape.insert(best);
    for cell in frontier {
      if cell != best && shape.neighbors(cell) == 3 {
        shape.insert(cell);
      }
    }
    perimeter += 2;
  }
  Ok(shape)
}

/// Enumerates the source family's valid box factorizations, rounder shapes first.
fn boxes(n: usize, work: &mut Work) -> Result<Vec<Shape>, Error> {
  let mut result = Vec::new();
  if n.is_multiple_of(4) {
    let sum = n / 4;
    for x in 1..sum / 2 {
      let y = sum - x;
      if y.is_multiple_of(2) && x > 1 {
        result.push(rectangle(x, y, BoxKind::Wide, work)?);
      }
      if x.is_multiple_of(2) && y > 1 {
        result.push(rectangle(y, x, BoxKind::Wide, work)?);
      }
    }
  } else {
    let sum = (n + 2) / 4;
    for x in 1..=sum / 2 {
      let y = sum - x;
      result.push(rectangle(x, y, BoxKind::Skewed, work)?);
      if x < 2 || y < 2 {
        continue;
      }
      result.push(rectangle(x, y, BoxKind::Staggered, work)?);
      result.push(rectangle(y, x, BoxKind::Staggered, work)?);
      if y % 2 == 1 {
        result.push(rectangle(x, y, BoxKind::Wide, work)?);
      }
      if x % 2 == 1 {
        result.push(rectangle(y, x, BoxKind::Wide, work)?);
      }
      if x > 2 && y % 2 == 1 {
        result.push(rectangle(x, y, BoxKind::Narrow, work)?);
      }
      if y > 2 && x % 2 == 1 {
        result.push(rectangle(y, x, BoxKind::Narrow, work)?);
      }
    }
  }
  result.reverse();
  Ok(result)
}

/// Initial candidate cohort, with odd closures marked before shape deduplication.
pub(super) fn initial_family(n: usize, work: &mut Work) -> Result<Vec<Shape>, Error> {
  if n < 9 {
    return Ok(Vec::new());
  }
  let perimeter = n + n % 2;
  let mut result = vec![compact(perimeter, work)?];
  result.extend(boxes(perimeter, work)?);
  if n % 2 == 1 {
    for shape in &mut result {
      shape.mark_pentagon(work)?;
    }
  }
  Ok(result)
}

#[cfg(test)]
mod tests {
  use super::*;
  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn compact_contours_should_match_integer_reference_vectors() -> TestResult {
    for line in include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/contours.tsv")).lines() {
      let (n, expected) = line.split_once('\t').ok_or("missing reference signature")?;
      let mut work = Work::new(1_000_000);
      let shape = compact(n.parse()?, &mut work)?;
      let path = shape.contour(&mut work)?.ok_or("invalid contour")?;
      let actual: String = path
        .into_iter()
        .map(|vertex| if shape.multiplicity(vertex) == 1 { '1' } else { '2' })
        .collect();
      assert_eq!(actual, expected, "perimeter {n}");
    }
    Ok(())
  }

  #[test]
  fn families_should_close_with_unit_bonds_and_unique_vertices() -> TestResult {
    for n in (10..=80).step_by(2) {
      let mut work = Work::new(4_000_000);
      let shapes = initial_family(n, &mut work)?;
      assert!(!shapes.is_empty(), "size {n}");
      let mut keys = BTreeSet::new();
      for shape in shapes {
        if !keys.insert(shape.key(&mut work)?) {
          continue;
        }
        let vertices = shape.contour(&mut work)?.ok_or("invalid contour")?;
        assert_eq!(vertices.len(), n);
        assert_eq!(vertices.iter().collect::<BTreeSet<_>>().len(), n);
        for i in 0..n {
          assert!((vertices[i].point().distance(vertices[(i + 1) % n].point()) - 1.).abs() < 1e-9);
        }
      }
    }
    Ok(())
  }
}
