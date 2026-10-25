//! Topology-free contour search, separate from chemical matching and rigid assembly.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{Error, Work};
use crate::geometry::Point;

mod lattice;
mod matching;
mod shapes;

pub(crate) use matching::{Rules, select};
