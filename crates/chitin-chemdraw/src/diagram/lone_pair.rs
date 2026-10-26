//! Visual position suggestions for explicitly supplied electron pairs.

use super::{Annotation, Diagram, DiagramError, LonePair, arrows, transform};
use crate::{
  geometry::{Bounds, Point},
  scene::Drawing,
};

/// Bounded local search for an explicitly authored lone pair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LonePairOptions {
  /// Preferred outward direction in radians in y-down axes.
  pub preferred_angle: f64,
  /// First atom-center distance searched, in local molecule drawing units.
  pub min_distance: f64,
  /// Last atom-center distance searched, in local molecule drawing units.
  pub max_distance: f64,
  /// Difference between consecutive searched distances.
  pub distance_step: f64,
  /// Angular candidates on a full circle; must be in 1..=64.
  pub directions: usize,
  /// Extra clearance around the two dots, in local molecule drawing units.
  pub margin: f64,
  /// Maximum complete position candidates; must be in 1..=1024.
  pub max_candidates: usize,
  /// Maximum dot/obstacle comparisons.
  pub work_budget: usize,
}

impl Default for LonePairOptions {
  fn default() -> Self {
    Self {
      preferred_angle: 0.,
      min_distance: 8.,
      max_distance: 48.,
      distance_step: 4.,
      directions: 16,
      margin: 2.,
      max_candidates: 256,
      work_budget: 1_000_000,
    }
  }
}

/// A geometric suggestion, with no claim about valence or electron count.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LonePairSuggestion {
  /// Pair with an updated attachment offset and perpendicular dot orientation.
  pub pair: LonePair,
  /// Both dot envelopes clear all considered obstacles.
  pub clear: bool,
  /// Complete positions evaluated.
  pub evaluated: usize,
  /// Dot/obstacle comparisons performed.
  pub work_used: usize,
  /// Search ended at the work or candidate limit.
  pub limited: bool,
}

impl Diagram {
  /// Suggests a free position for an explicitly requested electron pair.
  ///
  /// Molecular ink, existing non-highlight annotations and host text boxes are
  /// obstacles. Distances and margins follow the molecule scale. Preferred
  /// direction is tried first at each increasing radius. No pair is inserted,
  /// no chemical facts are changed, and failure is returned explicitly.
  ///
  /// # Errors
  /// Rejects invalid limits, foreign attachments and invalid drawing geometry.
  pub fn suggest_lone_pair(
    &self,
    pair: LonePair,
    options: &LonePairOptions,
    boxes: &[Bounds],
  ) -> Result<LonePairSuggestion, DiagramError> {
    if !options.preferred_angle.is_finite()
      || !options.min_distance.is_finite()
      || !options.max_distance.is_finite()
      || options.min_distance < 0.
      || options.max_distance < options.min_distance
      || options.max_distance > 1e6
      || !options.distance_step.is_finite()
      || options.distance_step <= 0.
      || !options.margin.is_finite()
      || !(0. ..=1e6).contains(&options.margin)
      || !(1..=64).contains(&options.directions)
      || !(1..=1024).contains(&options.max_candidates)
      || options.work_budget == 0
    {
      return Err(DiagramError::Geometry);
    }
    self.lone_pair(&pair, &mut Vec::new())?;
    let mut pieces = Vec::new();
    for molecule in &self.molecules {
      pieces.extend(
        transform::drawing(&molecule.drawing, molecule.placement)?
          .primitives()
          .iter()
          .cloned(),
      );
    }
    for annotation in &self.annotations {
      match annotation {
        Annotation::Highlight(_) => {}
        Annotation::ElectronArrow(a) => arrows::electron(self, a, &mut pieces)?,
        Annotation::ElectronDot(a) => self.electron_dot(a, &mut pieces)?,
        Annotation::ReactionArrow(a) => arrows::reaction(a, &mut pieces)?,
        Annotation::LonePair(a) => self.lone_pair(a, &mut pieces)?,
      }
    }
    let drawing = Drawing::new(pieces)?;
    // Ink envelopes are conservative; this helper may decline a crowded position.
    let mut obstacles = boxes.to_vec();
    for primitive in drawing.leaf_primitives() {
      if let Some(bounds) = Drawing::new(vec![primitive.clone()])?.bounds() {
        obstacles.push(bounds);
      }
    }
    if obstacles.len() > 131_072 {
      return Err(DiagramError::Geometry);
    }
    let scale = self.molecule(pair.anchor.molecule)?.placement.scale;
    let origin = self.resolve(pair.anchor)?;
    let mut result = LonePairSuggestion {
      pair,
      clear: false,
      evaluated: 0,
      work_used: 0,
      limited: false,
    };
    let mut distance = options.min_distance;
    while distance <= options.max_distance {
      for i in 0..options.directions {
        if result.evaluated >= options.max_candidates {
          result.limited = true;
          return Ok(result);
        }
        let order = if i == 0 {
          0.
        } else if i % 2 == 1 {
          (i + 1) as f64 / 2.
        } else {
          -(i as f64) / 2.
        };
        let angle = options.preferred_angle + std::f64::consts::TAU * order / options.directions as f64;
        let center = Point::new(
          origin.x + distance * scale * angle.cos(),
          origin.y + distance * scale * angle.sin(),
        );
        if !center.is_finite() {
          return Err(DiagramError::Geometry);
        }
        let radius = (pair.radius + options.margin) * scale;
        let mut clear = true;
        for sign in [-1., 1.] {
          let dot = Point::new(
            center.x - sign * angle.sin() * pair.separation * scale / 2.,
            center.y + sign * angle.cos() * pair.separation * scale / 2.,
          );
          for b in &obstacles {
            if result.work_used >= options.work_budget {
              result.limited = true;
              return Ok(result);
            }
            result.work_used += 1;
            let x = dot.x.clamp(b.min().x, b.max().x);
            let y = dot.y.clamp(b.min().y, b.max().y);
            if dot.distance(Point::new(x, y)) <= radius {
              clear = false;
            }
          }
        }
        result.evaluated += 1;
        if clear {
          result.pair.anchor.offset = Point::new(
            pair.anchor.offset.x + distance * angle.cos(),
            pair.anchor.offset.y + distance * angle.sin(),
          );
          result.pair.angle = angle + std::f64::consts::FRAC_PI_2;
          result.clear = true;
          return Ok(result);
        }
      }
      distance += options.distance_step;
    }
    Ok(result)
  }
}
