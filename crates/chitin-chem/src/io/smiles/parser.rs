//! Iterative topology assembly. Branch nesting never consumes the call stack.

use std::{collections::BTreeMap, ops::Range};

use crate::{AtomId, BondOrder, MolecularGraph};

use super::{
  ParsedSmiles, SmilesError, SmilesErrorKind, error,
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
  /// Live atom at the first occurrence of this numeric label.
  atom: AtomId,
  /// Explicit order, if supplied; absence defers to the other site or single.
  order: Option<BondOrder>,
  /// First site's bond-symbol/label range, retained for provenance and errors.
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
  let mut pending: Option<(BondOrder, Range<usize>)> = None;
  let mut branches: Vec<Branch> = Vec::new();
  let mut rings: BTreeMap<u8, Ring> = BTreeMap::new();
  let mut previous = Previous::Start;
  // Grammar places ring labels before an atom's branches. A bond after a
  // completed branch may lead to an atom, but cannot resume its ring labels.
  let mut ring_allowed = false;
  for Token { span, kind } in tokens {
    let syntax = |message| error(span.clone(), SmilesErrorKind::Syntax(message));
    match kind {
      Kind::Atom(atom) => {
        let id = result.graph.add_atom(atom).map_err(|e| error(span.clone(), e.into()))?;
        if let Some(parent) = current {
          let (order, bond_span) = pending.take().unwrap_or((BondOrder::Single, span.start..span.start));
          let edge = result
            .graph
            .add_bond(parent, id, order)
            .map_err(|e| error(span.clone(), e.into()))?;
          result.bond_spans.insert(edge, vec![bond_span]);
        }
        result.atom_spans.insert(id, span);
        current = Some(id);
        previous = Previous::Atom;
        ring_allowed = true;
      }
      Kind::Bond(order) => {
        if !(previous.is_endpoint() || matches!(previous, Previous::Open)) || current.is_none() {
          return Err(syntax(
            "a bond needs a preceding atom and a following atom or ring label",
          ));
        }
        pending = Some((order, span));
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
        let (order, site) = match pending.take() {
          Some((order, bond_span)) => (Some(order), bond_span.start..span.end),
          None => (None, span.clone()),
        };
        if let Some(open) = rings.remove(&label) {
          if open.order.is_some() && order.is_some() && open.order != order {
            return Err(syntax("conflicting bond orders at ring closure"));
          }
          // Ring labels denote pending edges, not perceived cycles. They may
          // legally connect across a dot and may be reused after closure.
          let edge = result
            .graph
            .add_bond(open.atom, atom, order.or(open.order).unwrap_or(BondOrder::Single))
            .map_err(|e| error(span.clone(), e.into()))?;
          result.bond_spans.insert(edge, vec![open.span, site]);
        } else {
          rings.insert(
            label,
            Ring {
              atom,
              order,
              span: site,
            },
          );
        }
        previous = Previous::Ring;
      }
    }
  }
  let eof = source.len()..source.len();
  if let Some((_, span)) = pending {
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
  Ok(result)
}
