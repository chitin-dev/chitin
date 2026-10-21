//! Derive directed wedge marks from relative configuration and final coordinates.
//!
//! Source traversal winding is not itself a wedge direction. Signed volume in
//! a right-handed y-up frame handles rotations, reflections, and virtual H.

use std::collections::{BTreeMap, BTreeSet};

use chitin_chem::{AtomId, BondId, MolecularGraph, StereoLigand, TetrahedralStereo, TetrahedralWinding};

use super::{DepictError as Error, DepictOptions, LabelBounds, StereoFailureReason, clip, color::BondPaint};
use crate::{
  geometry::Point,
  scene::{Line, LineCap, Polygon, Primitive, Source},
};

/// A mark directed from its stereocenter (narrow) to its ligand (wide).
pub(super) struct Mark {
  /// Narrow endpoint; reversing stored bond endpoints must not reverse this.
  pub center: AtomId,
  /// Explicit bonded ligand selected to carry the stereo information.
  pub ligand: AtomId,
  /// True for a solid wedge toward the observer, false for hashed away.
  pub solid: bool,
}

/// Plans one nonshared wedge per supplied tetrahedral center.
///
/// # Parameters
/// - `graph`: validated relative descriptors and their unchanged graph handles.
/// - `points`: complete final y-down drawing coordinates, before label clipping.
/// - `bridges`: already computed acyclic edges; prefer outward substituents.
/// - `expanded_hydrogens`: centers whose independently drawn C–H mark replaces
///   an existing-bond wedge; never add a second mark at those centers.
///
/// # Returns
/// Marks keyed by edge. Nonstereocenter, terminal ligands are preferred to
/// avoid ambiguous wedges at two adjacent chiral centers. No CIP labeling runs.
///
/// # Errors
/// Rejects degenerate projected directions or a center with no independently
/// drawable bond; no partial marks escape and no supplied stereo is omitted.
pub(super) fn plan(
  graph: &MolecularGraph,
  points: &BTreeMap<AtomId, Point>,
  bridges: &BTreeSet<BondId>,
  expanded_hydrogens: &BTreeSet<AtomId>,
) -> Result<BTreeMap<BondId, Mark>, Error> {
  let centers: Vec<_> = graph
    .tetrahedral_centers()
    .filter(|(center, _)| !expanded_hydrogens.contains(center))
    .collect();
  let mut marks = BTreeMap::new();
  // Preserve the common first-fit path without repeating projection tests.
  // Classifying exceptional failures must not tax every successful drawing.
  if greedy(graph, points, bridges, &centers, &mut marks).is_ok() {
    return Ok(marks);
  }
  marks.clear();
  // Classify intrinsic failure before combinatorial assignment: backtracking
  // cannot create an eligible bond or repair a collapsed local projection.
  for &(center, stereo) in &centers {
    let candidates = candidates(graph, bridges, center);
    if candidates.is_empty() {
      return Err(Error::StereoAssignment {
        atom: center,
        reason: StereoFailureReason::NoEligibleBond,
      });
    }
    let mut usable = false;
    for (ligand, _) in candidates {
      usable |= solid_for(stereo, points[&center], points, StereoLigand::Atom(ligand), None)?.is_some();
    }
    if !usable {
      return Err(Error::StereoAssignment {
        atom: center,
        reason: StereoFailureReason::DegenerateProjection,
      });
    }
  }
  let mut nodes = 0_usize;
  assign(graph, points, bridges, &centers, 0, &mut marks, &mut nodes)?;
  Ok(marks)
}

/// Assigns the first usable unused bond to each center in order.
fn greedy(
  graph: &MolecularGraph,
  points: &BTreeMap<AtomId, Point>,
  bridges: &BTreeSet<BondId>,
  centers: &[(AtomId, &TetrahedralStereo)],
  marks: &mut BTreeMap<BondId, Mark>,
) -> Result<(), Error> {
  for &(center, stereo) in centers {
    let mut chosen = None;
    for (ligand, edge) in candidates(graph, bridges, center) {
      if marks.contains_key(&edge) {
        continue;
      }
      let Some(solid) = solid_for(stereo, points[&center], points, StereoLigand::Atom(ligand), None)? else {
        continue;
      };
      chosen = Some((edge, ligand, solid));
      break;
    }
    let Some((edge, ligand, solid)) = chosen else {
      return Err(Error::StereoAssignment {
        atom: center,
        reason: StereoFailureReason::ConflictingBonds,
      });
    };
    marks.insert(edge, Mark { center, ligand, solid });
  }
  Ok(())
}

/// Returns the preferred bond order for one center's wedge.
fn candidates(graph: &MolecularGraph, bridges: &BTreeSet<BondId>, center: AtomId) -> Vec<(AtomId, BondId)> {
  let mut rows: Vec<_> = graph
    .neighbors(center)
    .into_iter()
    .flatten()
    .filter(|(neighbor, _)| graph.tetrahedral(*neighbor).is_none())
    .collect();
  if rows.is_empty() && graph.neighbors(center).into_iter().flatten().count() >= 4 {
    // A four-ligand center inside a fully annotated cage has no non-center
    // ligand; a bond between centers may carry the mark. Three-ligand centers
    // still require an expanded hydrogen rather than an ambiguous wedge.
    rows = graph.neighbors(center).into_iter().flatten().collect();
  }
  rows.sort_by_key(|&(neighbor, edge)| {
    (
      !bridges.contains(&edge),
      graph.neighbors(neighbor).map_or(usize::MAX, |row| row.len()),
      neighbor,
    )
  });
  rows
}

/// Assigns one unused bond to each center, backtracking on a conflicting edge.
///
/// # Parameters
/// - `centers`: stable supplied order, possibly longer than one branch.
/// - `index`: next center to mark.
/// - `marks`: bonds already carrying a wedge, updated in place.
/// - `nodes`: bounded search counter; exhaustion fails rather than dropping a
///   supplied configuration.
fn assign(
  graph: &MolecularGraph,
  points: &BTreeMap<AtomId, Point>,
  bridges: &BTreeSet<BondId>,
  centers: &[(AtomId, &TetrahedralStereo)],
  index: usize,
  marks: &mut BTreeMap<BondId, Mark>,
  nodes: &mut usize,
) -> Result<(), Error> {
  if index == centers.len() {
    return Ok(());
  }
  *nodes += 1;
  if *nodes > 65_536 {
    return Err(Error::StereoAssignment {
      atom: centers[index].0,
      reason: StereoFailureReason::AssignmentBudget,
    });
  }
  let (center, stereo) = centers[index];
  for (ligand, edge) in candidates(graph, bridges, center) {
    if marks.contains_key(&edge) {
      continue;
    }
    let Some(solid) = solid_for(stereo, points[&center], points, StereoLigand::Atom(ligand), None)? else {
      continue;
    };
    marks.insert(edge, Mark { center, ligand, solid });
    match assign(graph, points, bridges, centers, index + 1, marks, nodes) {
      Ok(()) => return Ok(()),
      Err(
        error @ Error::StereoAssignment {
          reason: StereoFailureReason::AssignmentBudget,
          ..
        },
      ) => return Err(error),
      Err(Error::StereoAssignment { .. }) => {}
      Err(error) => return Err(error),
    }
    marks.remove(&edge);
  }
  Err(Error::StereoAssignment {
    atom: center,
    reason: StereoFailureReason::ConflictingBonds,
  })
}

/// Determines a wedge's polarity for either an actual atom or a displayed H.
///
/// # Parameters
/// - `stereo`: supplied ordered ligand references, never a CIP assignment.
/// - `center`, `points`: final y-down coordinates in one drawing frame.
/// - `raised`: ligand provisionally toward the observer.
/// - `hydrogen_point`: drawn virtual-H position; absent for skeletal H.
///
/// # Returns
/// Solid/hashed polarity preserving the source configuration, or `None` for a
/// degenerate projection. A skeletal H is opposite the other three vectors;
/// an explicit depiction H participates directly without changing its identity.
///
/// # Errors
/// Invalid vectors are rejected before signed-volume arithmetic is used.
pub(super) fn solid_for(
  stereo: &TetrahedralStereo,
  center: Point,
  points: &BTreeMap<AtomId, Point>,
  raised: StereoLigand,
  hydrogen_point: Option<Point>,
) -> Result<Option<bool>, Error> {
  let mut vectors = [[0.; 3]; 4];
  let mut implicit_hydrogen = None;
  for (i, reference) in stereo.ligands().iter().enumerate() {
    let point = match reference {
      StereoLigand::Atom(atom) => points.get(atom).copied().ok_or(Error::MissingPosition(*atom))?,
      StereoLigand::Hydrogen => match hydrogen_point {
        Some(point) => point,
        None => {
          implicit_hydrogen = Some(i);
          continue;
        }
      },
    };
    let length = center.distance(point);
    if !length.is_finite() || length <= 0. {
      return Ok(None);
    }
    vectors[i] = [
      (point.x - center.x) / length,
      -(point.y - center.y) / length,
      f64::from(*reference == raised),
    ];
  }
  if let Some(hydrogen) = implicit_hydrogen {
    for axis in 0..3 {
      vectors[hydrogen][axis] = -vectors.iter().map(|vector| vector[axis]).sum::<f64>();
    }
  }
  let volume = volume(vectors);
  Ok(
    (volume.is_finite() && volume.abs() > 1e-8)
      .then_some((volume < 0.) == (stereo.winding() == TetrahedralWinding::Anticlockwise)),
  )
}

/// Returns six times oriented tetrahedron volume in a right-handed frame.
fn volume(vectors: [[f64; 3]; 4]) -> f64 {
  let a = std::array::from_fn::<_, 3, _>(|i| vectors[1][i] - vectors[0][i]);
  let b = std::array::from_fn::<_, 3, _>(|i| vectors[2][i] - vectors[0][i]);
  let c = std::array::from_fn::<_, 3, _>(|i| vectors[3][i] - vectors[0][i]);
  a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])
}

/// Emits clipped solid/hashed geometry with the original bond source.
///
/// Label envelopes receive an additional half-width margin so wide ends cannot
/// overlap text. A fully obscured wedge fails rather than erasing stereo.
pub(super) fn draw(
  edge: BondId,
  mark: &Mark,
  points: &BTreeMap<AtomId, Point>,
  labels: &BTreeMap<AtomId, LabelBounds>,
  options: &DepictOptions,
  paint: &BondPaint,
  output: &mut Vec<Primitive>,
) -> Result<(), Error> {
  wedge(
    [points[&mark.center], points[&mark.ligand]],
    [labels.get(&mark.center).copied(), labels.get(&mark.ligand).copied()],
    mark.solid,
    Source::Bond(edge),
    options,
    paint,
    output,
  )
}

/// Emits the same directed wedge geometry for source bonds and virtual C–H bonds.
///
/// # Errors
/// Returns a source-located error if label clipping obscures the entire mark.
pub(super) fn wedge(
  [p, q]: [Point; 2],
  bounds: [Option<LabelBounds>; 2],
  solid: bool,
  source: Source,
  options: &DepictOptions,
  paint: &BondPaint,
  output: &mut Vec<Primitive>,
) -> Result<(), Error> {
  let occluded = || match source {
    Source::Bond(edge) => Error::OccludedBond(edge),
    Source::Atom(center) | Source::AttachedHydrogen(center) => Error::Stereo(center),
  };
  let margin = options.wedge_width / 2. + options.stroke_width / 2.;
  let expanded = |bounds: Option<LabelBounds>| {
    bounds.map(|bounds| LabelBounds {
      min: bounds.min.translated(-margin, -margin),
      max: bounds.max.translated(margin, margin),
    })
  };
  let start = clip(p, q, expanded(bounds[0])).ok_or_else(occluded)?;
  let end = clip(q, p, expanded(bounds[1])).ok_or_else(occluded)?;
  let dx = q.x - p.x;
  let dy = q.y - p.y;
  let length = dx.hypot(dy);
  if !length.is_finite() || length <= 0. || (end.x - start.x) * dx + (end.y - start.y) * dy <= 0. {
    return Err(occluded());
  }
  let normal = Point::new(-dy / length, dx / length);
  let source = Some(source);
  if solid {
    paint.polygon(
      Polygon {
        points: vec![
          start,
          end.translated(normal.x * options.wedge_width / 2., normal.y * options.wedge_width / 2.),
          end.translated(
            -normal.x * options.wedge_width / 2.,
            -normal.y * options.wedge_width / 2.,
          ),
        ],
        color: options.foreground,
        source,
      },
      output,
    );
  } else {
    // The narrow center-to-wide-ligand direction is preserved for both marks.
    for i in 1..=7 {
      let t = f64::from(i) / 8.;
      let center = start.translated((end.x - start.x) * t, (end.y - start.y) * t);
      let half = options.wedge_width * t / 2.;
      paint.line(
        Line {
          start: center.translated(normal.x * half, normal.y * half),
          end: center.translated(-normal.x * half, -normal.y * half),
          width: options.stroke_width,
          color: options.foreground,
          caps: [LineCap::Round; 2],
          source,
        },
        output,
      );
    }
  }
  Ok(())
}
