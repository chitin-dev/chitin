//! Layer appearance independent of geometry, surface generation, and UI state.

/// Coloring policy, with sRGB bytes for a uniform color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
  Uniform,
  Element,
  Chain,
  ChainElement,
}

/// Persistent appearance of one layer, including when its style is disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerAppearance {
  pub scheme: ColorScheme,
  pub color: [u8; 3],
  opacity: u16,
  palette_uniform: bool,
}

impl LayerAppearance {
  pub const fn new(scheme: ColorScheme) -> Self {
    Self {
      scheme,
      color: [226, 219, 206],
      opacity: 10_000,
      palette_uniform: true,
    }
  }
  pub fn opacity(self) -> f32 {
    self.opacity as f32 / 10_000.0
  }
  /// Rejects non-finite or out-of-range opacity, preserving the existing value.
  pub fn with_opacity(mut self, value: f32) -> Option<Self> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
      return None;
    }
    self.opacity = (value * 10_000.0).round() as u16;
    Some(self)
  }
  pub fn linear_color(self) -> [f32; 3] {
    self.color.map(srgb_to_linear)
  }
  /// Uses a custom sRGB uniform color instead of the renderer's carbon palette.
  pub fn with_color(mut self, color: [u8; 3]) -> Self {
    self.color = color;
    self.palette_uniform = false;
    self
  }
  pub(crate) fn same_coloring(self, other: Self) -> bool {
    self.scheme == other.scheme && self.color == other.color && self.palette_uniform == other.palette_uniform
  }
}

pub(crate) fn srgb_to_linear(value: u8) -> f32 {
  let value = value as f32 / 255.0;
  if value <= 0.04045 {
    value / 12.92
  } else {
    ((value + 0.055) / 1.055).powf(2.4)
  }
}

/// Deterministic chain palette, independent of visible-chain ordering.
pub(crate) fn chain_color(chain: usize) -> [f32; 3] {
  const COLORS: [[u8; 3]; 12] = [
    [93, 173, 226],
    [245, 176, 65],
    [88, 214, 141],
    [195, 155, 211],
    [236, 112, 99],
    [72, 201, 176],
    [244, 208, 63],
    [133, 146, 158],
    [220, 118, 51],
    [165, 105, 189],
    [174, 214, 241],
    [169, 223, 191],
  ];
  COLORS[chain % COLORS.len()].map(srgb_to_linear)
}

use crate::BallAndStickPalette;
use chitin_bio::structure::{AtomSceneInstance, ElementCategory, StructureScene};
use chitin_bio::surface::{MolecularSurfaceArtifact, SurfaceGeometrySource, select_surface_scene_atoms};

struct Node {
  atom: AtomSceneInstance,
  axis: usize,
  left: Option<usize>,
  right: Option<usize>,
}

#[derive(Clone, Copy)]
struct ColorOwner {
  chain: u32,
  element: ElementCategory,
}
impl From<AtomSceneInstance> for ColorOwner {
  fn from(atom: AtomSceneInstance) -> Self {
    Self {
      chain: atom.chain_id.index() as u32,
      element: atom.element,
    }
  }
}

/// Spatial ownership is cached when geometry changes, not on every recoloring.
pub(crate) struct ColorCache {
  nodes: Vec<Node>,
  root: Option<usize>,
  pub atoms: Vec<[f32; 8]>,
  pub bonds: Vec<[f32; 16]>,
  pub cartoon: Vec<[f32; 9]>,
  pub surface: Vec<[f32; 9]>,
  atom_owners: Vec<Option<ColorOwner>>,
  bond_owners: Vec<[Option<ColorOwner>; 2]>,
  bond_element_colors: Vec<[[f32; 3]; 2]>,
  cartoon_owners: Vec<Option<ColorOwner>>,
  surface_owners: Vec<Option<ColorOwner>>,
  cartoon_chains: Vec<usize>,
}

impl ColorCache {
  pub fn new(scene: &StructureScene) -> Self {
    Self::from_atoms(scene.atoms.clone())
  }
  fn from_atoms(mut atoms: Vec<AtomSceneInstance>) -> Self {
    let mut cache = Self {
      nodes: Vec::with_capacity(atoms.len()),
      root: None,
      atoms: vec![],
      bonds: vec![],
      cartoon: vec![],
      surface: vec![],
      atom_owners: vec![],
      bond_owners: vec![],
      bond_element_colors: vec![],
      cartoon_owners: vec![],
      surface_owners: vec![],
      cartoon_chains: vec![],
    };
    cache.root = cache.build(&mut atoms, 0);
    cache
  }
  fn build(&mut self, atoms: &mut [AtomSceneInstance], depth: usize) -> Option<usize> {
    if atoms.is_empty() {
      return None;
    }
    let axis = depth % 3;
    let mid = atoms.len() / 2;
    atoms.select_nth_unstable_by(mid, |a, b| {
      a.position[axis]
        .total_cmp(&b.position[axis])
        .then(a.atom_id.index().cmp(&b.atom_id.index()))
    });
    let atom = atoms[mid];
    let (left, right) = atoms.split_at_mut(mid);
    let left = self.build(left, depth + 1);
    let right = self.build(&mut right[1..], depth + 1);
    let id = self.nodes.len();
    self.nodes.push(Node {
      atom,
      axis,
      left,
      right,
    });
    Some(id)
  }
  fn nearest(&self, position: [f32; 3]) -> Option<AtomSceneInstance> {
    fn visit(cache: &ColorCache, id: Option<usize>, p: [f32; 3], best: &mut (f32, Option<AtomSceneInstance>)) {
      let Some(id) = id else {
        return;
      };
      let node = &cache.nodes[id];
      let distance = (0..3)
        .map(|axis| (p[axis] - node.atom.position[axis]).powi(2))
        .sum::<f32>();
      if distance < best.0
        || (distance == best.0
          && best
            .1
            .is_none_or(|atom| node.atom.atom_id.index() < atom.atom_id.index()))
      {
        *best = (distance, Some(node.atom));
      }
      let delta = p[node.axis] - node.atom.position[node.axis];
      let (near, far) = if delta < 0.0 {
        (node.left, node.right)
      } else {
        (node.right, node.left)
      };
      visit(cache, near, p, best);
      if delta * delta <= best.0 {
        visit(cache, far, p, best);
      }
    }
    let mut best = (f32::INFINITY, None);
    visit(self, self.root, position, &mut best);
    best.1
  }
  pub fn set_atoms(&mut self, rows: Vec<[f32; 8]>) {
    self.atom_owners = rows
      .iter()
      .map(|row| self.nearest([row[0], row[1], row[2]]).map(ColorOwner::from))
      .collect();
    self.atoms = rows;
  }
  pub fn set_bonds(&mut self, rows: Vec<[f32; 16]>) {
    self.bond_element_colors = rows
      .iter()
      .map(|row| [[row[8], row[9], row[10]], [row[12], row[13], row[14]]])
      .collect();
    self.bond_owners = rows
      .iter()
      .map(|row| {
        [
          self.nearest([row[0], row[1], row[2]]).map(ColorOwner::from),
          self.nearest([row[4], row[5], row[6]]).map(ColorOwner::from),
        ]
      })
      .collect();
    self.bonds = rows;
  }
  pub fn set_cartoon(&mut self, rows: Vec<[f32; 9]>, chains: Vec<usize>) {
    self.cartoon_owners = rows
      .iter()
      .map(|row| self.nearest([row[0], row[1], row[2]]).map(ColorOwner::from))
      .collect();
    self.cartoon = rows;
    self.cartoon_chains = chains;
  }
  pub fn set_surface(&mut self, rows: Vec<[f32; 9]>, artifact: Option<&MolecularSurfaceArtifact>) {
    self.surface_owners = if let Some(artifact) = artifact {
      let scope = match artifact.source {
        SurfaceGeometrySource::ImplicitGrid(request) => request.atom_scope,
        SurfaceGeometrySource::Msms { atom_scope, .. } => atom_scope,
      };
      let atoms = self.nodes.iter().map(|node| node.atom).collect::<Vec<_>>();
      let eligible = select_surface_scene_atoms(&atoms, scope).copied().collect::<Vec<_>>();
      let mut owners = Vec::with_capacity(rows.len());
      for domain in &artifact.domains {
        // A per-chain mesh must never inherit an overlapping neighbor's chain.
        let tree = Self::from_atoms(
          eligible
            .iter()
            .filter(|atom| domain.chain_id.is_none_or(|chain| atom.chain_id == chain))
            .copied()
            .collect(),
        );
        let end = owners.len() + domain.mesh.vertices.len();
        owners.extend(
          rows[owners.len()..end]
            .iter()
            .map(|row| tree.nearest([row[0], row[1], row[2]]).map(ColorOwner::from)),
        );
      }
      owners.resize(rows.len(), None);
      owners
    } else {
      rows
        .iter()
        .map(|row| self.nearest([row[0], row[1], row[2]]).map(ColorOwner::from))
        .collect()
    };
    self.surface = rows;
  }
  pub fn recolor(&mut self, index: usize, appearance: LayerAppearance, palette: &BallAndStickPalette) {
    fn color(
      owner: Option<ColorOwner>,
      chain: Option<usize>,
      appearance: LayerAppearance,
      palette: &BallAndStickPalette,
    ) -> [f32; 3] {
      let element = owner.map_or(ElementCategory::Carbon, |atom| atom.element);
      match appearance.scheme {
        ColorScheme::Uniform => {
          if appearance.palette_uniform {
            palette.carbon.color
          } else {
            appearance.linear_color()
          }
        }
        ColorScheme::Element => palette.for_element(element).color,
        ColorScheme::ChainElement if element != ElementCategory::Carbon => palette.for_element(element).color,
        ColorScheme::Chain | ColorScheme::ChainElement => chain_color(
          chain
            .or_else(|| owner.map(|atom| atom.chain as usize))
            .unwrap_or_default(),
        ),
      }
    }
    match index {
      0 => {
        for (row, owner) in self.atoms.iter_mut().zip(&self.atom_owners) {
          row[4..7].copy_from_slice(&color(*owner, None, appearance, palette));
        }
        for (index, (row, owners)) in self.bonds.iter_mut().zip(&self.bond_owners).enumerate() {
          if appearance.scheme == ColorScheme::Element {
            row[8..11].copy_from_slice(&self.bond_element_colors[index][0]);
            row[12..15].copy_from_slice(&self.bond_element_colors[index][1]);
          } else {
            row[8..11].copy_from_slice(&color(owners[0], None, appearance, palette));
            row[12..15].copy_from_slice(&color(owners[1], None, appearance, palette));
          }
        }
      }
      1 => {
        for (index, (row, owner)) in self.cartoon.iter_mut().zip(&self.cartoon_owners).enumerate() {
          row[6..9].copy_from_slice(&color(
            *owner,
            self.cartoon_chains.get(index).copied(),
            appearance,
            palette,
          ));
        }
      }
      _ => {
        for (row, owner) in self.surface.iter_mut().zip(&self.surface_owners) {
          row[6..9].copy_from_slice(&color(*owner, None, appearance, palette));
        }
      }
    }
  }
}
