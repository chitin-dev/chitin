//! Fixed component-level vectors and optional live differential probes.

use super::*;
use super::{fragments::Forest, interactions::Interaction, math::Vector, topology::Model};
use chitin_chem::io::smiles::read_smiles;
use std::collections::BTreeSet;
type TestResult = Result<(), Box<dyn std::error::Error>>;

const POSITIONS: [[f32; 8]; 4] = [
  [0., 0., 40., 10., 60., 0., 10., -25.],
  [0.; 8],
  [-30., 15., 20., 0., 70., 25., 80., -30.],
  [0., 30., 0., 0., 50., 0., 50., 10.],
];

#[test]
fn typed_interactions_should_match_recorded_energy_and_force_vectors() -> TestResult {
  for line in include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/interactions.tsv")).lines() {
    let values: Vec<f32> = line.split_whitespace().map(str::parse).collect::<Result<_, _>>()?;
    let (sample, kind) = (values[0] as usize, values[1] as usize);
    let points: Vec<_> = POSITIONS[sample]
      .as_chunks::<2>()
      .0
      .iter()
      .map(|p| Vector::new(p[0], p[1]))
      .collect();
    let term = match kind {
      0 => Interaction::Stretch {
        atoms: [0, 1],
        target: 50.,
        tolerance: 2.,
        k: 0.1,
      },
      1 => Interaction::Bend {
        atoms: [0, 1, 2],
        target: 120.,
        k: 1.,
        k2: 0.05,
      },
      2 => Interaction::Clash {
        atoms: [0, 1, 2],
        target_square: 900.,
        k: 1.,
        k2: 0.1,
      },
      _ => Interaction::Alkene {
        atoms: [0, 1, 2, 3],
        same_side: true,
      },
    };
    let mut forces = vec![Vector::default(); 4];
    let energy = term.score(&points, &mut forces);
    let actual: Vec<_> = std::iter::once(energy)
      .chain(forces.iter().flat_map(|v| [v.x, v.y]))
      .collect();
    for (actual, &expected) in actual.into_iter().zip(&values[2..]) {
      assert!(
        (actual - expected).abs() <= 0.0002 + expected.abs() * 0.000002,
        "sample {sample} kind {kind}: {actual} != {expected}"
      );
    }
  }
  Ok(())
}

/// Serializes source atom order, directed parent ports and chain classification.
fn signature(forest: &Forest, ordinals: &[usize]) -> String {
  forest
    .order
    .iter()
    .map(|&id| {
      let f = &forest.fragments[id];
      let atoms = f
        .atoms
        .iter()
        .map(|&i| ordinals[i].to_string())
        .collect::<Vec<_>>()
        .join(",");
      let parent = f.parent.map_or_else(
        || "-".to_owned(),
        |p| format!("{}>{}", ordinals[p.parent_atom], ordinals[p.child_atom]),
      );
      format!("{atoms}@{parent}:{}", usize::from(f.chain))
    })
    .collect::<Vec<_>>()
    .join(";")
}

/// Representative branch, constrained-connection, cycle and longest-chain inputs.
const CASES: [&str; 19] = [
  "C",
  "CC",
  "CCCC",
  "CC(C)CC",
  "CC(=O)NCC",
  "CC#CC",
  "CC=C(C)C",
  "C1CCCCC1CC",
  "C1CCC2CCCCC2C1",
  "C1CCC2(CC1)CCCCC2",
  "CCOc1ccccc1",
  "CCCCCCCCCCc1ccccc1",
  "NCC(O)CO",
  "C1CC2CCC1C2",
  "C12C3C4C1C5C2C3C45",
  "C1C2CC3CC1CC(C2)C3",
  "C1C2CC1C2",
  "C12C3C1C1C2C31",
  "C1CCC1C2CCC2",
];

/// Compares shape without treating translation, rotation or global reflection as error.
fn pair_error(actual: &[Point], expected: &[f64]) -> f64 {
  let mut error = 0_f64;
  for i in 0..actual.len() {
    for j in i + 1..actual.len() {
      let distance = (expected[2 * i] - expected[2 * j]).hypot(expected[2 * i + 1] - expected[2 * j + 1]);
      error = error.max((actual[i].distance(actual[j]) - distance).abs());
    }
  }
  error
}

/// Measures display agreement after removing translation only.
fn translated_error(points: &[Point], expected: &[f64]) -> f64 {
  let dx = points
    .iter()
    .enumerate()
    .map(|(i, p)| p.x - expected[2 * i])
    .sum::<f64>()
    / points.len() as f64;
  let dy = points
    .iter()
    .enumerate()
    .map(|(i, p)| p.y - expected[2 * i + 1])
    .sum::<f64>()
    / points.len() as f64;
  points
    .iter()
    .enumerate()
    .map(|(i, p)| (p.x - dx - expected[2 * i]).hypot(p.y - dy - expected[2 * i + 1]))
    .fold(0_f64, f64::max)
}

#[test]
fn final_orientation_should_match_recorded_assemblies_without_clash_relaxation() -> TestResult {
  let fixture = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/oriented.tsv"));
  let seeds = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/assembled.tsv"));
  let mut failures = Vec::new();
  let mut count = 0;
  for (line, seed) in fixture.lines().zip(seeds.lines()) {
    let (source, coordinates) = line.split_once('\t').ok_or("missing orientation")?;
    let (seed_source, input) = seed.split_once('\t').ok_or("missing assembly")?;
    assert_eq!(source, seed_source);
    let expected: Vec<f64> = coordinates
      .split_whitespace()
      .map(str::parse)
      .collect::<Result<_, _>>()?;
    let input: Vec<f32> = input.split_whitespace().map(str::parse).collect::<Result<_, _>>()?;
    let graph = read_smiles(source)?.into_graph();
    let resolved = crate::chemical::integer_bonds(&graph)?;
    let mut work = Work::new(4_000_000);
    let model = Model::canonical(&resolved, &mut work)?;
    let forest = Forest::new(&model, &mut work)?;
    let index: BTreeMap<_, _> = model.atoms.iter().enumerate().map(|(i, a)| (a.id, i)).collect();
    let mut points = vec![Vector::default(); model.atoms.len()];
    for (i, (id, _)) in graph.atoms().enumerate() {
      points[index[&id]] = Vector::new(input[2 * i] * math::BOND, input[2 * i + 1] * math::BOND);
    }
    rotation::apply(&model, &forest, &mut points, &mut work)?;
    let actual: Vec<_> = graph
      .atoms()
      .map(|(id, _)| {
        let p = points[index[&id]] / math::BOND;
        Point::new(f64::from(p.x), f64::from(p.y))
      })
      .collect();
    let error = translated_error(&actual, &expected);
    if error >= 0.001 {
      failures.push(format!("{source}: {error}"));
    }
    count += 1;
  }
  assert_eq!(count, 33);
  assert!(failures.is_empty(), "{}", failures.join("\n"));
  Ok(())
}

#[test]
fn fragment_partitions_should_match_recorded_source_order_and_parent_directions() -> TestResult {
  let fixture = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fragments.tsv"));
  let mut count = 0;
  for line in fixture.lines() {
    let mut fields = line.split('\t');
    let source = fields.next().ok_or("missing input")?;
    let canonical = fields.next().ok_or("missing ordering")? == "1";
    let expected = fields.next().ok_or("missing fragment signature")?;
    let supplied = read_smiles(source)?.into_graph();
    let graph = crate::chemical::integer_bonds(&supplied)?;
    let mut work = Work::new(4_000_000);
    let original = Model::new(&graph, &mut work)?;
    let model = if canonical {
      Model::canonical(&graph, &mut work)?
    } else {
      Model::new(&graph, &mut work)?
    };
    let forest = Forest::new(&model, &mut work)?;
    let indices: BTreeMap<_, _> = original.atoms.iter().enumerate().map(|(i, a)| (a.id, i)).collect();
    let ordinals: Vec<_> = model.atoms.iter().map(|a| indices[&a.id]).collect();
    assert_eq!(
      signature(&forest, &ordinals),
      expected,
      "{source} canonical={canonical}"
    );
    count += 1;
  }
  assert_eq!(count, 38);
  Ok(())
}

#[test]
fn native_ring_frames_should_match_recorded_pre_minimization_geometry() -> TestResult {
  let fixture = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/assembled.tsv"));
  let mut count = 0;
  for line in fixture.lines() {
    let (source, coordinates) = line.split_once('\t').ok_or("missing coordinates")?;
    let supplied = read_smiles(source)?.into_graph();
    let graph = crate::chemical::integer_bonds(&supplied)?;
    let expected: Vec<f64> = coordinates
      .split_whitespace()
      .map(str::parse)
      .collect::<Result<_, _>>()?;
    let mut work = Work::new(4_000_000);
    let model = Model::canonical(&graph, &mut work)?;
    let forest = Forest::new(&model, &mut work)?;
    let mut points = vec![Vector::default(); model.atoms.len()];
    let seeded = rings::seed(&model, &forest, &mut points, &mut work)?;
    assert!(
      model.cycles.iter().flatten().all(|&a| seeded.rebuilt[a]),
      "unported ring seed: {source}"
    );
    let points = assembly::assemble(&model, &forest, &points, &mut work)?;
    let indices: BTreeMap<_, _> = model.atoms.iter().enumerate().map(|(i, a)| (a.id, i)).collect();
    let actual: Vec<_> = graph
      .atoms()
      .map(|(id, _)| {
        let p = points[indices[&id]] / math::BOND;
        Point::new(f64::from(p.x), f64::from(p.y))
      })
      .collect();
    assert_eq!(expected.len(), 2 * actual.len());
    let error = pair_error(&actual, &expected);
    assert!(error < 0.001, "{source}: {error}");
    count += 1;
  }
  assert_eq!(count, 33);
  Ok(())
}

#[test]
fn completed_coordinate_cases_should_retain_sub_quantization_shape_agreement() -> TestResult {
  let fixture = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/coordinates.tsv"));
  let mut checked = 0;
  let mut pending = 0;
  for line in fixture.lines() {
    let (status, row) = line.split_once('\t').ok_or("missing status")?;
    if status == "pending" {
      pending += 1;
      continue;
    }
    assert_eq!(status, "matched");
    let (source, coordinates) = row.split_once('\t').ok_or("missing coordinates")?;
    let expected: Vec<f64> = coordinates
      .split_whitespace()
      .map(str::parse)
      .collect::<Result<_, _>>()?;
    let graph = read_smiles(source)?.into_graph();
    let layout = super::super::generate(&graph, &LayoutOptions::default())?;
    let actual: Vec<_> = graph.atoms().map(|(id, _)| layout.positions()[&id]).collect();
    assert_eq!(expected.len(), 2 * actual.len());
    let error = pair_error(&actual, &expected);
    assert!(error < 0.001, "{source}: {error}");
    checked += 1;
  }
  assert_eq!((checked, pending), (40, 1));
  Ok(())
}

#[test]
fn completed_coordinate_cases_should_match_display_orientation_without_alignment() -> TestResult {
  let fixture = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/coordinates.tsv"));
  let mut failures = Vec::new();
  for line in fixture.lines() {
    let (status, row) = line.split_once('\t').ok_or("missing status")?;
    if status == "pending" {
      continue;
    }
    let (source, coordinates) = row.split_once('\t').ok_or("missing coordinates")?;
    let expected: Vec<f64> = coordinates
      .split_whitespace()
      .map(str::parse)
      .collect::<Result<_, _>>()?;
    let graph = read_smiles(source)?.into_graph();
    let layout = super::super::generate(&graph, &LayoutOptions::default())?;
    let points: Vec<_> = graph.atoms().map(|(id, _)| layout.positions()[&id]).collect();
    // Compare actual coordinates: no translation, rotation or reflection fitting
    // may conceal a frame assignment or final orientation defect.
    let error = points
      .iter()
      .enumerate()
      .map(|(i, p)| (p.x - expected[2 * i]).hypot(p.y - expected[2 * i + 1]))
      .fold(0_f64, f64::max);
    if error >= 0.001 {
      failures.push(format!("{source}: {error}"));
    }
  }
  assert!(failures.is_empty(), "{}", failures.join("\n"));
  Ok(())
}

#[test]
fn coordinate_quantization_should_round_negative_ties_towards_positive_infinity() {
  assert_eq!(Vector::new(-1.125, 1.125).rounded(), Vector::new(-1.12, 1.13));
}

#[test]
fn four_neighbor_order_should_match_recorded_branch_ring_and_cross_preferences() -> TestResult {
  let fixture = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/priorities.tsv"));
  let mut count = 0;
  for line in fixture.lines() {
    let (source, expected) = line.split_once('\t').ok_or("missing priority signature")?;
    let graph = read_smiles(source)?.into_graph();
    assert_eq!(priority_signature(&graph)?, expected, "{source}");
    count += 1;
  }
  assert_eq!(count, 7);
  Ok(())
}

/// Serializes drawing priorities using original source ordinals, not dense layout IDs.
fn priority_signature(graph: &MolecularGraph) -> Result<String, Box<dyn std::error::Error>> {
  let resolved = crate::chemical::integer_bonds(graph)?;
  let mut work = Work::new(4_000_000);
  let original = Model::new(&resolved, &mut work)?;
  let model = Model::canonical(&resolved, &mut work)?;
  let rules = priority::Rules::new(&model, &mut work)?;
  let indices: BTreeMap<_, _> = original.atoms.iter().enumerate().map(|(i, a)| (a.id, i)).collect();
  let mut nodes: Vec<_> = (0..model.atoms.len())
    .filter(|&a| model.neighbors[a].len() == 4)
    .collect();
  nodes.sort_by_key(|&a| indices[&model.atoms[a].id]);
  let mut pieces = Vec::new();
  for a in nodes {
    let neighbors = rules
      .neighbors(&model, a, &mut work)?
      .iter()
      .map(|&n| indices[&model.atoms[n].id].to_string())
      .collect::<Vec<_>>()
      .join(",");
    pieces.push(format!("{}@{neighbors}", indices[&model.atoms[a].id]));
  }
  Ok(pieces.join(";"))
}

#[test]
fn fragment_moves_should_match_native_coordinate_and_penalty_vectors() -> TestResult {
  let fixture = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/moves.tsv"));
  let points = [
    Vector::new(-13.257, 4.124),
    Vector::default(),
    Vector::new(42.125, 15.375),
    Vector::new(60.004, -35.005),
  ];
  let mut count = 0;
  for line in fixture.lines() {
    let values: Vec<f32> = line.split_whitespace().map(str::parse).collect::<Result<_, _>>()?;
    let kind = match values[0] as usize {
      0 => moves::Kind::Flip,
      1 => moves::Kind::ParentLength,
      2 => moves::Kind::Rotate,
      3 => moves::Kind::ScaleAtoms {
        pivot: 1,
        atoms: vec![2, 3],
      },
      _ => moves::Kind::InvertBond {
        pivot: 1,
        bound: 2,
        atoms: vec![2, 3],
      },
    };
    let action = moves::Move {
      fragment: 0,
      kind: kind.clone(),
    };
    let state = values[1] as u16;
    let mut frame: BTreeMap<_, _> = points.into_iter().enumerate().collect();
    action.apply(state, &mut frame, &mut Work::new(100))?;
    assert_eq!(action.penalty(state), values[2]);
    for (i, p) in frame.values().enumerate() {
      assert!(
        (p.x - values[3 + 2 * i]).abs() < 0.00002,
        "{kind:?}/{state}, atom {i}: {} != {}",
        p.x,
        values[3 + 2 * i]
      );
      assert!(
        (p.y - values[4 + 2 * i]).abs() < 0.00002,
        "{kind:?}/{state}, atom {i}: {} != {}",
        p.y,
        values[4 + 2 * i]
      );
    }
    count += 1;
  }
  assert_eq!(count, 18);
  Ok(())
}

/// Serializes registered atoms in canonical model order, retaining multiplicity.
fn scope_signature(assembly: &assembly::Assembly) -> String {
  assembly
    .actions
    .iter()
    .filter_map(|action| {
      let atoms = match &action.kind {
        moves::Kind::ScaleAtoms { atoms, .. } | moves::Kind::InvertBond { atoms, .. } => atoms,
        _ => return None,
      };
      let mut atoms = atoms.clone();
      atoms.sort_unstable();
      Some(format!(
        "{}@{}",
        action.domain().tier,
        atoms.iter().map(usize::to_string).collect::<Vec<_>>().join(",")
      ))
    })
    .collect::<Vec<_>>()
    .join(";")
}

const SCOPE_CASES: &[&str] = &[
  "CCC",
  "CC(C)C",
  "CC(=O)NCC",
  "CC(C)(C)O",
  "C1CCCCC1CCO",
  "CC1CCCCCCCCC1",
  "OC1CCCCCCCCC1",
  "CC1CCCCCCCC1CC",
  "C1CCCCC1.CCO",
];

fn scoped_assembly(source: &str) -> Result<assembly::Assembly, Box<dyn std::error::Error>> {
  let graph = read_smiles(source)?.into_graph();
  let mut work = Work::new(4_000_000);
  let model = Model::canonical(&graph, &mut work)?;
  let forest = Forest::new(&model, &mut work)?;
  let mut seed = vec![Vector::default(); model.atoms.len()];
  rings::seed(&model, &forest, &mut seed, &mut work)?;
  Ok(assembly::prepare(&model, &forest, &seed, &mut work)?)
}

#[test]
fn scoped_actions_should_match_recorded_traversal_registration() -> TestResult {
  let fixture = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/scopes.tsv"));
  let mut count = 0;
  for line in fixture.lines() {
    let (source, expected) = line.split_once('\t').ok_or("missing action signature")?;
    assert_eq!(scope_signature(&scoped_assembly(source)?), expected, "{source}");
    count += 1;
  }
  assert_eq!(count, SCOPE_CASES.len());
  Ok(())
}

#[test]
fn repaired_cohort_scopes_should_fit_their_actual_movable_fragment_frames() -> TestResult {
  let fixture = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/native-coordinate-failures.tsv"
  ));
  let mut checked = 0;
  for row in fixture.lines().skip(1) {
    let mut fields = row.split('\t');
    let name = fields.next().ok_or("missing source ID")?;
    let source = fields.next().ok_or("missing source SMILES")?;
    let graph = read_smiles(source)?.into_graph();
    let resolved = crate::chemical::integer_bonds(&graph)?;
    let mut work = Work::new(LayoutOptions::default().work_budget);
    let model = Model::canonical(&resolved, &mut work)?;
    let forest = Forest::new(&model, &mut work)?;
    let mut points = vec![Vector::default(); model.atoms.len()];
    rings::seed(&model, &forest, &mut points, &mut work)?;
    let assembly = assembly::prepare(&model, &forest, &points, &mut work)?;
    for action in &assembly.actions {
      let (pivot, bound, atoms) = match &action.kind {
        moves::Kind::ScaleAtoms { pivot, atoms } => (*pivot, None, atoms),
        moves::Kind::InvertBond { pivot, bound, atoms } => (*pivot, Some(*bound), atoms),
        _ => continue,
      };
      let mut frame = assembly.frames[action.fragment].clone();
      if let Some(parent) = forest.fragments[action.fragment].parent {
        frame.remove(&parent.parent_atom);
      }
      assert!(frame.contains_key(&pivot), "{name}: pivot {pivot}");
      assert!(
        bound.is_none_or(|bound| frame.contains_key(&bound)),
        "{name}: inversion axis"
      );
      assert!(
        atoms.iter().all(|atom| frame.contains_key(atom)),
        "{name}: scope {atoms:?}"
      );
    }
    checked += 1;
  }
  assert_eq!(checked, 23);
  Ok(())
}

#[test]
fn invalid_scoped_action_should_reject_before_mutating_the_frame() {
  for kind in [
    moves::Kind::ScaleAtoms {
      pivot: 0,
      atoms: vec![1, 99],
    },
    moves::Kind::InvertBond {
      pivot: 0,
      bound: 1,
      atoms: vec![1, 99],
    },
  ] {
    let action = moves::Move { fragment: 0, kind };
    let mut frame = BTreeMap::from([(0, Vector::default()), (1, Vector::new(50., 0.))]);
    let before = frame.clone();
    assert!(action.apply(1, &mut frame, &mut Work::new(100)).is_err());
    assert_eq!(frame, before);
  }
}

#[test]
fn scoped_inversion_should_not_be_created_at_supplied_stereo_centers() -> TestResult {
  let source = "N[C@H]1CCCCCCCCO1";
  let graph = read_smiles(source)?.into_graph();
  let mut work = Work::new(100000);
  let model = Model::canonical(&graph, &mut work)?;
  let assembly = scoped_assembly(source)?;
  assert!(!assembly.actions.iter().any(|action| {
    matches!(&action.kind, moves::Kind::InvertBond { pivot, .. } if model.atoms[*pivot].stereochemical)
  }));
  Ok(())
}

#[test]
fn scoped_actions_should_leave_frames_untouched_when_work_is_exhausted() {
  let mut frame = BTreeMap::from([(0, Vector::default()), (1, Vector::new(50., 0.))]);
  let before = frame.clone();
  let action = moves::Move {
    fragment: 0,
    kind: moves::Kind::ScaleAtoms {
      pivot: 0,
      atoms: vec![1],
    },
  };
  assert!(action.apply(1, &mut frame, &mut Work::new(0)).is_err());
  assert_eq!(frame, before);
}

#[test]
fn scoped_registration_should_enable_intrafragment_clash_scores() -> TestResult {
  let graph = read_smiles("CCC")?.into_graph();
  let mut work = Work::new(10000);
  let model = Model::canonical(&graph, &mut work)?;
  let forest = Forest::new(&model, &mut work)?;
  let points = vec![Vector::new(0., 0.), Vector::new(25., 5.), Vector::new(50., 0.)];
  let term = Interaction::Clash {
    atoms: [0, 1, 2],
    target_square: 900.,
    k: 1.,
    k2: 0.1,
  };
  let ignored = refinement::discrete(&model, &forest, &points, &[term], &[false; 3], &mut work)?;
  let included = refinement::discrete(&model, &forest, &points, &[term], &[false, true, false], &mut work)?;
  assert!(included > ignored);
  Ok(())
}

#[test]
#[ignore = "requires CHITIN_COORDINATE_ORACLE; compares traversal-created action registration"]
fn live_scoped_actions_should_match_native_registration_order() -> TestResult {
  for source in SCOPE_CASES {
    let graph = read_smiles(source)?.into_graph();
    let mut work = Work::new(4_000_000);
    let model = Model::canonical(&graph, &mut work)?;
    let forest = Forest::new(&model, &mut work)?;
    let mut seed = vec![Vector::default(); model.atoms.len()];
    rings::seed(&model, &forest, &mut seed, &mut work)?;
    let assembly = assembly::prepare(&model, &forest, &seed, &mut work)?;
    let expected = native("--action-scopes", &model)?;
    println!("{source}\t{}", expected.trim());
    assert_eq!(scope_signature(&assembly), expected.trim(), "{source}");
  }
  Ok(())
}

#[test]
fn immutable_assembly_frames_should_rebuild_zero_states_without_cumulative_drift() -> TestResult {
  for source in CASES {
    let graph = read_smiles(source)?.into_graph();
    let graph = crate::chemical::integer_bonds(&graph)?;
    let mut work = Work::new(4_000_000);
    let model = Model::canonical(&graph, &mut work)?;
    let forest = Forest::new(&model, &mut work)?;
    let mut seed = vec![Vector::default(); model.atoms.len()];
    rings::seed(&model, &forest, &mut seed, &mut work)?;
    let assembly = assembly::prepare(&model, &forest, &seed, &mut work)?;
    let before = assembly.frames.clone();
    let actions = &assembly.actions;
    let states = vec![0; actions.len()];
    let one = assembly.rebuild(&model, &forest, actions, &states, &mut work)?;
    let two = assembly.rebuild(&model, &forest, actions, &states, &mut work)?;
    assert_eq!(one, two);
    assert_eq!(assembly.frames, before);
    for (actual, expected) in one.iter().zip(&assembly.points) {
      assert!(
        (*actual - *expected).length() < 0.011,
        "{source}: {actual:?} != {expected:?}"
      );
    }
  }
  Ok(())
}

#[test]
fn coupled_template_interactions_should_match_recorded_energy_and_force_fields() -> TestResult {
  let fixture = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/assembly-forces.tsv"
  ));
  for line in fixture.lines() {
    let (source, row) = line.split_once('\t').ok_or("missing force field")?;
    let expected: Vec<f32> = row.split_whitespace().map(str::parse).collect::<Result<_, _>>()?;
    let graph = read_smiles(source)?.into_graph();
    let mut work = Work::new(4_000_000);
    let model = Model::canonical(&graph, &mut work)?;
    let forest = Forest::new(&model, &mut work)?;
    let mut points = vec![Vector::default(); model.atoms.len()];
    let seeded = rings::seed(&model, &forest, &mut points, &mut work)?;
    let points = assembly::assemble(&model, &forest, &points, &mut work)?;
    let terms = constraints::build(&model, &points, &seeded.rigid, &mut work)?;
    let mut forces = vec![Vector::default(); points.len()];
    let energy: f32 = terms.iter().map(|&term| term.score(&points, &mut forces)).sum();
    assert!(
      (energy - expected[0]).abs() < 0.005,
      "{source}: {energy} != {}",
      expected[0]
    );
    let index: BTreeMap<_, _> = model.atoms.iter().enumerate().map(|(i, a)| (a.id, i)).collect();
    for (i, (id, _)) in graph.atoms().enumerate() {
      let force = forces[index[&id]];
      assert!((force.x - expected[1 + 2 * i]).abs() < 0.0001, "{source}, atom {i}, x");
      assert!((force.y - expected[2 + 2 * i]).abs() < 0.0001, "{source}, atom {i}, y");
    }
  }
  Ok(())
}

#[test]
fn unconstrained_even_macrocycles_should_build_without_compatibility_geometry() -> TestResult {
  for n in (10..=80).step_by(2) {
    let graph = read_smiles(&format!("C1{}1", "C".repeat(n - 1)))?.into_graph();
    let layout = super::generate(&graph, &LayoutOptions::default())?;
    assert_eq!(layout.positions().len(), n);
    for (_, bond) in graph.bonds() {
      let [a, b] = bond.atoms().map(|id| layout.positions()[&id]);
      assert!((a.distance(b) - 1.).abs() < 0.001, "size {n}: {}", a.distance(b));
    }
  }
  Ok(())
}

#[test]
fn native_entry_should_preserve_limits_and_construct_macrocycles_without_legacy_seeds() -> TestResult {
  let graph = read_smiles("CC")?.into_graph();
  let invalid = LayoutOptions {
    max_atoms: 1,
    ..Default::default()
  };
  assert!(matches!(
    super::super::generate(&graph, &invalid),
    Err(Error::AtomBudget { .. })
  ));
  let exhausted = LayoutOptions {
    work_budget: 1,
    ..Default::default()
  };
  assert!(super::generate(&graph, &exhausted).is_err());
  let odd = read_smiles("C1CCCCCCCCCC1")?.into_graph();
  assert!(super::generate(&odd, &LayoutOptions::default()).is_ok());
  let coupled = read_smiles("C1CCCCCCCC2CCCCCCCCC12")?.into_graph();
  assert!(super::generate(&coupled, &LayoutOptions::default()).is_ok());
  Ok(())
}

/// Exchanges one source graph with the standalone native test driver.
fn native(mode: &str, model: &Model) -> Result<String, Box<dyn std::error::Error>> {
  use std::{
    io::Write,
    process::{Command, Stdio},
  };
  let path = std::env::var_os("CHITIN_COORDINATE_ORACLE").ok_or("missing native oracle")?;
  let mut child = Command::new("timeout")
    .arg("10s")
    .arg(path)
    .arg(mode)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()?;
  let mut input = format!("{} {}\n", model.atoms.len(), model.edges.len());
  for atom in &model.atoms {
    input.push_str(&format!("{} {} {}\n", atom.number, atom.hydrogens, atom.charge));
  }
  for edge in &model.edges {
    input.push_str(&format!("{} {} {}\n", edge.ends[0], edge.ends[1], edge.order));
  }
  if mode.ends_with("-stereo") {
    input.push_str(&format!("{}\n", model.stereo.len()));
    for &(atoms, same) in &model.stereo {
      input.push_str(&format!(
        "{} {} {} {} {}\n",
        atoms[0],
        atoms[1],
        atoms[2],
        atoms[3],
        usize::from(same)
      ));
    }
  }
  child.stdin.take().ok_or("missing input")?.write_all(input.as_bytes())?;
  let output = child.wait_with_output()?;
  if !output.status.success() {
    return Err(
      format!(
        "native probe failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
      )
      .into(),
    );
  }
  Ok(String::from_utf8(output.stdout)?)
}

#[test]
#[ignore = "diagnostic stage trace for strict corpus failures"]
fn strict_native_failure_stage_should_be_reported() -> TestResult {
  let source = "CC(C)CCCCCCCCCCCCCCCCCCC(=O)OC[C@@H](COC(=O)CCCCCCCCCCCCCC(C)C)OC(=O)CCCCCCCCCCC(C)C";
  let graph = read_smiles(source)?.into_graph();
  let mut work = Work::new(16_000_000);
  let model = Model::canonical(&graph, &mut work)?;
  println!("model {}", work.remaining());
  let forest = Forest::new(&model, &mut work)?;
  let mut points = vec![Vector::default(); model.atoms.len()];
  let seeded = rings::seed(&model, &forest, &mut points, &mut work)?;
  println!("rings {}", work.remaining());
  let assembly = assembly::prepare(&model, &forest, &points, &mut work)?;
  println!("assembly {}", work.remaining());
  let mut points = refinement::run(
    &model,
    &forest,
    &assembly,
    &seeded.rigid,
    seeded.requires_minimization,
    &mut work,
  )?
  .points;
  println!("refined {}", work.remaining());
  rotation::apply(&model, &forest, &mut points, &mut work)?;
  println!("rotated {}", work.remaining());
  let layout = super::generate(&graph, &LayoutOptions::default())?;
  println!("complete {}", layout.positions().len());
  Ok(())
}

#[test]
#[ignore = "diagnostic stage trace for fixed recovery corpus"]
fn recovery_failure_stage_should_be_reported() -> TestResult {
  let id = std::env::var("CHITIN_LAYOUT_CASE").unwrap_or_else(|_| "CNP0097745.1".to_owned());
  let fixture = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/native-recovery.tsv"
  ));
  let source = fixture
    .lines()
    .find_map(|line| {
      line
        .split_once('\t')
        .filter(|(name, _)| *name == id)
        .map(|(_, source)| source)
    })
    .ok_or("missing recovery case")?;
  let graph = read_smiles(source)?.into_graph();
  let graph = crate::chemical::integer_bonds(&graph)?;
  let mut work = Work::new(16_000_000);
  let model = Model::canonical(&graph, &mut work)?;
  println!("model: {} atoms; remaining {}", model.atoms.len(), work.remaining());
  let forest = Forest::new(&model, &mut work)?;
  let mut points = vec![Vector::default(); model.atoms.len()];
  let seeded = rings::seed(&model, &forest, &mut points, &mut work)?;
  println!("rings remaining {}", work.remaining());
  let shortest = |points: &[Vector]| {
    let mut lengths: Vec<_> = model
      .edges
      .iter()
      .map(|edge| {
        (
          edge.id,
          (points[edge.ends[0]] - points[edge.ends[1]]).length(),
          edge.ends,
          edge.ends.map(|a| seeded.rigid[a]),
        )
      })
      .collect();
    lengths.sort_by(|a, b| a.1.total_cmp(&b.1));
    format!("{:?}", &lengths[..lengths.len().min(4)])
  };
  if std::env::var_os("CHITIN_COORDINATE_ORACLE").is_some() {
    let text = native("--coordinates-stereo", &model)?;
    let values = text
      .split_whitespace()
      .map(str::parse::<f32>)
      .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(values.len(), model.atoms.len() * 2);
    let reference: Vec<_> = values
      .as_chunks::<2>()
      .0
      .iter()
      .map(|xy| Vector::new(xy[0] * math::BOND, xy[1] * math::BOND))
      .collect();
    println!("reference shortest {}", shortest(&reference));
  }
  let assembly = assembly::prepare(&model, &forest, &points, &mut work)?;
  println!(
    "assembly remaining {}; shortest {}",
    work.remaining(),
    shortest(&assembly.points)
  );
  let refined = refinement::run(
    &model,
    &forest,
    &assembly,
    &seeded.rigid,
    seeded.requires_minimization,
    &mut work,
  )?;
  println!(
    "refinement remaining {}; shortest {}",
    work.remaining(),
    shortest(&refined.points)
  );
  let layout = super::generate(&graph, &LayoutOptions::default())?;
  println!("full layout {}", layout.positions().len());
  let drawing = crate::depict::depict(&graph, &layout, &crate::depict::DepictOptions::default())?;
  println!("depicted {}", drawing.primitives().len());
  Ok(())
}

fn ranking_signature(model: &Model, work: &mut Work) -> Result<String, Error> {
  let mut pieces = Vec::new();
  for center in 0..model.atoms.len() {
    let mut neighbors: Vec<_> = model.neighbors[center].iter().map(|&(atom, _)| atom).collect();
    neighbors.sort_unstable();
    for (i, &a) in neighbors.iter().enumerate() {
      for &b in &neighbors[i + 1..] {
        let winner =
          ranking::prefer(model, a, b, center, work)?.map_or_else(|| "-".to_owned(), |atom| atom.to_string());
        pieces.push(format!("{center}@{a},{b}>{winner}"));
      }
    }
  }
  Ok(pieces.join(";"))
}

#[test]
fn drawing_rankings_should_match_recorded_ancestry_and_ghost_vectors() -> TestResult {
  let fixture = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rankings.tsv"));
  let mut count = 0;
  for line in fixture.lines() {
    let (source, expected) = line.split_once('\t').ok_or("missing branch signature")?;
    let graph = read_smiles(source)?.into_graph();
    let mut work = Work::new(8_000_000);
    let model = Model::canonical(&graph, &mut work)?;
    assert_eq!(ranking_signature(&model, &mut work)?, expected, "{source}");
    count += 1;
  }
  assert_eq!(count, 9);
  Ok(())
}

#[test]
fn constrained_macrocycles_should_match_recorded_native_assembly_vectors() -> TestResult {
  let fixture = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/macrocycle-assembly.tsv"
  ));
  let mut count = 0;
  for line in fixture.lines() {
    let (source, coordinates) = line.split_once('\t').ok_or("missing macrocycle coordinates")?;
    let expected: Vec<f32> = coordinates
      .split_whitespace()
      .map(str::parse)
      .collect::<Result<_, _>>()?;
    let graph = read_smiles(source)?.into_graph();
    let mut work = Work::new(4_000_000);
    let model = Model::canonical(&graph, &mut work)?;
    let forest = Forest::new(&model, &mut work)?;
    let mut points = vec![Vector::default(); model.atoms.len()];
    let seeded = rings::seed(&model, &forest, &mut points, &mut work)?;
    assert!(model.cycles.iter().flatten().all(|&a| seeded.rebuilt[a]));
    let assembly = assembly::prepare(&model, &forest, &points, &mut work)?;
    assert_eq!(expected.len(), model.atoms.len() * 2);
    for (i, &point) in assembly.points.iter().enumerate() {
      let target = Vector::new(expected[2 * i], expected[2 * i + 1]);
      assert!((point / math::BOND - target).length() < 0.00001, "{source}: atom {i}");
    }
    count += 1;
  }
  assert_eq!(count, 10);
  Ok(())
}

#[test]
fn supplied_stereo_should_match_recorded_native_local_orientation() -> TestResult {
  let fixture = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/stereo-assembly.tsv"
  ));
  for line in fixture.lines() {
    let (source, coordinates) = line.split_once('\t').ok_or("missing stereo assembly")?;
    let expected: Vec<f32> = coordinates
      .split_whitespace()
      .map(str::parse)
      .collect::<Result<_, _>>()?;
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let mut work = Work::new(4_000_000);
    let model = Model::canonical(&graph, &mut work)?;
    let forest = Forest::new(&model, &mut work)?;
    let assembly = assembly::prepare(&model, &forest, &vec![Vector::default(); model.atoms.len()], &mut work)?;
    for (i, &point) in assembly.points.iter().enumerate() {
      let target = Vector::new(expected[2 * i], expected[2 * i + 1]);
      assert!((point / math::BOND - target).length() < 0.00001, "{source}: atom {i}");
    }
    let layout = super::super::generate(&graph, &LayoutOptions::default())?;
    for (_, descriptor) in graph.double_bond_stereos() {
      let [a, b] = descriptor.ends();
      let [left, right] = descriptor.references();
      let actual = crate::stereo_geometry::relation([a, b, left, right].map(|atom| layout.positions()[&atom]));
      assert_eq!(actual, Some(descriptor.relation()));
    }
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn native_stereo_should_reject_unresolved_cycles_without_running_legacy_closure() -> TestResult {
  let graph = read_smiles("C1/C=C/CCCCCCC1")?.into_graph();
  let mut work = Work::new(100000);
  let model = Model::canonical(&graph, &mut work)?;
  let &(atoms, _) = model.stereo.first().ok_or("missing cycle stereo")?;
  let mut points = vec![Vector::default(); model.atoms.len()];
  points[atoms[0]] = Vector::new(0., 50.);
  points[atoms[1]] = Vector::new(0., 0.);
  points[atoms[2]] = Vector::new(50., 0.);
  points[atoms[3]] = Vector::new(50., 50.);
  assert!(matches!(
    stereo::enforce(&model, &mut points, &mut work),
    Err(Error::DoubleBondStereo(_))
  ));
  Ok(())
}

#[test]
fn opened_models_should_restore_original_handles_without_mutating_chemistry() -> TestResult {
  let graph = read_smiles("C1CCCCCCCC2CCCCCCCCC12")?.into_graph();
  let before = graph.clone();
  let mut work = Work::new(16_000_000);
  let model = Model::canonical(&graph, &mut work)?;
  let edge = opening::choose_edge(&model, 0, &mut work)?.ok_or("no cut")?;
  let mut temporary = model.opened(edge, &mut work)?;
  assert_eq!(temporary.edges.len() + 1, model.edges.len());
  temporary.restore(&model.edges[edge], &model, &mut work)?;
  let actual: BTreeSet<_> = temporary.edges.iter().map(|edge| edge.id).collect();
  let expected: BTreeSet<_> = model.edges.iter().map(|edge| edge.id).collect();
  assert_eq!(actual, expected);
  let layout = super::super::generate(&graph, &LayoutOptions::default())?;
  assert_eq!(layout.positions().len(), graph.atom_count());
  assert_eq!(graph, before);
  Ok(())
}

#[test]
fn long_branched_lipids_should_not_fail_when_connectivity_scores_wrap() -> TestResult {
  let source = "CC(C)CCCCCCCCCCCCCCCCCCC(=O)OC[C@@H](COC(=O)CCCCCCCCCCCCCC(C)C)OC(=O)CCCCCCCCCCC(C)C";
  let graph = read_smiles(source)?.into_graph();
  let layout = super::super::generate(&graph, &LayoutOptions::default())?;
  assert_eq!(layout.positions().len(), graph.atom_count());
  Ok(())
}

#[test]
fn ring_opening_should_match_recorded_native_edge_selection() -> TestResult {
  let fixture = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/opening-edges.tsv"));
  for line in fixture.lines() {
    let (source, expected) = line.split_once('\t').ok_or("missing cut signature")?;
    let graph = read_smiles(source)?.into_graph();
    let mut work = Work::new(4_000_000);
    let model = Model::canonical(&graph, &mut work)?;
    let mut cuts = Vec::new();
    for ring in 0..model.cycles.len() {
      cuts.push(if let Some(edge) = opening::choose_edge(&model, ring, &mut work)? {
        let [a, b] = model.edges[edge].ends;
        format!("{},{}", a.min(b), a.max(b))
      } else {
        "-".to_owned()
      });
    }
    assert_eq!(cuts.join(";"), expected, "{source}");
  }
  Ok(())
}

#[test]
fn terminal_cleanup_should_shorten_crossing_strokes_in_source_order() -> TestResult {
  let graph = read_smiles("CCCC")?.into_graph();
  let mut work = Work::new(10000);
  let model = Model::new(&graph, &mut work)?;
  let mut points = vec![
    Vector::new(0., 0.),
    Vector::new(50., 0.),
    Vector::new(25., -25.),
    Vector::new(25., 25.),
  ];
  cleanup::terminals(&model, &mut points, &mut work)?;
  assert_eq!(points[0], Vector::new(45., 0.));
  Ok(())
}

#[test]
fn terminal_cleanup_should_not_modify_disconnected_components_or_partial_trials() -> TestResult {
  let graph = read_smiles("CC.CC")?.into_graph();
  let mut work = Work::new(10000);
  let model = Model::new(&graph, &mut work)?;
  let mut points = vec![
    Vector::new(0., 0.),
    Vector::new(50., 0.),
    Vector::new(25., -25.),
    Vector::new(25., 25.),
  ];
  let before = points.clone();
  cleanup::terminals(&model, &mut points, &mut work)?;
  assert_eq!(points, before);
  assert!(cleanup::terminals(&model, &mut points, &mut Work::new(1)).is_err());
  assert_eq!(points, before);
  Ok(())
}

#[test]
fn backbone_preferences_should_require_a_backbone_within_one_component() -> TestResult {
  let mut counts = Vec::new();
  for source in ["CC(=O)NCC(=O)NCC(=O)O", "CC(=O)NCC(=O)O.CC(=O)NCC(=O)O"] {
    let graph = read_smiles(source)?.into_graph();
    let mut work = Work::new(10000);
    let model = Model::canonical(&graph, &mut work)?;
    counts.push(backbone::torsions(&model, &mut work)?.len());
  }
  assert_eq!(counts, [4, 0]);
  Ok(())
}

#[test]
#[ignore = "requires CHITIN_COORDINATE_ORACLE; validates drawing-local branch ranking"]
fn live_substituent_rankings_should_match_native_ancestry_and_ghost_rules() -> TestResult {
  for source in [
    "CCC",
    "CC(C)C",
    "CC(C)(CO)CCO",
    "CC(C)(C#N)C(=O)O",
    "C1CCCCC1",
    "CC1CCCCC1O",
    "CC1=CCCCCCCCC1",
    "C1CC2CCC1C2",
    "CC(C)(CC(C)C)CC(C)(CC(C)C)C",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let mut work = Work::new(8_000_000);
    let model = Model::canonical(&graph, &mut work)?;
    let expected = native("--rankings", &model)?;
    println!("{source}\t{}", expected.trim());
    assert_eq!(ranking_signature(&model, &mut work)?, expected.trim(), "{source}");
  }
  Ok(())
}

#[test]
#[ignore = "requires CHITIN_COORDINATE_ORACLE; verifies native ring-cut ordering"]
fn live_ring_opening_edges_should_match_native_bfs_order_and_exclusions() -> TestResult {
  for source in [
    "C1CCCCCCCC1",
    "C1CCCCCCCC2CCCCCCCCC12",
    "C1=CC=CC=CC=CC=C1",
    "C1CC2CCC1C2",
    "C1CCC2CC3CCC(C1)C23",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let mut work = Work::new(4_000_000);
    let model = Model::canonical(&graph, &mut work)?;
    let mut cuts = Vec::new();
    for ring in 0..model.cycles.len() {
      cuts.push(if let Some(edge) = opening::choose_edge(&model, ring, &mut work)? {
        let [a, b] = model.edges[edge].ends;
        format!("{},{}", a.min(b), a.max(b))
      } else {
        "-".to_owned()
      });
    }
    let expected = native("--opening-edges", &model)?;
    println!("{source}\t{}", expected.trim());
    assert_eq!(cuts.join(";"), expected.trim(), "{source}");
  }
  Ok(())
}

#[test]
#[ignore = "requires CHITIN_COORDINATE_ORACLE; checks explicit fixed-width score overflow"]
fn live_long_lipid_partition_should_match_native_fixed_width_connectivity() -> TestResult {
  let source = "CC(C)CCCCCCCCCCCCCCCCCCC(=O)OC[C@@H](COC(=O)CCCCCCCCCCCCCC(C)C)OC(=O)CCCCCCCCCCC(C)C";
  let graph = read_smiles(source)?.into_graph();
  let mut work = Work::new(4_000_000);
  let original = Model::new(&graph, &mut work)?;
  let model = Model::canonical(&graph, &mut work)?;
  let forest = Forest::new(&model, &mut work)?;
  let ordinals: BTreeMap<_, _> = original
    .atoms
    .iter()
    .enumerate()
    .map(|(i, atom)| (atom.id, i))
    .collect();
  let indices: Vec<_> = model.atoms.iter().map(|atom| ordinals[&atom.id]).collect();
  assert_eq!(
    signature(&forest, &indices),
    native("--canonical-fragments", &original)?.trim()
  );
  Ok(())
}

#[test]
#[ignore = "requires CHITIN_COORDINATE_ORACLE; transmits supplied double-bond references"]
fn live_supplied_double_bond_assemblies_should_match_native_local_orientation() -> TestResult {
  for source in [
    "C/C=C/C",
    "C/C=C\\C",
    "F/C=C/F",
    "F/C=C\\F",
    "CC/C(C)=C(/O)CC",
    "CC/C(C)=C(\\O)CC",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let mut work = Work::new(4_000_000);
    let model = Model::canonical(&graph, &mut work)?;
    let forest = Forest::new(&model, &mut work)?;
    let assembly = assembly::prepare(&model, &forest, &vec![Vector::default(); model.atoms.len()], &mut work)?;
    let expected = native("--assembled-stereo", &model)?;
    let values: Vec<f32> = expected.split_whitespace().map(str::parse).collect::<Result<_, _>>()?;
    let error = assembly
      .points
      .iter()
      .enumerate()
      .map(|(i, p)| (*p / math::BOND - Vector::new(values[2 * i], values[2 * i + 1])).length())
      .fold(0_f32, f32::max);
    println!("{source}\t{error}\t{}", expected.trim());
    assert!(error < 0.001, "{source}: {error}");
  }
  Ok(())
}

#[test]
#[ignore = "requires CHITIN_COORDINATE_ORACLE; checks new native macrocycle assembly coverage"]
fn live_macrocycle_assembly_should_match_native_constraints_and_odd_closures() -> TestResult {
  let mut cases: Vec<_> = (9..=23).map(|n| format!("C1{}1", "C".repeat(n - 1))).collect();
  cases.extend(
    [
      "CC1CCCCCCCCC1",
      "OC1CCCCCCCCC1",
      "CC1CCCCCCCC1CC",
      "O1CCCCCCCCCCC1",
      "N1CCCCCCCCCCC1",
      "C1=CCCCCCCCCCC1",
      "O=C1CCCCCCCCCC1",
    ]
    .map(str::to_owned),
  );
  for source in cases {
    let graph = read_smiles(&source)?.into_graph();
    let mut work = Work::new(4_000_000);
    let model = Model::canonical(&graph, &mut work)?;
    let forest = Forest::new(&model, &mut work)?;
    let mut points = vec![Vector::default(); model.atoms.len()];
    let seeded = rings::seed(&model, &forest, &mut points, &mut work)?;
    assert!(
      model.cycles.iter().flatten().all(|&a| seeded.rebuilt[a]),
      "coverage: {source}"
    );
    let assembly = assembly::prepare(&model, &forest, &points, &mut work)?;
    let expected = native("--assembled", &model)?;
    let values: Vec<f32> = expected.split_whitespace().map(str::parse).collect::<Result<_, _>>()?;
    let error = assembly
      .points
      .iter()
      .enumerate()
      .map(|(i, p)| (*p / math::BOND - Vector::new(values[2 * i], values[2 * i + 1])).length())
      .fold(0_f32, f32::max);
    println!("macro-assembly\t{source}\t{error}\t{}", expected.trim());
    assert!(error < 0.001, "{source}: {error}");
  }
  Ok(())
}

#[test]
#[ignore = "requires CHITIN_COORDINATE_ORACLE; records final orientation without clash search"]
fn live_oriented_assemblies_should_be_recorded_independently_from_minimization() -> TestResult {
  for line in include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/assembled.tsv")).lines() {
    let (source, _) = line.split_once('\t').ok_or("missing coordinates")?;
    let graph = read_smiles(source)?.into_graph();
    let resolved = crate::chemical::integer_bonds(&graph)?;
    let model = Model::new(&resolved, &mut Work::new(4_000_000))?;
    println!(
      "orientation-reference\t{source}\t{}",
      native("--oriented-assembly", &model)?.trim()
    );
  }
  Ok(())
}

#[test]
#[ignore = "requires CHITIN_COORDINATE_ORACLE; checks native four-neighbor drawing priorities"]
fn live_four_neighbor_priorities_should_match_native_order() -> TestResult {
  for source in [
    "CC(C)(C)c1ccccc1",
    "CC(C)(F)CC",
    "OP(=O)(O)O",
    "CS(=O)(=O)C",
    "CC(C)(CC(C)C)CC(C)(CC(C)C)C",
    "C1CC2(C1)CCC2",
    "C1CC2(C3CCC2CC3)CC1",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let resolved = crate::chemical::integer_bonds(&graph)?;
    let mut work = Work::new(4_000_000);
    let original = Model::new(&resolved, &mut work)?;
    let reference = native("--priorities", &original)?;
    let model = Model::canonical(&resolved, &mut work)?;
    let rules = priority::Rules::new(&model, &mut work)?;
    let indices: BTreeMap<_, _> = original.atoms.iter().enumerate().map(|(i, a)| (a.id, i)).collect();
    let mut nodes: Vec<_> = (0..model.atoms.len())
      .filter(|&a| model.neighbors[a].len() == 4)
      .collect();
    nodes.sort_by_key(|&a| indices[&model.atoms[a].id]);
    let mut pieces = Vec::new();
    for a in nodes {
      let neighbors = rules
        .neighbors(&model, a, &mut work)?
        .iter()
        .map(|&n| indices[&model.atoms[n].id].to_string())
        .collect::<Vec<_>>()
        .join(",");
      pieces.push(format!("{}@{neighbors}", indices[&model.atoms[a].id]));
    }
    println!("priority-reference\t{source}\t{}", reference.trim());
    assert_eq!(pieces.join(";"), reference.trim(), "{source}");
  }
  Ok(())
}

#[test]
#[ignore = "requires CHITIN_COORDINATE_ORACLE; reports branched molecules rather than claiming parity"]
fn live_branched_coordinate_differences_should_cover_fragment_search_cases() -> TestResult {
  for source in [
    "CC(C)(C)c1cc(C(C)(C)C)cc(C(C)(C)C)c1",
    "CC(C)(CC(C)C)CC(C)(CC(C)C)CC(C)(CC(C)C)C",
    "c1ccccc1-c1ccccc1-c1ccccc1-c1ccccc1",
    "CC(C)c1ccc(C(C)C)c(C(C)C)c1",
    "C1CCCCC1C1CCCCC1C1CCCCC1",
    "c1ccc(CC(Cc2ccccc2)Cc2ccccc2)cc1",
    "CC(=O)NCC(=O)NCC(=O)O",
    "CCCCCCCCCCCCCCCCCCCC",
    "C12C3C4C1C5C2C3C45",
    "C1C2CC3CC1CC(C2)C3",
  ] {
    let graph = read_smiles(source)?.into_graph();
    let resolved = crate::chemical::integer_bonds(&graph)?;
    let model = Model::new(&resolved, &mut Work::new(4_000_000))?;
    let reference = native("--coordinates", &model)?;
    println!("branched-reference\t{source}\t{}", reference.trim());
    let expected: Vec<f64> = reference.split_whitespace().map(str::parse).collect::<Result<_, _>>()?;
    match super::super::generate(&graph, &LayoutOptions::default()) {
      Ok(layout) => {
        let actual: Vec<_> = graph.atoms().map(|(id, _)| layout.positions()[&id]).collect();
        println!(
          "branched-difference\t{source}\tpair {}\tpose {}",
          pair_error(&actual, &expected),
          translated_error(&actual, &expected)
        );
      }
      Err(error) => println!("branched-failure\t{source}\t{error}"),
    }
  }
  Ok(())
}

#[test]
fn discrete_fragment_search_should_unfold_a_colliding_chain_without_editing_source_chemistry() -> TestResult {
  let graph = read_smiles("CCCCCCCC")?.into_graph();
  let before = graph.clone();
  let mut work = Work::new(4_000_000);
  let model = Model::canonical(&graph, &mut work)?;
  let forest = Forest::new(&model, &mut work)?;
  let mut assembly = assembly::prepare(&model, &forest, &vec![Vector::default(); model.atoms.len()], &mut work)?;
  let actions = moves::defaults(&forest);
  let original = assembly.frames.clone();
  let flips: Vec<_> = actions
    .iter()
    .filter(|a| matches!(a.kind, moves::Kind::Flip))
    .map(|a| a.fragment)
    .collect();
  let rigid = vec![false; model.atoms.len()];
  let mut colliding = None;
  for mask in 1..(1usize << flips.len()) {
    assembly.frames.clone_from(&original);
    for (i, &fragment) in flips.iter().enumerate() {
      if mask & (1 << i) == 0 {
        continue;
      }
      for point in assembly.frames[fragment].values_mut() {
        point.y = -point.y;
      }
    }
    let candidate = assembly.rebuild(&model, &forest, &actions, &vec![0; actions.len()], &mut work)?;
    let terms = constraints::build(&model, &candidate, &rigid, &mut work)?;
    let score = refinement::discrete(&model, &forest, &candidate, &terms, &assembly.movable, &mut work)?;
    if score >= 10. {
      colliding = Some((candidate, score));
      break;
    }
  }
  let (points, bad_score) = colliding.ok_or("fixture did not produce a collision")?;
  assembly.points = points;
  let result = refinement::run(
    &model,
    &forest,
    &assembly,
    &vec![false; model.atoms.len()],
    false,
    &mut work,
  )?;
  let terms = constraints::build(&model, &result.points, &rigid, &mut work)?;
  let good_score = refinement::discrete(&model, &forest, &result.points, &terms, &assembly.movable, &mut work)?;
  assert!(good_score < bad_score, "{good_score} >= {bad_score}");
  assert_eq!(graph, before);
  Ok(())
}

#[test]
#[ignore = "requires locally compiled CHITIN_COORDINATE_ORACLE; prints stage coverage, not a passing parity claim"]
fn live_coordinate_differences_should_be_reported_without_hiding_failures() -> TestResult {
  let macrocycles: Vec<_> = (10..=40)
    .step_by(2)
    .map(|n| format!("C1{}1", "C".repeat(n - 1)))
    .collect();
  for source in CASES.into_iter().chain(macrocycles.iter().map(String::as_str)) {
    let graph = read_smiles(source)?.into_graph();
    let resolved = crate::chemical::integer_bonds(&graph)?;
    let model = Model::new(&resolved, &mut Work::new(4_000_000))?;
    let text = match native("--coordinates", &model) {
      Ok(text) => text,
      Err(error) => {
        println!("reference-failure\t{source}\t{error}");
        continue;
      }
    };
    println!(
      "assembled-reference\t{source}\t{}",
      native("--assembled", &model)?.trim()
    );
    let expected: Vec<f64> = text.split_whitespace().map(str::parse).collect::<Result<_, _>>()?;
    println!("coordinate-reference\t{source}\t{}", text.trim());
    let layout = match super::super::generate(&graph, &LayoutOptions::default()) {
      Ok(layout) => layout,
      Err(error) => {
        println!("coordinate-failure\t{source}\t{error}");
        continue;
      }
    };
    assert_eq!(expected.len(), 2 * model.atoms.len());
    let mut max_pair_error = 0_f64;
    for i in 0..model.atoms.len() {
      for j in i + 1..model.atoms.len() {
        let a = layout.positions()[&model.atoms[i].id];
        let b = layout.positions()[&model.atoms[j].id];
        let native = (expected[2 * i] - expected[2 * j]).hypot(expected[2 * i + 1] - expected[2 * j + 1]);
        max_pair_error = max_pair_error.max((a.distance(b) - native).abs());
      }
    }
    println!("coordinate-difference\t{source}\t{max_pair_error:.8}");
  }
  Ok(())
}

#[test]
#[ignore = "requires CHITIN_COORDINATE_ORACLE pointing to the locally compiled reference driver"]
fn live_fragment_partition_and_root_order_should_match_native_oracle() -> TestResult {
  use std::{
    io::Write,
    process::{Command, Stdio},
  };
  let path = std::env::var_os("CHITIN_COORDINATE_ORACLE").ok_or("missing native oracle")?;
  for (source, canonical) in CASES.into_iter().flat_map(|source| [(source, false), (source, true)]) {
    let original = read_smiles(source)?.into_graph();
    let graph = crate::chemical::integer_bonds(&original)?;
    let mut work = Work::new(4_000_000);
    let model = Model::new(&graph, &mut work)?;
    let canonical_model = if canonical {
      Model::canonical(&graph, &mut work)?
    } else {
      Model::new(&graph, &mut work)?
    };
    let forest = Forest::new(&canonical_model, &mut work)?;
    let lookup: BTreeMap<_, _> = model.atoms.iter().enumerate().map(|(i, atom)| (atom.id, i)).collect();
    let ordinals: Vec<_> = canonical_model.atoms.iter().map(|atom| lookup[&atom.id]).collect();
    let mut child = Command::new(&path)
      .arg(if canonical {
        "--canonical-fragments"
      } else {
        "--fragments"
      })
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn()?;
    let mut input = format!("{} {}\n", model.atoms.len(), model.edges.len());
    for atom in &model.atoms {
      input.push_str(&format!("{} {} {}\n", atom.number, atom.hydrogens, atom.charge));
    }
    for edge in &model.edges {
      input.push_str(&format!("{} {} {}\n", edge.ends[0], edge.ends[1], edge.order));
    }
    child
      .stdin
      .take()
      .ok_or("missing input pipe")?
      .write_all(input.as_bytes())?;
    let output = child.wait_with_output()?;
    assert!(
      output.status.success(),
      "{source}: {}",
      String::from_utf8_lossy(&output.stderr)
    );
    let expected = String::from_utf8(output.stdout)?;
    println!("{source}\t{}\t{}", usize::from(canonical), expected.trim());
    assert_eq!(
      signature(&forest, &ordinals),
      expected.trim(),
      "{source} canonical={canonical}"
    );
  }
  Ok(())
}

#[test]
fn fragment_forest_should_cover_salts_and_empty_inputs_without_connecting_them() -> TestResult {
  for source in ["", "[Na+].[Cl-]", "C.CC.O"] {
    let graph = read_smiles(source)?.into_graph();
    let mut work = Work::new(1_000_000);
    let model = Model::new(&graph, &mut work)?;
    let forest = Forest::new(&model, &mut work)?;
    assert_eq!(forest.owner.len(), graph.atom_count());
    assert_eq!(forest.roots.len(), graph.connected_components().len());
    assert_eq!(forest.order.len(), forest.fragments.len());
  }
  Ok(())
}

#[test]
fn solver_should_relax_stretch_and_never_move_fixed_atoms() -> TestResult {
  let mut points = vec![Vector::new(0., 0.), Vector::new(80., 10.)];
  let origin = points[0];
  let terms = [Interaction::Stretch {
    atoms: [0, 1],
    target: 50.,
    tolerance: 0.,
    k: 0.1,
  }];
  let initial = terms[0].score(&points, &mut [Vector::default(); 2]);
  let outcome = solver::minimize(&mut points, &[true, false], &terms, 1000, &mut Work::new(10000))?;
  let final_energy = terms[0].score(&points, &mut [Vector::default(); 2]);
  assert_eq!(points[0], origin);
  assert!(outcome.minimum_energy < initial);
  assert!(final_energy < initial);
  Ok(())
}

#[test]
fn interrupted_simulation_should_not_publish_a_partial_coordinate_update() {
  let mut points = vec![Vector::new(0., 0.), Vector::new(100., 0.)];
  let before = points.clone();
  let terms = [Interaction::Stretch {
    atoms: [0, 1],
    target: 50.,
    tolerance: 0.,
    k: 0.1,
  }];
  assert!(solver::minimize(&mut points, &[false, false], &terms, 1000, &mut Work::new(1)).is_err());
  assert_eq!(points, before);
}

#[test]
fn spatial_evaluation_should_preserve_exhaustive_energy_and_force_bits() -> TestResult {
  for sample in 0..20 {
    let points: Vec<_> = (0..40)
      .map(|i| {
        Vector::new(
          ((i * 73 + sample * 17) % 503) as f32 - 251.,
          ((i * 47 + sample * 29) % 401) as f32 - 200.,
        )
      })
      .collect();
    let mut terms = Vec::new();
    for a in 0..points.len() - 1 {
      terms.push(Interaction::Stretch {
        atoms: [a, a + 1],
        target: 50.,
        tolerance: 0.,
        k: 0.1,
      });
      for p in 0..points.len() {
        if p != a && p != a + 1 {
          let term = Interaction::Clash {
            atoms: [a, p, a + 1],
            target_square: 1600.,
            k: 1.,
            k2: 0.1,
          };
          terms.push(term);
          // Repeated terms remain repeated, even when they share incidence.
          if p % 7 == 0 {
            terms.push(term);
          }
        }
      }
    }
    let mut exhaustive = vec![Vector::default(); points.len()];
    let expected = terms
      .iter()
      .fold(123.45, |energy, &term| energy + term.score(&points, &mut exhaustive));
    let mut indexed = vec![Vector::default(); points.len()];
    let mut work = Work::new(1_000_000);
    let actual = evaluation::Evaluator::new(&terms, &mut work)?.score_from(123.45, &points, &mut indexed, &mut work)?;
    assert_eq!(expected.to_bits(), actual.to_bits(), "sample {sample}");
    for (a, b) in exhaustive.iter().zip(indexed) {
      assert_eq!([a.x.to_bits(), a.y.to_bits()], [b.x.to_bits(), b.y.to_bits()]);
    }
  }
  Ok(())
}

#[test]
fn spatial_evaluation_should_keep_boundary_degenerate_and_large_coordinate_contacts() -> TestResult {
  for origin in [0., -10_000., 1_000_000.] {
    for length in [0., 50.] {
      for distance in [29.999, 30., 30.001] {
        let points = [
          Vector::new(origin, origin),
          Vector::new(origin + length, origin),
          Vector::new(origin, origin + distance),
        ];
        let terms = [Interaction::Clash {
          atoms: [0, 2, 1],
          target_square: 900.,
          k: 1.,
          k2: 0.1,
        }];
        let mut expected = [Vector::default(); 3];
        let energy = terms[0].score(&points, &mut expected);
        let mut actual = [Vector::default(); 3];
        let mut work = Work::new(1000);
        let indexed = evaluation::Evaluator::new(&terms, &mut work)?.score(&points, &mut actual, &mut work)?;
        assert_eq!(energy.to_bits(), indexed.to_bits());
        assert_eq!(expected, actual);
      }
    }
  }
  Ok(())
}

#[test]
fn sparse_spatial_evaluation_should_reduce_repeated_contact_work() -> TestResult {
  let points: Vec<_> = (0..64).map(|i| Vector::new(i as f32 * 200., 0.)).collect();
  let mut terms = Vec::new();
  for a in (0..64).step_by(2) {
    for p in 0..64 {
      if p != a && p != a + 1 {
        terms.push(Interaction::Clash {
          atoms: [a, p, a + 1],
          target_square: 900.,
          k: 1.,
          k2: 0.1,
        });
      }
    }
  }
  let mut work = Work::new(1_000_000);
  let evaluator = evaluation::Evaluator::new(&terms, &mut work)?;
  let before = work.remaining();
  for _ in 0..10 {
    let mut forces = vec![Vector::default(); points.len()];
    assert_eq!(evaluator.score(&points, &mut forces, &mut work)?, 0.);
    assert!(forces.iter().all(|&force| force == Vector::default()));
  }
  assert!(before - work.remaining() < terms.len() * 10 / 2);
  Ok(())
}

#[test]
fn input_tetrahedral_facts_should_not_act_as_authored_wedge_priorities() -> TestResult {
  let plain = read_smiles("CC(C(F)Cl)(Br)I")?.into_graph();
  let chiral = read_smiles("CC([C@H](F)Cl)(Br)I")?.into_graph();
  assert_eq!(priority_signature(&plain)?, priority_signature(&chiral)?);
  Ok(())
}

#[test]
fn restored_topology_should_relax_once_after_deferred_opening_search() -> TestResult {
  let graph = read_smiles("CC")?.into_graph();
  let mut work = Work::new(1_000_000);
  let model = Model::canonical(&graph, &mut work)?;
  let forest = Forest::new(&model, &mut work)?;
  let mut assembly = assembly::prepare(&model, &forest, &[Vector::default(); 2], &mut work)?;
  assembly.points = vec![Vector::default(), Vector::new(100., 0.)];
  let closure = Interaction::Stretch {
    atoms: [0, 1],
    target: 50.,
    tolerance: 0.,
    k: 1.,
  };
  let deferred = refinement::run_with_policy(
    &model,
    &forest,
    &assembly,
    &[false; 2],
    refinement::Relaxation::Deferred { closures: &[closure] },
    &mut work,
  )?;
  assert_eq!(deferred.points, assembly.points);
  let complete = refinement::run(&model, &forest, &assembly, &[false; 2], true, &mut work)?;
  assert!((complete.points[0] - complete.points[1]).length() < 100.);
  let source = "C1CCCCCCCC2CCCCCCCCC12";
  let graph = read_smiles(source)?.into_graph();
  let model = Model::canonical(&graph, &mut work)?;
  let forest = Forest::new(&model, &mut work)?;
  let seeded = rings::seed_with_policy(
    &model,
    &forest,
    &mut vec![Vector::default(); model.atoms.len()],
    true,
    0,
    &mut work,
  )?;
  assert!(seeded.requires_minimization);
  Ok(())
}

#[test]
fn recovery_corpus_should_remain_valid_unescaped_source_smiles() -> TestResult {
  let fixture = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/native-recovery.tsv"
  ));
  let mut ids = BTreeSet::new();
  for line in fixture.lines() {
    let (id, smiles) = line.split_once('\t').ok_or("missing recovery source")?;
    assert!(ids.insert(id));
    read_smiles(smiles)?;
  }
  assert_eq!(ids.len(), 309);
  Ok(())
}

#[test]
fn deferred_opening_and_terminal_relaxation_should_recover_historical_failures() -> TestResult {
  let fixture = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/native-recovery.tsv"
  ));
  for id in ["CNP0169424.1", "CNP0097745.1"] {
    let source = fixture
      .lines()
      .find_map(|line| {
        line
          .split_once('\t')
          .filter(|(name, _)| *name == id)
          .map(|(_, smiles)| smiles)
      })
      .ok_or("missing recovered case")?;
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let layout = super::super::generate(&graph, &LayoutOptions::default())?;
    assert_eq!(layout.positions().len(), graph.atom_count());
    crate::depict::depict(&graph, &layout, &crate::depict::DepictOptions::default())?;
    assert_eq!(graph, before);
  }
  Ok(())
}

#[test]
fn fragment_assembly_should_preserve_namespace_and_complete_connectivity() -> TestResult {
  for source in ["CCCO", "CC(=O)NCC", "C1CCCCC1CCC", "C1CCC2CCCCC2C1"] {
    let graph = read_smiles(source)?.into_graph();
    let before = graph.clone();
    let options = LayoutOptions {
      refinement_passes: 0,
      ..Default::default()
    };
    let layout = super::super::generate(&graph, &options)?;
    assert_eq!(graph, before);
    assert_eq!(layout.positions().len(), graph.atom_count());
    for (_, bond) in graph.bonds() {
      let [a, b] = bond.atoms().map(|id| layout.positions()[&id]);
      // Two-decimal drawing-space assignments quantize nominal unit edges.
      assert!((a.distance(b) - 1.).abs() < 0.001, "{source}: {}", a.distance(b));
    }
  }
  Ok(())
}

#[test]
fn invalid_move_pivots_and_members_should_have_distinct_failure_codes() {
  use crate::layout::{NativeFailureKind as Failure, NativeStage as Stage};
  let mut frame = BTreeMap::from([(0, Vector::default()), (1, Vector::new(50., 0.))]);
  for (pivot, atoms, kind) in [
    (9, vec![1], Failure::MissingMovePivot),
    (0, vec![9], Failure::MissingMoveMember),
  ] {
    let before = frame.clone();
    let action = moves::Move {
      fragment: 0,
      kind: moves::Kind::ScaleAtoms { pivot, atoms },
    };
    assert_eq!(
      action.apply(1, &mut frame, &mut Work::new(100)),
      Err(Error::native(Stage::Move, kind))
    );
    assert_eq!(before, frame);
  }
}

#[test]
fn degenerate_reflection_should_not_be_reported_as_nonfinite_geometry() {
  use crate::layout::{NativeFailureKind as Failure, NativeStage as Stage};
  let mut frame = BTreeMap::from([(0, Vector::default()), (1, Vector::default())]);
  let action = moves::Move {
    fragment: 0,
    kind: moves::Kind::InvertBond {
      pivot: 0,
      bound: 1,
      atoms: vec![1],
    },
  };
  let before = frame.clone();
  assert_eq!(
    action.apply(1, &mut frame, &mut Work::new(100)),
    Err(Error::native(Stage::Move, Failure::DegenerateReflectionAxis))
  );
  assert_eq!(frame, before);
}

#[test]
fn coordinate_failure_cohort_should_preserve_valid_raw_sources_and_original_rows() -> TestResult {
  let mut rows = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/native-coordinate-failures.tsv"
  ))
  .lines();
  assert_eq!(rows.next(), Some("id\tsmiles\tsource_row"));
  let mut ids = BTreeSet::new();
  for row in rows {
    let fields: Vec<_> = row.split('\t').collect();
    let [id, source, original_row] = fields.as_slice() else {
      return Err("invalid failure cohort row".into());
    };
    assert!(ids.insert(*id));
    assert!(original_row.parse::<usize>()? > 0);
    read_smiles(source)?;
  }
  assert_eq!(ids.len(), 23);
  Ok(())
}

#[test]
fn simulation_shape_coordinate_and_energy_failures_should_be_distinguishable() {
  use crate::layout::{NativeFailureKind as Failure, NativeStage as Stage};
  let mut points = [Vector::default(), Vector::new(50., 0.)];
  assert!(
    matches!(solver::minimize(&mut points, &[], &[], 1, &mut Work::new(100)), Err(Error::Native(failure)) if failure.kind == Failure::InvalidSimulationDimensions)
  );
  let before = points;
  let terms = [Interaction::Stretch {
    atoms: [0, 1],
    target: 50.,
    tolerance: 0.,
    k: f32::NAN,
  }];
  assert!(
    matches!(solver::minimize(&mut points, &[false;2], &terms, 1, &mut Work::new(100)), Err(Error::Native(failure)) if failure.stage == Stage::Simulation && failure.kind == Failure::NonFiniteEnergy)
  );
  assert_eq!(points, before);
  points[0].x = f32::NAN;
  assert!(
    matches!(solver::minimize(&mut points, &[false;2], &[], 1, &mut Work::new(100)), Err(Error::Native(failure)) if failure.kind == Failure::NonFiniteCoordinates)
  );
}
