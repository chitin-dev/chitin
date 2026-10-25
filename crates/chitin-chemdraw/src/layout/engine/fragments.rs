//! Value-owned rigid components, stable main-fragment priority and oriented ports.
//! Source attribution and redistribution terms are in licenses/coordinate-generation.txt.

use super::{Error, Work, topology::Model};
use std::collections::{BTreeMap, VecDeque};

/// One fragment's atoms and internal edges, in stable source construction order.
pub(super) struct Fragment {
  pub atoms: Vec<usize>,
  pub edges: Vec<usize>,
  pub rings: Vec<usize>,
  pub ports: Vec<usize>,
  pub chain: bool,
  pub parent: Option<Connection>,
  pub children: Vec<usize>,
}

/// Parent/child orientation belongs to layout, never to a mutated chemical bond.
#[derive(Clone, Copy, Debug)]
pub(super) struct Connection {
  pub fragment: usize,
  pub edge: usize,
  pub parent_atom: usize,
  pub child_atom: usize,
}

/// A forest covers disconnected inputs and isolated atoms without synthetic edges.
pub(super) struct Forest {
  pub fragments: Vec<Fragment>,
  pub owner: Vec<usize>,
  pub order: Vec<usize>,
  pub roots: Vec<usize>,
}

impl Forest {
  /// Splits only internal-to-internal acyclic non-double connections.
  ///
  /// # Returns
  /// The source's terminal-group, ring and double-bond fragment partition. Every
  /// live atom has exactly one owner; every connecting bond becomes a tree port.
  /// Spiro rings remain in one fragment, while disconnected salts remain separate.
  ///
  /// # Errors
  /// Exhausted traversal allowance or inconsistent contracted connectivity fails
  /// before any source coordinates or bonds can be modified.
  pub(super) fn new(model: &Model, work: &mut Work) -> Result<Self, Error> {
    work.charge(model.atoms.len() + model.edges.len())?;
    let mut owner = vec![usize::MAX; model.atoms.len()];
    let mut groups = Vec::<Vec<usize>>::new();
    for edge in &model.edges {
      work.charge(1)?;
      let [a, b] = edge.ends;
      let cut = !edge.cyclic && edge.order != 2 && model.neighbors[a].len() > 1 && model.neighbors[b].len() > 1;
      if cut {
        for node in [a, b] {
          if owner[node] == usize::MAX {
            owner[node] = groups.len();
            groups.push(vec![node]);
          }
        }
        continue;
      }
      match (owner[a] == usize::MAX, owner[b] == usize::MAX) {
        (true, true) => {
          owner[a] = groups.len();
          owner[b] = groups.len();
          groups.push(vec![a, b]);
          continue;
        }
        (false, true) => {
          owner[b] = owner[a];
          groups[owner[a]].push(b);
          continue;
        }
        (true, false) => {
          owner[a] = owner[b];
          groups[owner[b]].push(a);
          continue;
        }
        (false, false) => {}
      }
      if owner[a] == owner[b] {
        continue;
      }
      let (keep, remove) = (owner[a], owner[b]);
      work.charge(groups[remove].len())?;
      let moved = std::mem::take(&mut groups[remove]);
      for node in moved {
        owner[node] = keep;
        groups[keep].push(node);
      }
    }
    for (node, entry) in owner.iter_mut().enumerate() {
      if *entry == usize::MAX {
        *entry = groups.len();
        groups.push(vec![node]);
      }
    }
    let mut mapping = BTreeMap::new();
    let mut fragments = Vec::new();
    for (old, atoms) in groups.into_iter().enumerate().filter(|(_, atoms)| !atoms.is_empty()) {
      mapping.insert(old, fragments.len());
      fragments.push(Fragment {
        atoms,
        edges: Vec::new(),
        rings: Vec::new(),
        ports: Vec::new(),
        chain: false,
        parent: None,
        children: Vec::new(),
      });
    }
    for entry in &mut owner {
      *entry = mapping[entry];
    }
    for (i, edge) in model.edges.iter().enumerate() {
      let [a, b] = edge.ends.map(|node| owner[node]);
      if a == b {
        fragments[a].edges.push(i);
      } else {
        fragments[a].ports.push(i);
        fragments[b].ports.push(i);
      }
    }
    for (i, ring) in model.cycles.iter().enumerate() {
      let fragment = owner[ring[0]];
      if ring.iter().any(|&node| owner[node] != fragment) {
        return Err(Error::Invariant(model.atoms[ring[0]].id));
      }
      fragments[fragment].rings.push(i);
    }
    for fragment in &mut fragments {
      fragment.chain = fragment.atoms.len() <= 3
        && fragment.rings.is_empty()
        && fragment.atoms.iter().all(|&node| model.neighbors[node].len() <= 3)
        && fragment.edges.iter().all(|&edge| model.edges[edge].order <= 2);
    }
    let mut forest = Self {
      fragments,
      owner,
      roots: Vec::new(),
      order: Vec::new(),
    };
    for component in &model.components {
      let mut seen = std::collections::BTreeSet::new();
      let candidates: Vec<_> = component
        .iter()
        .map(|&node| forest.owner[node])
        .filter(|id| seen.insert(*id))
        .collect();
      let Some(&first) = candidates.iter().min() else {
        continue;
      };
      // Source fragment creation order, not sorted atom handles, breaks ties.
      let mut candidates = candidates;
      candidates.sort_unstable();
      let root = candidates.iter().copied().fold(first, |best, id| {
        if forest.priority(id, model) > forest.priority(best, model) {
          id
        } else {
          best
        }
      });
      let chain = forest.longest_chain(&candidates, model, work)?;
      let threshold = match forest.fragments[root].rings.len() {
        0 => 1,
        1 => 5,
        2 => 8,
        3 => 10,
        _ => 12,
      };
      let root = if chain.len() >= threshold { chain[0] } else { root };
      forest.roots.push(root);
      forest.orient(root, model, work)?;
    }
    if forest.order.len() != forest.fragments.len() {
      return Err(Error::Invariant(model.atoms[0].id));
    }
    Ok(forest)
  }

  /// Source priority uses non-carbon count, despite its original heavy-atom label.
  fn priority(&self, id: usize, model: &Model) -> [usize; 6] {
    let f = &self.fragments[id];
    [
      f.rings.len(),
      f.atoms.len(),
      f.ports.len(),
      f.atoms.iter().filter(|&&i| model.atoms[i].number != 6).count(),
      f.atoms
        .iter()
        .map(|&i| usize::from(model.atoms[i].number) + usize::from(model.atoms[i].hydrogens))
        .sum(),
      f.edges.iter().filter(|&&i| model.edges[i].order == 2).count(),
    ]
  }

  /// Finds the farthest chain endpoint from every eligible leaf, retaining ties.
  fn longest_chain(&self, candidates: &[usize], model: &Model, work: &mut Work) -> Result<Vec<usize>, Error> {
    let mut best = Vec::new();
    for &start in candidates {
      let f = &self.fragments[start];
      if !f.chain {
        continue;
      }
      work.charge(1 + f.ports.len())?;
      let other = |edge: usize, id: usize| {
        let [a, b] = model.edges[edge].ends.map(|node| self.owner[node]);
        if a == id { b } else { a }
      };
      if f
        .ports
        .iter()
        .filter(|&&edge| self.fragments[other(edge, start)].chain)
        .count()
        > 1
      {
        continue;
      }
      let mut parents = BTreeMap::from([(start, start)]);
      let mut pending = VecDeque::from([start]);
      let mut last = start;
      while let Some(id) = pending.pop_front() {
        last = id;
        work.charge(1 + self.fragments[id].ports.len())?;
        for &edge in &self.fragments[id].ports {
          let next = other(edge, id);
          if self.fragments[next].chain && !parents.contains_key(&next) {
            parents.insert(next, id);
            pending.push_back(next);
          }
        }
      }
      let mut path = vec![last];
      while last != start {
        work.charge(1)?;
        last = parents[&last];
        path.push(last);
      }
      path.reverse();
      if path.len() > best.len() {
        best = path;
      }
    }
    Ok(best)
  }

  /// Establishes breadth-first ownership while leaving all input edge directions intact.
  fn orient(&mut self, root: usize, model: &Model, work: &mut Work) -> Result<(), Error> {
    let mut seen = std::collections::BTreeSet::from([root]);
    let mut pending = VecDeque::from([root]);
    while let Some(id) = pending.pop_front() {
      self.order.push(id);
      let ports = self.fragments[id].ports.clone();
      work.charge(1 + ports.len())?;
      for edge in ports {
        let [a, b] = model.edges[edge].ends;
        let (parent_atom, child_atom) = if self.owner[a] == id { (a, b) } else { (b, a) };
        let child = self.owner[child_atom];
        if self.fragments[id].parent.is_some_and(|parent| parent.fragment == child) {
          continue;
        }
        if !seen.insert(child) {
          return Err(Error::Invariant(model.atoms[child_atom].id));
        }
        self.fragments[id].children.push(child);
        self.fragments[child].parent = Some(Connection {
          fragment: id,
          edge,
          parent_atom,
          child_atom,
        });
        pending.push_back(child);
      }
    }
    Ok(())
  }
}
