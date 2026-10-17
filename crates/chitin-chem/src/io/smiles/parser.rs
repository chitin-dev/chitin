//! Iterative topology assembly. Branch nesting never consumes the call stack.

use std::{collections::BTreeMap, ops::Range};

use crate::{AtomId, BondOrder, MolecularGraph};

use super::{
  ParsedSmiles, SmilesError, SmilesErrorKind, error,
  syntax::{Kind, Token},
};

struct Branch {
  parent: AtomId,
  atom_count: usize,
  span: Range<usize>,
}

struct Ring {
  atom: AtomId,
  order: Option<BondOrder>,
  span: Range<usize>,
}

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
  fn is_endpoint(self) -> bool {
    matches!(self, Self::Atom | Self::Ring | Self::Close)
  }
}

pub(super) fn build(source: &str, tokens: Vec<Token>) -> Result<ParsedSmiles, SmilesError> {
  let mut result = ParsedSmiles {
    source: source.into(),
    graph: MolecularGraph::new(),
    atom_spans: BTreeMap::new(),
    bond_spans: BTreeMap::new(),
  };
  let mut current = None;
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
