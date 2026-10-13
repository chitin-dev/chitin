//! Cached sequence data and arithmetic row/column projection for virtualized views.

use std::{
  collections::{BTreeSet, HashMap},
  ops::Range,
  sync::Arc,
};

use chitin_bio::structure::{AtomId, ChainId, ResidueId, ResidueKind, StructureScene, selection::AtomSelection};
use gpui::{Bounds, Pixels, Point, px};

pub(super) const CELL_WIDTH: f32 = 20.0;
pub(super) const CELL_HEIGHT: f32 = 36.0;
pub(super) const ROW_HEIGHT: f32 = 44.0;
pub(super) const LABEL_WIDTH: f32 = 56.0;
pub(super) const ROW_GAP: f32 = 8.0;
pub(super) const SIDE_PADDING: f32 = 8.0;

pub(super) struct SequenceResidue {
  pub id: ResidueId,
  pub letter: char,
  /// Tooltip formatting is cached with topology, rather than repeated per frame.
  pub tooltip: String,
  pub atoms: Arc<AtomSelection>,
}

pub(super) struct SequenceChain {
  pub id: ChainId,
  pub label: String,
  pub residues: Vec<SequenceResidue>,
}

#[derive(Default)]
pub(super) struct SequenceData {
  pub chains: Vec<SequenceChain>,
  /// Canonical selected atoms are projected only when their snapshot changes.
  pub atom_residues: HashMap<AtomId, ResidueId>,
}

impl SequenceData {
  pub fn project(scene: &StructureScene) -> Self {
    // Source chain metadata supplies residue order; grouping needs no sorted map.
    let mut atoms_by_residue: HashMap<ResidueId, Vec<AtomId>> = HashMap::new();
    let mut present_chains = BTreeSet::new();
    let mut atom_residues = HashMap::with_capacity(scene.atoms.len());
    for atom in &scene.atoms {
      atoms_by_residue.entry(atom.residue_id).or_default().push(atom.atom_id);
      present_chains.insert(atom.chain_id);
      atom_residues.insert(atom.atom_id, atom.residue_id);
    }
    let chains = present_chains
      .into_iter()
      .filter_map(|id| {
        let source = scene.selection.chain(id)?;
        let label = source
          .auth_id
          .as_ref()
          .or(source.label_id.as_ref())
          .cloned()
          .unwrap_or_else(|| "(blank)".into());
        let residues: Vec<_> = source
          .residue_ids
          .iter()
          .filter_map(|residue_id| {
            let metadata = scene.selection.residue(*residue_id)?;
            (metadata.kind == ResidueKind::Polymer).then(|| SequenceResidue {
              id: *residue_id,
              letter: one_letter(&metadata.name),
              tooltip: format!(
                "{} {}{} · chain {}",
                metadata.name,
                metadata.sequence_number,
                metadata.insertion_code.map(|code| code.to_string()).unwrap_or_default(),
                label
              ),
              atoms: Arc::new(
                atoms_by_residue
                  .remove(residue_id)
                  .unwrap_or_default()
                  .into_iter()
                  .collect(),
              ),
            })
          })
          .collect();
        (!residues.is_empty()).then_some(SequenceChain { id, label, residues })
      })
      .collect();
    Self { chains, atom_residues }
  }
}

/// A contiguous slice of one chain; preview contains no atom sets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ResidueSpan {
  pub chain: usize,
  pub range: Range<usize>,
}

struct ChainRows {
  chain: usize,
  first: usize,
  len: usize,
}

/// Prefix counts map a virtual row to a chain without materializing every wrapped row.
#[derive(Default)]
pub(super) struct SequenceLayout {
  chains: Vec<ChainRows>,
  columns: Option<usize>,
  pub count: usize,
}

impl SequenceLayout {
  pub fn new(chains: impl Iterator<Item = (usize, usize)>, columns: Option<usize>) -> Self {
    let columns = columns.map(|columns| columns.max(1));
    let mut count = 0;
    let chains = chains
      .filter(|(_, len)| *len > 0)
      .map(|(chain, len)| {
        let first = count;
        count += columns.map_or(1, |columns| len.div_ceil(columns));
        ChainRows { chain, first, len }
      })
      .collect();
    Self { chains, columns, count }
  }

  pub fn row(&self, row: usize) -> Option<ResidueSpan> {
    if row >= self.count {
      return None;
    }
    let index = self.chains.partition_point(|chain| chain.first <= row).checked_sub(1)?;
    let chain = &self.chains[index];
    let columns = self.columns.unwrap_or(chain.len);
    let start = (row - chain.first) * columns;
    Some(ResidueSpan {
      chain: chain.chain,
      range: start..(start + columns).min(chain.len),
    })
  }
}

pub(super) fn sequence_width(width: Pixels) -> Pixels {
  (width - px(2.0 * SIDE_PADDING + LABEL_WIDTH + ROW_GAP)).max(px(CELL_WIDTH))
}

pub(super) fn wrap_columns(width: Pixels) -> usize {
  (f32::from(sequence_width(width)) / CELL_WIDTH).floor().max(1.0) as usize
}

/// Keep one neighboring cell on each side to avoid gaps during fractional scrolling.
pub(super) fn visible_columns(len: usize, offset: Pixels, width: Pixels) -> Range<usize> {
  let left = (-f32::from(offset)).max(0.0);
  let first = (left / CELL_WIDTH).floor() as usize;
  let end = ((left + f32::from(width).max(CELL_WIDTH)) / CELL_WIDTH).ceil() as usize;
  first.saturating_sub(1).min(len)..end.saturating_add(1).min(len)
}

/// One visible row measurement replaces individual residue hit boxes.
#[derive(Clone)]
pub(super) struct RowGeometry {
  pub span: ResidueSpan,
  /// Origin of the complete strip, including its scrolled-off columns.
  pub origin: Point<Pixels>,
  pub clip: Bounds<Pixels>,
}

impl RowGeometry {
  pub fn hit(&self, rectangle: Bounds<Pixels>) -> Option<ResidueSpan> {
    if self.clip.size.width <= px(0.0) || self.clip.size.height <= px(0.0) {
      return None;
    }
    let click = rectangle.size.width <= px(3.0) && rectangle.size.height <= px(3.0);
    let (left, right) = if click {
      let point = rectangle.origin;
      if point.x < self.clip.left()
        || point.x >= self.clip.right()
        || point.y < self.clip.top()
        || point.y >= self.clip.bottom()
      {
        return None;
      }
      (point.x, point.x)
    } else {
      let rectangle = Bounds::new(
        rectangle.origin,
        gpui::size(rectangle.size.width.max(px(1.0)), rectangle.size.height.max(px(1.0))),
      );
      if !self.clip.intersects(&rectangle) {
        return None;
      }
      let intersection = self.clip.intersect(&rectangle);
      (intersection.left(), intersection.right())
    };
    let len = self.span.range.len();
    let start = (f32::from(left - self.origin.x).max(0.0) / CELL_WIDTH).floor() as usize;
    let end = if click {
      start + 1
    } else {
      (f32::from(right - self.origin.x).max(0.0) / CELL_WIDTH).ceil() as usize
    };
    let start = start.min(len);
    let end = end.min(len);
    (start < end).then_some(ResidueSpan {
      chain: self.span.chain,
      range: self.span.range.start + start..self.span.range.start + end,
    })
  }
}

fn one_letter(name: &str) -> char {
  match name.to_ascii_uppercase().as_str() {
    "ALA" => 'A',
    "ARG" => 'R',
    "ASN" => 'N',
    "ASP" => 'D',
    "CYS" => 'C',
    "GLN" => 'Q',
    "GLU" => 'E',
    "GLY" => 'G',
    "HIS" => 'H',
    "ILE" => 'I',
    "LEU" => 'L',
    "LYS" => 'K',
    "MET" | "MSE" => 'M',
    "PHE" => 'F',
    "PRO" => 'P',
    "SER" => 'S',
    "THR" => 'T',
    "TRP" => 'W',
    "TYR" => 'Y',
    "VAL" => 'V',
    "SEC" => 'U',
    "PYL" => 'O',
    "DA" | "A" => 'A',
    "DC" | "C" => 'C',
    "DG" | "G" => 'G',
    "DT" | "T" => 'T',
    "U" => 'U',
    _ => 'X',
  }
}

#[cfg(test)]
mod tests {

  #[test]
  fn projection_preserves_polymer_order_and_author_tooltips() {
    use super::*;
    use chitin_bio::structure::PdbParser;
    let parsed = PdbParser::new()
      .parse_bytes(
        concat!(
          "ATOM      1  CA  GLY A  10       0.000   0.000   0.000  1.00 10.00           C  \n",
          "ATOM      2  CA  ALA A  11       1.000   0.000   0.000  1.00 10.00           C  \n",
          "HETATM    3  O   HOH A  12       2.000   0.000   0.000  1.00 10.00           O  \nEND\n",
        )
        .as_bytes(),
      )
      .unwrap_or_else(|error| panic!("fixture: {error}"));
    let scene = StructureScene::from_first_model(&parsed.structure).unwrap_or_else(|error| panic!("scene: {error}"));
    let data = SequenceData::project(&scene);
    assert_eq!(
      data.chains[0]
        .residues
        .iter()
        .map(|residue| residue.letter)
        .collect::<String>(),
      "GA"
    );
    assert_eq!(data.chains[0].residues[0].tooltip, "GLY 10 · chain A");
  }

  #[test]
  fn unknown_polymer_code_uses_x() {
    use super::one_letter;
    assert_eq!(one_letter("A2G"), 'X');
  }

  #[test]
  fn million_residue_chain_mounts_only_the_visible_columns() {
    use super::*;
    assert_eq!(visible_columns(1_000_000, px(-10_000.0), px(1_000.0)), 499..551);
  }

  #[test]
  fn wrapped_chain_boundaries_map_to_correct_residue_ranges() {
    use super::*;
    let layout = SequenceLayout::new([(0, 99), (1, 4)].into_iter(), Some(40));
    assert_eq!(
      layout.row(2),
      Some(ResidueSpan {
        chain: 0,
        range: 80..99
      })
    );
    assert_eq!(layout.row(3), Some(ResidueSpan { chain: 1, range: 0..4 }));
    assert_eq!(layout.row(4), None);
  }

  #[test]
  fn many_long_chains_need_only_one_prefix_record_per_chain() {
    use super::*;
    let layout = SequenceLayout::new((0..1_000).map(|chain| (chain, 10_000)), Some(50));
    assert_eq!(layout.chains.len(), 1_000);
    assert_eq!(layout.count, 200_000);
    assert_eq!(
      layout.row(199_999),
      Some(ResidueSpan {
        chain: 999,
        range: 9_950..10_000
      })
    );
  }

  #[test]
  fn scroll_and_clip_define_hit_columns_without_per_residue_boxes() {
    use super::*;
    let row = RowGeometry {
      span: ResidueSpan {
        chain: 1,
        range: 0..1_000,
      },
      origin: gpui::point(px(-200.0), px(40.0)),
      clip: Bounds::new(gpui::point(px(80.0), px(40.0)), gpui::size(px(200.0), px(36.0))),
    };
    let rectangle = Bounds::new(gpui::point(px(60.0), px(50.0)), gpui::size(px(80.0), px(0.0)));
    assert_eq!(
      row.hit(rectangle),
      Some(ResidueSpan {
        chain: 1,
        range: 14..17
      })
    );
    assert_eq!(
      row.hit(Bounds::new(gpui::point(px(20.0), px(50.0)), Default::default())),
      None
    );
  }

  #[test]
  fn wrapped_hit_is_offset_to_the_correct_sequence_positions() {
    use super::*;
    let row = RowGeometry {
      span: ResidueSpan {
        chain: 0,
        range: 40..80,
      },
      origin: gpui::point(px(80.0), px(50.0)),
      clip: Bounds::new(gpui::point(px(80.0), px(50.0)), gpui::size(px(800.0), px(36.0))),
    };
    assert_eq!(
      row.hit(Bounds::new(gpui::point(px(100.0), px(55.0)), Default::default())),
      Some(ResidueSpan {
        chain: 0,
        range: 41..42
      })
    );
    assert_eq!(
      row.hit(Bounds::new(gpui::point(px(100.0), px(10.0)), Default::default())),
      None
    );
  }
}
