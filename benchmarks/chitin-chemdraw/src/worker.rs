//! Disposable per-molecule workers: stage timings exclude process startup and QA.

use crate::{
  dataset::Case,
  metrics,
  quality::{self, Features, Metrics},
  scene_quality,
};
use anyhow::{Context, Result, ensure};
use chitin_chem::io::smiles::read_smiles;
use chitin_chemdraw::{
  depict::{self, DepictOptions},
  layout::{self, LayoutOptions},
  svg::{self, SvgOptions},
};
use serde::{Deserialize, Serialize};
use std::{
  collections::BTreeMap,
  io::{self, Write},
  path::PathBuf,
  time::Instant,
};

/// Parent-owned options; artifact names never contain untrusted dataset IDs.
#[derive(Debug, Deserialize, Serialize)]
pub struct Request {
  pub case: Case,
  pub max_atoms: usize,
  pub svg_path: Option<PathBuf>,
  /// Must match the recorded pipeline when reproducing a historical drawing.
  #[serde(default)]
  pub label_feedback: bool,
  /// Validated imported coordinates bypass all automatic geometry repair.
  #[serde(default)]
  pub imported_positions: Option<Vec<[f64; 2]>>,
  /// Untrusted provider exchange must be checked against the source graph first.
  #[serde(default)]
  pub external_layout: Option<crate::external::ExternalLayout>,
  #[serde(default)]
  pub external_timings_ms: BTreeMap<String, f64>,
}

/// Durable per-case result, including failures and separate stage timings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Record {
  pub case: Case,
  pub status: String,
  pub stage: String,
  pub error: Option<String>,
  pub timings_ms: BTreeMap<String, f64>,
  pub wall_ms: f64,
  pub metrics: Option<Metrics>,
  pub svg_bytes: Option<usize>,
  pub atom_count: Option<usize>,
  pub bond_count: Option<usize>,
  pub features: Option<Features>,
  /// Classified drawing failure; absent in historical records or other failures.
  #[serde(default)]
  pub stereo_failure: Option<String>,
  /// Required coupled closure was used; not a claim of readability or correctness.
  #[serde(default)]
  pub constraint_closure: Option<bool>,
  /// Persisted normalized source-order coordinates permit exact provider-free replay.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub imported_positions: Option<Vec<[f64; 2]>>,
}

/// Line-framed worker protocol; a stage marker survives timeout or process death.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
  Checkpoint { record: Box<Record> },
  Finished { record: Box<Record> },
}

impl Record {
  /// Starts a case with no claimed timings or quality measurements.
  pub fn pending(case: Case) -> Self {
    Self {
      case,
      status: "error".into(),
      stage: "worker_start".into(),
      error: None,
      timings_ms: BTreeMap::new(),
      wall_ms: 0.,
      metrics: None,
      svg_bytes: None,
      atom_count: None,
      bond_count: None,
      features: None,
      stereo_failure: None,
      constraint_closure: None,
      imported_positions: None,
    }
  }

  /// Announces a stage before entering fallible or potentially expensive work.
  fn timed<T>(&mut self, stage: &str, run: impl FnOnce() -> Result<T>) -> Result<T> {
    self.stage = stage.into();
    // Preserve completed timings and parsed size even when this stage times out.
    emit(&Event::Checkpoint {
      record: Box::new(self.clone()),
    })?;
    let start = Instant::now();
    let result = run();
    self
      .timings_ms
      .insert(stage.into(), start.elapsed().as_secs_f64() * 1000.);
    result
  }
}

/// Flushes one complete protocol record, including the final error payload.
fn emit(event: &Event) -> Result<()> {
  let mut output = io::stdout().lock();
  serde_json::to_writer(&mut output, event)?;
  writeln!(output)?;
  output.flush()?;
  Ok(())
}

/// Runs the recorded planar/perspective selector and explicit label-feedback policy.
///
/// Each phase is measured independently. QA happens only after SVG export and
/// does not affect generated coordinates. Tetrahedral depiction parity is not
/// independently verified by this first benchmark; its source count is recorded.
pub fn run() -> Result<()> {
  let request: Request = serde_json::from_reader(io::stdin().lock())?;
  let mut record = Record::pending(request.case.clone());
  record.timings_ms.clone_from(&request.external_timings_ms);
  let result = execute(&request, &mut record);
  match result {
    Ok(()) => {
      record.status = "success".into();
    }
    Err(error) => {
      record.stereo_failure = stereo_failure(&error).map(String::from);
      record.error = Some(format!("{error:#}"));
    }
  }
  emit(&Event::Finished {
    record: Box::new(record),
  })
}

/// Reads typed failure reasons through direct, contextual and transparent errors.
fn stereo_failure(error: &anyhow::Error) -> Option<&'static str> {
  error.chain().find_map(|cause| {
    // Transparent wrappers delegate their source chain; inspect the wrapper too.
    let depiction = cause.downcast_ref::<depict::DepictError>().or_else(|| match cause
      .downcast_ref::<chitin_chemdraw::pipeline::RenderError>(
    ) {
      Some(chitin_chemdraw::pipeline::RenderError::Depict(error)) => Some(error),
      _ => None,
    });
    match depiction {
      Some(depict::DepictError::StereoAssignment { reason, .. }) => Some(reason.code()),
      _ => None,
    }
  })
}

/// Executes immutable chemistry, automatic geometry, depiction, export and QA.
fn execute(request: &Request, record: &mut Record) -> Result<()> {
  let graph = record.timed("parse", || {
    ensure!(!request.case.smiles.trim().is_empty(), "empty SMILES field");
    Ok(read_smiles(&request.case.smiles)?.into_graph())
  })?;
  record.atom_count = Some(graph.atom_count());
  record.bond_count = Some(graph.bond_count());
  record.features = Some(record.timed("characterize", || quality::characterize(&graph))?);
  ensure!(graph.atom_count() <= request.max_atoms, "source exceeds atom budget");
  let options = LayoutOptions {
    max_atoms: request.max_atoms,
    ..Default::default()
  };
  let (coordinates, drawing) = if request.external_layout.is_some() || request.imported_positions.is_some() {
    ensure!(
      !request.label_feedback,
      "external coordinates cannot request internal label feedback"
    );
    ensure!(
      !(request.external_layout.is_some() && request.imported_positions.is_some()),
      "ambiguous imported geometry"
    );
    let coordinates = record.timed("coordinate_import", || {
      match (&request.external_layout, &request.imported_positions) {
        (Some(external), None) => crate::external::validate(&graph, external),
        (None, Some(positions)) => crate::external::imported(&graph, positions),
        _ => anyhow::bail!("missing imported geometry"),
      }
    })?;
    record.imported_positions = Some(
      graph
        .atoms()
        .map(|(id, _)| {
          let point = coordinates.positions()[&id];
          [point.x, point.y]
        })
        .collect(),
    );
    let drawing = record.timed("depict", || {
      Ok(depict::depict(&graph, &coordinates, &DepictOptions::default())?)
    })?;
    (coordinates, drawing)
  } else if request.label_feedback {
    // Measure the same automatic orchestration as normal consumers. Finishing
    // a fixed seed alone would silently omit quality-triggered re-embedding.
    let result = record.timed("automatic_drawing", || {
      Ok(chitin_chemdraw::pipeline::generate(
        &graph,
        &options,
        &DepictOptions::default(),
      )?)
    })?;
    (result.layout, result.drawing)
  } else {
    let coordinates = record.timed("layout", || Ok(layout::generate(&graph, &options)?))?;
    let drawing = record.timed("depict", || {
      Ok(depict::depict(&graph, &coordinates, &DepictOptions::default())?)
    })?;
    (coordinates, drawing)
  };
  let document = record.timed("svg", || Ok(svg::export(&drawing, &SvgOptions::default())?))?;
  record.constraint_closure = Some(
    coordinates
      .diagnostics()
      .contains(&layout::LayoutDiagnostic::ConstraintClosure),
  );
  record.svg_bytes = Some(document.len());
  if let Some(path) = &request.svg_path {
    // Artifacts are outside export timings; create_new prevents accidental overwrite.
    record.timed("artifact", || {
      let mut file = std::fs::File::create_new(path).context("create SVG artifact")?;
      file.write_all(document.as_bytes())?;
      Ok(())
    })?;
  }
  record.metrics = Some(record.timed("quality", || {
    let mut result = metrics::measure(&graph, &coordinates)?;
    scene_quality::measure(&graph, &drawing, &mut result)?;
    if let Some(geometry) = result.macrocycle_geometry.as_mut().filter(|g| !g.samples.is_empty())
      && let Some(bounds) = drawing.bounds()
    {
      let length = DepictOptions::default().bond_length;
      let padding = SvgOptions::default().padding;
      geometry.scene = Some(crate::macrocycle_angles::SceneFrame {
        positions: graph
          .atoms()
          .map(|(id, _)| {
            let p = coordinates.positions()[&id];
            [p.x * length, -p.y * length]
          })
          .collect(),
        view_box: [
          bounds.min().x - padding,
          bounds.min().y - padding,
          bounds.width().max(1.) + 2. * padding,
          bounds.height().max(1.) + 2. * padding,
        ],
      });
    }
    Ok(result)
  })?);
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn closure_provenance_should_round_trip_without_breaking_historical_records() -> Result<()> {
    let mut record = Record::pending(Case {
      row: 1,
      id: "fixture".into(),
      smiles: "CC".into(),
    });
    record.constraint_closure = Some(true);
    let mut encoded = serde_json::to_value(&record)?;
    assert_eq!(
      serde_json::from_value::<Record>(encoded.clone())?.constraint_closure,
      Some(true)
    );
    encoded
      .as_object_mut()
      .context("record must be an object")?
      .remove("constraint_closure");
    assert_eq!(serde_json::from_value::<Record>(encoded)?.constraint_closure, None);
    Ok(())
  }

  #[test]
  fn transparent_pipeline_errors_should_retain_stereo_failure_codes() -> Result<()> {
    let graph = read_smiles("[C@](F)(Cl)(Br)I")?.into_graph();
    let positions = graph
      .atoms()
      .enumerate()
      .map(|(index, (id, _))| (id, chitin_chemdraw::geometry::Point::new(index as f64, 0.)))
      .collect();
    let coordinates = layout::Layout2d::from_positions(positions)?;
    let error = depict::depict(&graph, &coordinates, &DepictOptions::default())
      .err()
      .context("expected degenerate projection")?;
    assert!(matches!(
      error,
      depict::DepictError::StereoAssignment {
        reason: depict::StereoFailureReason::DegenerateProjection,
        ..
      }
    ));
    let wrapped = anyhow::Error::new(chitin_chemdraw::pipeline::RenderError::from(error));
    assert_eq!(stereo_failure(&wrapped), Some("degenerate_projection"));
    assert_eq!(
      stereo_failure(&wrapped.context("render case")),
      Some("degenerate_projection")
    );
    assert_eq!(stereo_failure(&anyhow::anyhow!("other failure")), None);
    Ok(())
  }
}
