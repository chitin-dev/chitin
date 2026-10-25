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
  /// Historical transport flag; both values now select the sole native backend.
  #[serde(default)]
  pub fragmented_layout: bool,
  /// Validated imported coordinates bypass all automatic geometry repair.
  #[serde(default)]
  pub imported_positions: Option<Vec<[f64; 2]>>,
  /// Untrusted provider exchange must be checked against the source graph first.
  #[serde(default)]
  pub external_layout: Option<crate::external::ExternalLayout>,
  #[serde(default)]
  pub external_timings_ms: BTreeMap<String, f64>,
}

/// Stable native substage and rejection identifiers retained in benchmark records.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LayoutFailure {
  /// Native substage at the original rejection site.
  pub stage: String,
  /// Stable typed rejection code, not a parsed error message.
  pub code: String,
}

/// Durable per-case result, including failures and separate stage timings.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Record {
  pub case: Case,
  pub status: String,
  pub stage: String,
  pub error: Option<String>,
  /// Typed native failure; absent for historical records and unrelated errors.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub layout_failure: Option<LayoutFailure>,
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
      layout_failure: None,
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
      record.layout_failure = layout_failure(&error);
      record.stereo_failure = stereo_failure(&error).map(String::from);
      record.error = Some(format!("{error:#}"));
    }
  }
  emit(&Event::Finished {
    record: Box::new(record),
  })
}

/// Extracts native provenance through contextual and transparent error wrappers.
fn layout_failure(error: &anyhow::Error) -> Option<LayoutFailure> {
  error.chain().find_map(|cause| {
    let failure = cause.downcast_ref::<layout::NativeLayoutFailure>().or_else(|| {
      let layout = cause.downcast_ref::<layout::LayoutError>().or_else(|| match cause
        .downcast_ref::<chitin_chemdraw::pipeline::RenderError>(
      ) {
        Some(chitin_chemdraw::pipeline::RenderError::Layout(error)) => Some(error),
        _ => None,
      });
      match layout {
        Some(layout::LayoutError::Native(failure)) => Some(failure),
        _ => None,
      }
    })?;
    Some(LayoutFailure {
      stage: failure.stage.code().into(),
      code: failure.kind.code().into(),
    })
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
      !request.fragmented_layout,
      "imported coordinates cannot request fragment assembly"
    );
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
    // The same native geometry generator is used with or without label repair.
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
  // Retain the journal field for historical readers; this backend has no
  // separate coverage-closure generator.
  record.constraint_closure = Some(false);
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
  fn fragment_engine_selection_should_round_trip_without_changing_historical_requests() -> Result<()> {
    let old = serde_json::json!({
      "case":{"row":1,"id":"fixture","smiles":"CC"},
      "max_atoms":4096,"svg_path":null
    });
    let mut request: Request = serde_json::from_value(old)?;
    assert!(!request.fragmented_layout);
    request.fragmented_layout = true;
    let replay: Request = serde_json::from_value(serde_json::to_value(request)?)?;
    assert!(replay.fragmented_layout);
    assert!(!replay.label_feedback);
    Ok(())
  }

  #[test]
  fn historical_fragment_flag_should_not_select_a_different_current_generator() -> Result<()> {
    let mut request: Request = serde_json::from_value(serde_json::json!({
      "case":{"row":1,"id":"fixture","smiles":"CCO"},
      "max_atoms":4096,"svg_path":null,"label_feedback":false
    }))?;
    let mut first = Record::pending(request.case.clone());
    execute(&request, &mut first)?;
    request.fragmented_layout = true;
    let mut second = Record::pending(request.case.clone());
    execute(&request, &mut second)?;
    assert_eq!(
      serde_json::to_value(first.metrics)?,
      serde_json::to_value(second.metrics)?
    );
    assert_eq!(first.constraint_closure, Some(false));
    assert_eq!(second.constraint_closure, Some(false));
    Ok(())
  }

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

  #[test]
  fn native_failure_codes_should_survive_context_and_transparent_wrappers() -> Result<()> {
    let failure = layout::NativeLayoutFailure {
      stage: layout::NativeStage::Move,
      kind: layout::NativeFailureKind::MissingMoveMember,
    };
    let expected = Some(LayoutFailure {
      stage: "move".into(),
      code: "missing_move_member".into(),
    });
    assert_eq!(layout_failure(&anyhow::Error::new(failure.clone())), expected);
    assert_eq!(
      layout_failure(&anyhow::Error::new(layout::LayoutError::from(failure.clone())).context("layout case")),
      expected
    );
    let wrapped = chitin_chemdraw::pipeline::RenderError::from(layout::LayoutError::from(failure));
    assert_eq!(
      layout_failure(&anyhow::Error::new(wrapped).context("render case")),
      expected
    );
    assert_eq!(
      layout_failure(&anyhow::anyhow!("native layout failure: move/missing_move_member")),
      None
    );
    let historical = serde_json::json!({"case":{"row":1,"id":"old","smiles":"C"},"status":"error","stage":"layout","error":"layout coordinate range exceeded","timings_ms":{},"wall_ms":0,"metrics":null,"svg_bytes":null,"atom_count":1,"bond_count":0,"features":null});
    let record: Record = serde_json::from_value(historical)?;
    assert_eq!(record.layout_failure, None);
    Ok(())
  }
}
