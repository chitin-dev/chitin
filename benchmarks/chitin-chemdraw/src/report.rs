//! Macro/micro aggregation, explicit missingness and complexity-stratified reports.

use crate::{
  quality::{self, Observation, State},
  worker::Record,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, time::Duration};

/// Bounded largest-value references; source records remain in the full JSONL.
#[derive(Default)]
struct Aggregate {
  values: Vec<f64>,
  not_applicable: usize,
  not_implemented: usize,
  eligible: usize,
  unavailable_objects: usize,
  thresholded: usize,
  violation_count: usize,
  affected_cases: usize,
  largest: Vec<(f64, usize, String, Option<usize>)>,
}

impl Aggregate {
  /// Never turns an unavailable check into a successful zero-valued measurement.
  fn add(&mut self, metric: &Observation, record: &Record) {
    match metric.state {
      State::NotApplicable => {
        self.not_applicable += 1;
        self.unavailable_objects += metric.eligible;
      }
      State::NotImplemented => {
        self.not_implemented += 1;
        self.unavailable_objects += metric.eligible;
      }
      State::Measured => {
        if let Some(value) = metric.value {
          self.values.push(value);
          self.eligible += metric.eligible;
          if let Some(count) = metric.violations {
            self.thresholded += 1;
            self.violation_count += count;
            self.affected_cases += usize::from(count > 0);
          }
          self
            .largest
            .push((value, record.case.row, record.case.id.clone(), metric.violations));
          self.largest.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
          self.largest.truncate(20);
        }
      }
    }
  }

  /// Distributions are molecule-weighted; micro rate pools eligible objects.
  fn json(&self) -> Value {
    json!({"distribution": distribution(&self.values), "measured_cases": self.values.len(),
      "not_applicable_cases": self.not_applicable, "not_implemented_cases": self.not_implemented,
      "unavailable_objects": self.unavailable_objects, "eligible_objects_measured": self.eligible,
      "thresholded_cases": self.thresholded,
      "violations": (self.thresholded > 0).then_some(self.violation_count),
      "affected_cases": (self.thresholded > 0).then_some(self.affected_cases),
      "affected_fraction_of_measured": ratio(self.affected_cases, self.thresholded),
      "micro_violation_rate": (self.thresholded > 0).then(|| ratio(self.violation_count, self.eligible)).flatten(),
      "largest_values": self.largest.iter().map(|(value, row, id, violations)| json!({"row": row, "id": id,
        "value": value, "violations": violations})).collect::<Vec<_>>()})
  }
}

/// One named cohort's true selected-case denominator and measured quality vectors.
#[derive(Default)]
struct Cohort {
  selected: usize,
  generated: usize,
  evaluated: usize,
  clear: usize,
  readability_assessed: usize,
  metrics: BTreeMap<String, Aggregate>,
}

impl Cohort {
  /// Counts failures in coverage, while unavailable metrics remain explicit.
  fn add(&mut self, record: &Record) {
    self.selected += 1;
    self.generated += usize::from(record.svg_bytes.is_some());
    if let Some(metrics) = &record.metrics {
      self.evaluated += 1;
      self.clear += usize::from(metrics.readability_clear == Some(true));
      self.readability_assessed += usize::from(metrics.readability_clear.is_some());
      for (name, metric) in &metrics.measurements {
        self.metrics.entry(name.clone()).or_default().add(metric, record);
      }
    }
  }

  /// Neither end-to-end screening yield nor conditional yield is chemical accuracy.
  fn json(&self) -> Value {
    json!({"selected_cases": self.selected, "generated_cases": self.generated, "evaluated_cases": self.evaluated,
      "evaluation_coverage": ratio(self.evaluated, self.selected), "readability_clear_cases": self.clear,
      "readability_assessed_cases": self.readability_assessed,
      "readability_flagged_cases": self.readability_assessed - self.clear,
      "readability_unknown_cases": self.evaluated - self.readability_assessed,
      "end_to_end_readability_yield": ratio(self.clear, self.selected),
      "conditional_readability_yield": ratio(self.clear, self.evaluated),
      "metrics": self.metrics.iter().map(|(key, metric)| (key, metric.json())).collect::<BTreeMap<_, _>>()})
  }
}

/// Run-level outcomes and independently named cohorts; cohorts overlap by design.
#[derive(Default)]
pub struct Summary {
  pub cases: usize,
  pub success: usize,
  pub timeouts: usize,
  outcomes: BTreeMap<String, usize>,
  stages: BTreeMap<String, Vec<f64>>,
  all: Cohort,
  cohorts: BTreeMap<String, Cohort>,
}

impl Summary {
  /// Retains all selected records, including failures before graph characterization.
  pub fn add(&mut self, record: &Record) {
    self.cases += 1;
    *self
      .outcomes
      .entry(format!("{}:{}", record.status, record.stage))
      .or_default() += 1;
    self.timeouts += usize::from(record.status == "timeout");
    self.all.add(record);
    if record.status == "success" {
      self.success += 1;
      for (name, value) in &record.timings_ms {
        self.stages.entry(name.clone()).or_default().push(*value);
      }
    }
    let mut groups = Vec::new();
    if let Some(features) = &record.features {
      groups.push(format!(
        "atoms:{}",
        match features.atoms {
          0..=24 => "0-24",
          25..=49 => "25-49",
          50..=99 => "50-99",
          _ => "100+",
        }
      ));
      groups.push(format!(
        "block_rank:{}",
        match features.maximum_block_rank {
          0 => "0",
          1 => "1",
          2..=3 => "2-3",
          _ => "4+",
        }
      ));
      groups.push(format!("tetrahedral:{}", features.tetrahedral_centers > 0));
      groups.push(format!("double_bond_stereo:{}", features.double_bond_descriptors > 0));
    } else {
      groups.push("input:uncharacterized".into());
    }
    groups.push(format!(
      "profile:{}",
      record.metrics.as_ref().map_or("unavailable", |m| m.profile.as_str())
    ));
    for group in groups {
      self.cohorts.entry(group).or_default().add(record);
    }
  }

  /// Emits the versioned measurement protocol and denominator-explicit distributions.
  pub fn json(&self, elapsed: Duration) -> Value {
    json!({"schema_version": 2, "quality_version": quality::VERSION, "cases": self.cases,
      "success": self.success, "timeouts": self.timeouts, "outcomes": self.outcomes,
      "quality": self.all.json(), "cohorts": self.cohorts.iter().map(|(name, cohort)| (name, cohort.json())).collect::<BTreeMap<_, _>>(),
      "successful_stage_timings_ms": self.stages.iter().map(|(key, values)| (key, distribution(values))).collect::<BTreeMap<_, _>>(),
      "elapsed_seconds_including_sampling_and_spawn": elapsed.as_secs_f64(),
      "certification": "readability screens only; full chemical/graphical correctness is not established"})
  }
}

/// Nearest-rank quantiles on per-molecule values; no values means null statistics.
fn distribution(values: &[f64]) -> Value {
  let mut values = values.to_vec();
  values.sort_by(f64::total_cmp);
  let percentile = |p: f64| {
    values
      .get(((values.len() as f64 * p).ceil() as usize).saturating_sub(1))
      .copied()
  };
  json!({"n": values.len(), "mean": (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64),
    "p50": percentile(0.5), "p95": percentile(0.95), "p99": percentile(0.99), "max": values.last()})
}

/// Ratios with no observations are unknown, not 0% or 100%.
fn ratio(numerator: usize, denominator: usize) -> Option<f64> {
  (denominator > 0).then(|| numerator as f64 / denominator as f64)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn empty_distributions_have_null_percentiles() {
    assert_eq!(distribution(&[])["p95"], Value::Null);
  }

  #[test]
  fn failures_remain_in_the_end_to_end_denominator() {
    let record = Record::pending(crate::dataset::Case {
      row: 1,
      id: "bad".into(),
      smiles: "C(".into(),
    });
    let mut summary = Summary::default();
    summary.add(&record);
    let report = summary.json(Duration::ZERO);
    assert_eq!(report["quality"]["selected_cases"], 1);
    assert_eq!(report["quality"]["evaluation_coverage"], 0.);
    assert_eq!(report["quality"]["conditional_readability_yield"], Value::Null);
  }
}
