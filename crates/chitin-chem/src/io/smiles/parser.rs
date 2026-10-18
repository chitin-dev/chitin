//! Iterative topology assembly. Branch nesting never consumes the call stack.

use std::{collections::BTreeMap, ops::Range};

use crate::{AtomId, BondOrder, MolecularGraph, StereoLigand, TetrahedralStereo, TetrahedralWinding};

use super::{
  ParsedSmiles, SmilesError, SmilesErrorKind,
  directional::Direction,
  error,
  syntax::{Kind, Token},
};

/// Saved parent and opening location for one unfinished, nonrecursive branch.
struct Branch {
  /// Restored after closing the branch, even if a dot disconnected its contents.
  parent: AtomId,
  /// Vertex-count checkpoint used to reject a branch that added no atoms.
  atom_count: usize,
  /// Opening parenthesis range for an unclosed-branch diagnostic.
  span: Range<usize>,
}

/// One unmatched ring site; this is a pending edge, not a perceived chemical ring.
struct Ring {
  /// Reserved neighbor-order position at the opening site, not closure time.
  slot: usize,
  /// Live atom at the first occurrence of this numeric label.
  atom: AtomId,
  /// Explicit order, if supplied; absence defers to the other site or single.
  order: Option<BondOrder>,
  /// Direction in opening-to-closing traversal, if explicitly marked.
  direction: Option<bool>,
  /// First site's bond-symbol/label range, retained for provenance and errors.
  span: Range<usize>,
}

/// A chiral token awaiting all future branch and ring ligand identities.
struct StereoSite {
  /// Live atom whose complete neighborhood is checked after topology assembly.
  center: AtomId,
  /// Source winding, still relative to lexical neighbor encounter order.
  winding: TetrahedralWinding,
  /// Virtual H goes after an incoming atom, or first at a component root.
  hydrogen_slot: Option<usize>,
  /// Whole bracket token used for unsupported-coordination diagnostics.
  span: Range<usize>,
}

/// Most recent grammar category; carries no chemical meaning or graph ownership.
#[derive(Clone, Copy)]
enum Previous {
  Start,
  Atom,
  Ring,
  Open,
  Close,
  Bond,
  Dot,
}

impl Previous {
  /// Returns whether the last category leaves a complete current atom endpoint.
  fn is_endpoint(self) -> bool {
    matches!(self, Self::Atom | Self::Ring | Self::Close)
  }
}

/// Assembles a detached graph with explicit branch and pending-ring state.
///
/// # Parameters
/// - `source`: exact input whose byte ranges are carried by `tokens`.
/// - `tokens`: ordered output of the successful lexical scan of that same input.
///
/// # Returns
/// A complete graph plus provenance, without valence/ring/aromaticity perception.
/// Dots clear only the current chain; ring labels can legally connect across dots.
/// Branch closure restores its saved parent. Explicit orders at two ring sites
/// must agree, and a closed label is available for later reuse.
///
/// # Errors
/// Returns positioned grammar or simple-graph failures, including self/duplicate
/// ring edges and unfinished state at end-of-input. All construction is local:
/// any failure drops the partial graph instead of exposing a usable prefix.
pub(super) fn build(source: &str, tokens: Vec<Token>) -> Result<ParsedSmiles, SmilesError> {
  let mut result = ParsedSmiles {
    source: source.into(),
    graph: MolecularGraph::new(),
    atom_spans: BTreeMap::new(),
    bond_spans: BTreeMap::new(),
  };
  let mut current = None;
  // At most one explicit bond can await an endpoint. Atom/ring handling takes
  // it exactly once; grammar checks prevent overwriting it with another bond.
  let mut pending: Option<(BondOrder, Option<bool>, Range<usize>)> = None;
  let mut directions = BTreeMap::new();
  let mut branches: Vec<Branch> = Vec::new();
  let mut rings: BTreeMap<u8, Ring> = BTreeMap::new();
  let mut previous = Previous::Start;
  // Ring openings reserve slots immediately. Filling them only at closure
  // would change tetrahedral parity whenever a branch lies between the sites.
  let mut neighbors: BTreeMap<AtomId, Vec<Option<AtomId>>> = BTreeMap::new();
  let mut stereo = Vec::new();
  // Grammar places ring labels before an atom's branches. A bond after a
  // completed branch may lead to an atom, but cannot resume its ring labels.
  let mut ring_allowed = false;
  for Token { span, kind } in tokens {
    let syntax = |message| error(span.clone(), SmilesErrorKind::Syntax(message));
    match kind {
      Kind::Atom(atom, winding) => {
        let declared = atom.declared_hydrogens();
        let id = result.graph.add_atom(atom).map_err(|e| error(span.clone(), e.into()))?;
        neighbors.insert(id, current.into_iter().map(Some).collect());
        if let Some(winding) = winding {
          stereo.push(StereoSite {
            center: id,
            winding,
            hydrogen_slot: (declared == Some(1)).then_some(usize::from(current.is_some())),
            span: span.clone(),
          });
        }
        if let Some(parent) = current {
          if let Some(order) = neighbors.get_mut(&parent) {
            order.push(Some(id));
          }
          let default = default_order(&result.graph, parent, id);
          let (order, direction, bond_span) = pending.take().unwrap_or((default, None, span.start..span.start));
          let edge = result
            .graph
            .add_bond(parent, id, order)
            .map_err(|e| error(span.clone(), e.into()))?;
          if let Some(slash) = direction {
            directions.insert(
              edge,
              Direction {
                ends: [parent, id],
                slash,
                span: bond_span.clone(),
              },
            );
          }
          result.bond_spans.insert(edge, vec![bond_span]);
        }
        result.atom_spans.insert(id, span);
        current = Some(id);
        previous = Previous::Atom;
        ring_allowed = true;
      }
      Kind::Bond(order, direction) => {
        if !(previous.is_endpoint() || matches!(previous, Previous::Open)) || current.is_none() {
          return Err(syntax(
            "a bond needs a preceding atom and a following atom or ring label",
          ));
        }
        pending = Some((order, direction, span));
        previous = Previous::Bond;
      }
      Kind::Open => {
        if !previous.is_endpoint() {
          return Err(syntax("a branch needs a preceding atom"));
        }
        let parent = current.ok_or_else(|| syntax("a branch needs a parent"))?;
        branches.push(Branch {
          parent,
          atom_count: result.graph.atom_count(),
          span,
        });
        previous = Previous::Open;
        ring_allowed = false;
      }
      Kind::Close => {
        if !previous.is_endpoint() {
          return Err(syntax("empty or incomplete branch"));
        }
        let branch = branches.pop().ok_or_else(|| syntax("unmatched ')'"))?;
        if result.graph.atom_count() == branch.atom_count {
          return Err(syntax("empty branch"));
        }
        current = Some(branch.parent);
        previous = Previous::Close;
        ring_allowed = false;
      }
      Kind::Dot => {
        if !previous.is_endpoint() && !matches!(previous, Previous::Open) {
          return Err(syntax("a dot must separate atom-containing chains"));
        }
        current = None;
        previous = Previous::Dot;
        ring_allowed = false;
      }
      Kind::Ring(label) => {
        if !ring_allowed || !(previous.is_endpoint() || matches!(previous, Previous::Bond)) {
          return Err(syntax("ring labels must follow an atom and precede its branches"));
        }
        let atom = current.ok_or_else(|| syntax("a ring label needs an atom"))?;
        let (order, direction, site) = match pending.take() {
          Some((order, direction, bond_span)) => (Some(order), direction, bond_span.start..span.end),
          None => (None, None, span.clone()),
        };
        if let Some(open) = rings.remove(&label) {
          if open.order.is_some() && order.is_some() && open.order != order {
            return Err(syntax("conflicting bond orders at ring closure"));
          }
          if open.direction.zip(direction).is_some_and(|(a, b)| a == b) {
            return Err(syntax("conflicting directions at ring closure"));
          }
          // Ring labels denote pending edges, not perceived cycles. They may
          // legally connect across a dot and may be reused after closure.
          let edge = result
            .graph
            .add_bond(
              open.atom,
              atom,
              order
                .or(open.order)
                .unwrap_or_else(|| default_order(&result.graph, open.atom, atom)),
            )
            .map_err(|e| error(span.clone(), e.into()))?;
          if let Some(slash) = open.direction.or(direction.map(|value| !value)) {
            directions.insert(
              edge,
              Direction {
                ends: [open.atom, atom],
                slash,
                span: site.clone(),
              },
            );
          }
          result.bond_spans.insert(edge, vec![open.span, site]);
          if let Some(order) = neighbors.get_mut(&open.atom) {
            order[open.slot] = Some(atom);
          }
          if let Some(order) = neighbors.get_mut(&atom) {
            order.push(Some(open.atom));
          }
        } else {
          let neighbor_order = neighbors
            .get_mut(&atom)
            .ok_or_else(|| syntax("missing ring endpoint"))?;
          let slot = neighbor_order.len();
          neighbor_order.push(None);
          rings.insert(
            label,
            Ring {
              slot,
              atom,
              order,
              direction,
              span: site,
            },
          );
        }
        previous = Previous::Ring;
      }
    }
  }
  let eof = source.len()..source.len();
  if let Some((_, _, span)) = pending {
    return Err(error(span, SmilesErrorKind::Syntax("bond has no following endpoint")));
  }
  if let Some(branch) = branches.first() {
    return Err(error(branch.span.clone(), SmilesErrorKind::Syntax("unclosed branch")));
  }
  if let Some(ring) = rings.values().min_by_key(|ring| ring.span.start) {
    return Err(error(ring.span.clone(), SmilesErrorKind::Syntax("unclosed ring label")));
  }
  if matches!(previous, Previous::Dot) {
    return Err(error(eof, SmilesErrorKind::Syntax("expected atom after dot")));
  }
  for StereoSite {
    center: id,
    winding,
    hydrogen_slot,
    span,
  } in stereo
  {
    let mut ligands: Vec<_> = neighbors[&id]
      .iter()
      .filter_map(|id| id.map(StereoLigand::Atom))
      .collect();
    if let Some(slot) = hydrogen_slot {
      ligands.insert(slot, StereoLigand::Hydrogen);
    }
    let ligands: [_; 4] = ligands.try_into().map_err(|_| {
      error(
        span.clone(),
        SmilesErrorKind::Stereo(crate::StereoError::Coordination(id)),
      )
    })?;
    let descriptor = TetrahedralStereo::new(ligands, winding).map_err(|e| error(span.clone(), e.into()))?;
    result
      .graph
      .set_tetrahedral(id, Some(descriptor))
      .map_err(|e| error(span, e.into()))?;
  }
  super::directional::install(&mut result.graph, directions)?;
  Ok(result)
}

/// Omitted bonds are aromatic only when both endpoint designations are aromatic.
fn default_order(graph: &MolecularGraph, a: AtomId, b: AtomId) -> BondOrder {
  if [a, b]
    .into_iter()
    .all(|id| graph.atom(id).is_some_and(|atom| atom.is_aromatic()))
  {
    BondOrder::Aromatic
  } else {
    BondOrder::Single
  }
}
