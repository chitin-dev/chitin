//! Final source-edge admission includes tangency and overlap, not only crossings.

use super::RenderError;
use crate::{depict, geometry::Point, layout::Layout2d};
use chitin_chem::MolecularGraph;

/// Counts crossings, tangencies and collinear overlap independently of diagnostics.
///
/// A strict crossing count alone can trade crossings for coincident paths.
/// Geometry is normalized by the median positive source-bond length; authored
/// perspective crossings are excluded only by their exact source identities.
pub(super) fn count(graph: &MolecularGraph, layout: &Layout2d) -> Result<usize, RenderError> {
  let edges = graph
    .bonds()
    .map(|(id, bond)| {
      let atoms = bond.atoms();
      let a = layout
        .position(atoms[0])
        .ok_or(depict::DepictError::MissingPosition(atoms[0]))?;
      let b = layout
        .position(atoms[1])
        .ok_or(depict::DepictError::MissingPosition(atoms[1]))?;
      Ok((id, atoms, [a, b]))
    })
    .collect::<Result<Vec<_>, depict::DepictError>>()?;
  let mut lengths: Vec<_> = edges
    .iter()
    .map(|(_, _, [a, b])| a.distance(*b))
    .filter(|length| *length > 0.)
    .collect();
  if lengths.is_empty() {
    return Ok(0);
  }
  lengths.sort_by(f64::total_cmp);
  let middle = lengths.len() / 2;
  let scale = if lengths.len().is_multiple_of(2) {
    lengths[middle - 1] / 2. + lengths[middle] / 2.
  } else {
    lengths[middle]
  };
  let epsilon = scale * 1e-9;
  let mut count = 0;
  for (index, &(id, atoms, [a, b])) in edges.iter().enumerate() {
    for &(other, ends, [c, d]) in &edges[index + 1..] {
      if atoms.iter().any(|atom| ends.contains(atom))
        || layout.bond_crossings().iter().any(|crossing| {
          (crossing.front == id && crossing.back == other) || (crossing.front == other && crossing.back == id)
        })
      {
        continue;
      }
      if a.x.max(b.x) + epsilon < c.x.min(d.x)
        || c.x.max(d.x) + epsilon < a.x.min(b.x)
        || a.y.max(b.y) + epsilon < c.y.min(d.y)
        || c.y.max(d.y) + epsilon < a.y.min(b.y)
      {
        continue;
      }
      count += usize::from(segments_touch(a, b, c, d, epsilon));
    }
  }
  Ok(count)
}

/// Includes endpoint and collinear contacts, not only strict intersections.
fn segments_touch(a: Point, b: Point, c: Point, d: Point, epsilon: f64) -> bool {
  let side = |a: Point, b: Point, p: Point| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
  let distance = |p: Point, a: Point, b: Point| {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let denominator = dx * dx + dy * dy;
    let parameter = if denominator > 0. {
      ((p.x - a.x) * dx + (p.y - a.y) * dy) / denominator
    } else {
      0.
    };
    p.distance(a.translated(parameter.clamp(0., 1.) * dx, parameter.clamp(0., 1.) * dy))
  };
  side(a, b, c) * side(a, b, d) < 0. && side(c, d, a) * side(c, d, b) < 0.
    || [(a, c, d), (b, c, d), (c, a, b), (d, a, b)]
      .iter()
      .any(|&(p, a, b)| distance(p, a, b) <= epsilon)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::layout;
  use chitin_chem::io::smiles::read_smiles;

  type TestResult = Result<(), Box<dyn std::error::Error>>;

  #[test]
  fn edge_admission_should_include_nonincident_tangencies_at_any_scale() -> TestResult {
    let graph = read_smiles("CC.CC")?.into_graph();
    let base = [
      Point::new(0., 0.),
      Point::new(1., 0.),
      Point::new(0.5, 0.),
      Point::new(0.5, 1.),
    ];
    for scale in [0.1, 1., 10.] {
      let positions = graph
        .atoms()
        .zip(base)
        .map(|((id, _), point)| (id, Point::new(7. + point.x * scale, -3. + point.y * scale)))
        .collect();
      let layout = Layout2d::from_positions(positions)?;
      assert_eq!(layout::contact_counts(&graph, &layout, 0.55)?[2], 0);
      assert_eq!(count(&graph, &layout)?, 1);
    }
    Ok(())
  }

  #[test]
  fn exact_edge_predicate_should_distinguish_overlap_from_clear_parallel_lines() {
    let (a, b) = (Point::new(0., 0.), Point::new(1., 0.));
    assert!(segments_touch(a, b, Point::new(0.5, 0.), Point::new(1.5, 0.), 1e-9));
    assert!(segments_touch(a, b, Point::new(0.5, -1.), Point::new(0.5, 1.), 1e-9));
    assert!(!segments_touch(a, b, Point::new(0., 1e-6), Point::new(1., 1e-6), 1e-9));
  }
}
