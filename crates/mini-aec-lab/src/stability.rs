#![allow(
  clippy::cast_precision_loss,
  clippy::cast_possible_truncation,
  clippy::cast_sign_loss
)]

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use mini_aec_engine::{
  EngineErrorKind, EngineState, SourceDescriptor, ValidationEvent, ValidationEventKind,
};
use serde::{Deserialize, Serialize};

const REPORT_SCHEMA_VERSION: u16 = 2;
const CURRENT_EVENT_SCHEMA_VERSION: u16 = 2;
const SNAPSHOT_INTERVAL_MS: u128 = 1_000;
const MAX_EVENT_GAP_MS: u128 = 2_000;
const WINDOW_MS: u128 = 300_000;
const MIN_WINDOW_DURATION_MS: u128 = 295_000;
const MIN_WINDOW_OBSERVATIONS: usize = 250;
const MIN_GATE_DURATION_MS: u128 = 1_800_000;
const MIN_LONGEST_SEGMENT_MS: u128 = 600_000;
const MIN_COVERAGE: f64 = 0.95;
const MIN_NUMERICAL_PPM: f64 = 1.0;
const ALIGNMENT_TOLERANCE_MS: f64 = 5.0;
const QPC_TICKS_PER_SECOND: f64 = 10_000_000.0;

#[derive(Clone, Debug)]
pub struct StabilityReportConfig {
  pub events_path: PathBuf,
  pub output_path: Option<PathBuf>,
  pub operator_observations_path: Option<PathBuf>,
  pub software_revision: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OperatorObservations {
  pub client_continuously_consumed: bool,
  pub render_active_during_scored_interval: bool,
  pub no_stale_replay_or_unexplained_interruption: bool,
  #[serde(default)]
  pub explained_counters: Vec<String>,
  #[serde(default)]
  pub notes: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DriftDisposition {
  BoundedSynchronizerSufficient,
  ClockDriftCompensationRequired,
  Inconclusive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionalDisposition {
  Passed,
  Failed,
  Inconclusive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FollowUpGuidance {
  MonitorEarlyVersionDiagnostics,
  CompleteOperatorObservations,
  ResolveFunctionalFailure,
  RepeatThirtyMinuteEvidence,
  ProposeClockDriftCompensation,
}

#[derive(Clone, Debug, Serialize)]
pub struct StabilityReport {
  pub schema_version: u16,
  pub event_schema_version: u16,
  pub authoritative: bool,
  pub software_revision: Option<String>,
  pub source_events: String,
  pub run_id: u128,
  pub session_id: Option<u128>,
  pub aec_instance_id: Option<u128>,
  pub microphone: SourceDescriptor,
  pub render: SourceDescriptor,
  pub requested_duration_ms: u128,
  pub observed_duration_ms: u128,
  pub terminal_event: ValidationEventKind,
  pub data_quality: DataQualityReport,
  pub clock_analysis: ClockAnalysisReport,
  pub drift_disposition: DriftDisposition,
  pub drift_reasons: Vec<String>,
  pub functional_disposition: FunctionalDisposition,
  pub functional_reasons: Vec<String>,
  pub operator_observations: Option<OperatorObservations>,
  pub thirty_minute_accepted: bool,
  pub follow_up: FollowUpGuidance,
}

#[derive(Clone, Debug, Serialize)]
pub struct DataQualityReport {
  pub expected_periodic_events: u64,
  pub observed_periodic_events: u64,
  pub periodic_event_coverage: f64,
  pub usable_duration_ms: u128,
  pub required_usable_duration_ms: u128,
  pub longest_clean_segment_ms: u128,
  pub clean_segments: Vec<CleanSegmentSummary>,
  pub excluded_intervals: Vec<ExcludedInterval>,
  pub gate_ready: bool,
  pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CleanSegmentSummary {
  pub start_elapsed_ms: u128,
  pub end_elapsed_ms: u128,
  pub duration_ms: u128,
  pub observations: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExcludedInterval {
  pub elapsed_ms: u128,
  pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClockAnalysisReport {
  pub method: &'static str,
  pub signed_ppm_definition: &'static str,
  pub window_duration_ms: u128,
  pub windows: Vec<ClockWindowReport>,
  pub median_relative_ppm: Option<f64>,
  pub median_absolute_deviation_ppm: Option<f64>,
  pub median_uncertainty_ppm: Option<f64>,
  pub conservative_drift_ppm: Option<f64>,
  pub persistent_direction: Option<&'static str>,
  pub synchronization_delta_trend_ms_per_minute: Option<f64>,
  pub predicted_five_ms_crossing_seconds: Option<f64>,
  pub predicted_phase_ms_at_requested_duration: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClockWindowReport {
  pub start_elapsed_ms: u128,
  pub end_elapsed_ms: u128,
  pub duration_ms: u128,
  pub observations: usize,
  pub microphone_rate_hz: f64,
  pub render_rate_hz: f64,
  pub relative_ppm: f64,
  pub uncertainty_ppm: f64,
  pub microphone_residual_rms_frames: f64,
  pub render_residual_rms_frames: f64,
  pub delta_start_100ns: Option<i64>,
  pub delta_end_100ns: Option<i64>,
  pub stale_render_increase: u64,
  pub silent_reference_increase: u64,
}

#[derive(Clone)]
struct Observation {
  elapsed_ms: u128,
  microphone_position: u64,
  microphone_qpc: u64,
  render_position: u64,
  render_qpc: u64,
  aec_instance_id: Option<u128>,
  synchronization_epoch: u64,
  discontinuities: u64,
  timestamp_errors: u64,
  render_discontinuities: u64,
  render_timestamp_errors: u64,
  current_delta_100ns: Option<i64>,
  stale_render_frames: u64,
  silent_render_references: u64,
}

struct PreparedEvidence {
  events: Vec<ValidationEvent>,
  event_schema_version: u16,
  authoritative: bool,
  run_id: u128,
  microphone: SourceDescriptor,
  render: SourceDescriptor,
  requested_duration_ms: u128,
  observed_duration_ms: u128,
  terminal_event: ValidationEventKind,
}

struct Regression {
  slope: f64,
  slope_standard_error: f64,
  residual_rms: f64,
}

pub fn create_report(config: &StabilityReportConfig) -> Result<PathBuf> {
  let events_path = validated_existing_evidence_path(&config.events_path)?;
  let operator_observations = config
    .operator_observations_path
    .as_ref()
    .map(|path| read_operator_observations(path))
    .transpose()?;
  let evidence = read_evidence(&events_path)?;
  let report = analyze(
    &evidence,
    operator_observations,
    config.software_revision.clone(),
    &events_path,
  );
  let requested_output = config
    .output_path
    .clone()
    .unwrap_or_else(|| events_path.with_file_name("stability-report.json"));
  let output_path = validated_output_path(&requested_output)?;
  fs::write(&output_path, serde_json::to_vec_pretty(&report)?)
    .with_context(|| format!("failed to write {}", output_path.display()))?;
  Ok(output_path)
}

fn read_operator_observations(path: &Path) -> Result<OperatorObservations> {
  let path = validated_existing_evidence_path(path)?;
  let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
  serde_json::from_slice(&bytes).with_context(|| {
    format!(
      "failed to parse operator observations from {}",
      path.display()
    )
  })
}

fn read_evidence(path: &Path) -> Result<PreparedEvidence> {
  let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
  parse_evidence(BufReader::new(file))
}

#[allow(
  clippy::too_many_lines,
  reason = "event validation keeps the single-run schema invariants together"
)]
fn parse_evidence(input: impl BufRead) -> Result<PreparedEvidence> {
  let mut events = Vec::new();
  for (index, line) in input.lines().enumerate() {
    let line = line.with_context(|| format!("failed to read line {}", index + 1))?;
    if line.trim().is_empty() {
      continue;
    }
    let event = serde_json::from_str::<ValidationEvent>(&line)
      .with_context(|| format!("invalid validation event on line {}", index + 1))?;
    if !matches!(event.schema_version, 1 | CURRENT_EVENT_SCHEMA_VERSION) {
      bail!(
        "unsupported validation event schema {} on line {}",
        event.schema_version,
        index + 1
      );
    }
    events.push(event);
  }
  if events.len() < 2 {
    bail!("validation evidence must contain at least started and terminal events");
  }
  if events.first().map(|event| event.event) != Some(ValidationEventKind::Started) {
    bail!("validation evidence must begin with a started event");
  }
  let terminal_event = events
    .last()
    .map(|event| event.event)
    .expect("events exist");
  if !matches!(
    terminal_event,
    ValidationEventKind::Final | ValidationEventKind::Failed
  ) {
    bail!("validation evidence must end with a final or failed event");
  }
  if events[..events.len() - 1].iter().any(|event| {
    matches!(
      event.event,
      ValidationEventKind::Final | ValidationEventKind::Failed
    )
  }) {
    bail!("validation evidence contains an event after a terminal event");
  }

  let event_schema_version = events[0].schema_version;
  if events
    .iter()
    .any(|event| event.schema_version != event_schema_version)
  {
    bail!("validation evidence mixes event schema versions");
  }
  let authoritative = event_schema_version == CURRENT_EVENT_SCHEMA_VERSION;
  let first_unix_ms = events[0].unix_ms;
  if !authoritative {
    for event in &mut events {
      event.monotonic_elapsed_ms = event.unix_ms.saturating_sub(first_unix_ms);
    }
  }
  for pair in events.windows(2) {
    if pair[1].monotonic_elapsed_ms < pair[0].monotonic_elapsed_ms {
      bail!("validation evidence contains regressing monotonic elapsed time");
    }
  }

  let first = &events[0].snapshot;
  let run_id = first
    .run_id
    .context("started event does not contain a run identity")?;
  let microphone = first
    .source
    .clone()
    .context("started event does not contain a microphone descriptor")?;
  let render = first
    .render_source
    .clone()
    .context("started event does not contain a render descriptor")?;
  let session_id = first.session_id;
  for event in &events {
    if event.snapshot.run_id != Some(run_id) {
      bail!("validation evidence mixes run identities");
    }
    if event.snapshot.source.as_ref() != Some(&microphone)
      || event.snapshot.render_source.as_ref() != Some(&render)
    {
      bail!("validation evidence changes endpoint identity within one run");
    }
    if event.snapshot.session_id != session_id {
      bail!("validation evidence changes sink-session identity within one run");
    }
  }
  let requested_duration_ms = if authoritative {
    let requested = events[0].requested_duration_ms;
    if requested == 0
      || events
        .iter()
        .any(|event| event.requested_duration_ms != requested)
    {
      bail!("schema version 2 evidence requires one consistent nonzero requested duration");
    }
    requested
  } else {
    0
  };
  let observed_duration_ms = events.last().map_or(0, |event| event.monotonic_elapsed_ms);

  Ok(PreparedEvidence {
    events,
    event_schema_version,
    authoritative,
    run_id,
    microphone,
    render,
    requested_duration_ms,
    observed_duration_ms,
    terminal_event,
  })
}

fn analyze(
  evidence: &PreparedEvidence,
  operator_observations: Option<OperatorObservations>,
  software_revision: Option<String>,
  source_path: &Path,
) -> StabilityReport {
  let (segments, excluded_intervals) = clean_segments(&evidence.events);
  let windows = clock_windows(
    &segments,
    nominal_rate_hz(&evidence.microphone),
    nominal_rate_hz(&evidence.render),
  );
  let data_quality = data_quality(evidence, &segments, excluded_intervals);
  let clock_analysis = summarize_clock_analysis(&windows, evidence.requested_duration_ms);
  let (drift_disposition, drift_reasons) = classify_drift(evidence, &data_quality, &clock_analysis);
  let (functional_disposition, functional_reasons) =
    classify_functional(evidence, operator_observations.as_ref());
  let last = evidence
    .events
    .last()
    .expect("prepared evidence is nonempty");
  let thirty_minute_accepted = evidence.requested_duration_ms >= MIN_GATE_DURATION_MS
    && drift_disposition == DriftDisposition::BoundedSynchronizerSufficient
    && functional_disposition == FunctionalDisposition::Passed;
  let follow_up = match drift_disposition {
    DriftDisposition::ClockDriftCompensationRequired => {
      FollowUpGuidance::ProposeClockDriftCompensation
    }
    DriftDisposition::Inconclusive => FollowUpGuidance::RepeatThirtyMinuteEvidence,
    DriftDisposition::BoundedSynchronizerSufficient => match functional_disposition {
      FunctionalDisposition::Passed => FollowUpGuidance::MonitorEarlyVersionDiagnostics,
      FunctionalDisposition::Failed => FollowUpGuidance::ResolveFunctionalFailure,
      FunctionalDisposition::Inconclusive => FollowUpGuidance::CompleteOperatorObservations,
    },
  };

  StabilityReport {
    schema_version: REPORT_SCHEMA_VERSION,
    event_schema_version: evidence.event_schema_version,
    authoritative: evidence.authoritative,
    software_revision,
    source_events: source_path.display().to_string(),
    run_id: evidence.run_id,
    session_id: last.snapshot.session_id,
    aec_instance_id: last.snapshot.aec_instance_id,
    microphone: evidence.microphone.clone(),
    render: evidence.render.clone(),
    requested_duration_ms: evidence.requested_duration_ms,
    observed_duration_ms: evidence.observed_duration_ms,
    terminal_event: evidence.terminal_event,
    data_quality,
    clock_analysis,
    drift_disposition,
    drift_reasons,
    functional_disposition,
    functional_reasons,
    operator_observations,
    thirty_minute_accepted,
    follow_up,
  }
}

fn clean_segments(events: &[ValidationEvent]) -> (Vec<Vec<Observation>>, Vec<ExcludedInterval>) {
  let mut segments = Vec::new();
  let mut current = Vec::new();
  let mut excluded = Vec::new();
  let mut previous: Option<Observation> = None;

  for event in events {
    if !matches!(
      event.event,
      ValidationEventKind::Periodic | ValidationEventKind::Final
    ) {
      continue;
    }
    let Some(observation) = observation(event) else {
      finish_segment(&mut segments, &mut current);
      previous = None;
      excluded.push(ExcludedInterval {
        elapsed_ms: event.monotonic_elapsed_ms,
        reason: "missing microphone or active-render clock observation".to_owned(),
      });
      continue;
    };
    if let Some(prior) = previous.as_ref() {
      let reason = segment_boundary_reason(prior, &observation);
      if let Some(reason) = reason {
        finish_segment(&mut segments, &mut current);
        excluded.push(ExcludedInterval {
          elapsed_ms: observation.elapsed_ms,
          reason: reason.to_owned(),
        });
      }
    }
    current.push(observation.clone());
    previous = Some(observation);
  }
  finish_segment(&mut segments, &mut current);
  (segments, excluded)
}

fn finish_segment(segments: &mut Vec<Vec<Observation>>, current: &mut Vec<Observation>) {
  if current.len() >= 2 {
    segments.push(std::mem::take(current));
  } else {
    current.clear();
  }
}

fn observation(event: &ValidationEvent) -> Option<Observation> {
  let snapshot = &event.snapshot;
  Some(Observation {
    elapsed_ms: event.monotonic_elapsed_ms,
    microphone_position: snapshot.last_device_position?,
    microphone_qpc: snapshot.last_qpc_timestamp_100ns?,
    render_position: snapshot.last_render_device_position?,
    render_qpc: snapshot.last_render_qpc_timestamp_100ns?,
    aec_instance_id: snapshot.aec_instance_id,
    synchronization_epoch: snapshot.synchronization_epoch,
    discontinuities: snapshot.discontinuities,
    timestamp_errors: snapshot.timestamp_errors,
    render_discontinuities: snapshot.render_discontinuities,
    render_timestamp_errors: snapshot.render_timestamp_errors,
    current_delta_100ns: snapshot.current_delta_100ns,
    stale_render_frames: snapshot.stale_render_frames,
    silent_render_references: snapshot.silent_render_references,
  })
}

fn segment_boundary_reason(previous: &Observation, current: &Observation) -> Option<&'static str> {
  if current.elapsed_ms.saturating_sub(previous.elapsed_ms) > MAX_EVENT_GAP_MS {
    return Some("periodic event gap exceeds two seconds");
  }
  if current.aec_instance_id != previous.aec_instance_id {
    return Some("AEC instance identity changed");
  }
  if current.synchronization_epoch != previous.synchronization_epoch {
    return Some("synchronization epoch changed");
  }
  if current.discontinuities != previous.discontinuities
    || current.render_discontinuities != previous.render_discontinuities
  {
    return Some("input discontinuity counter changed");
  }
  if current.timestamp_errors != previous.timestamp_errors
    || current.render_timestamp_errors != previous.render_timestamp_errors
  {
    return Some("input timestamp-error counter changed");
  }
  if current.microphone_position <= previous.microphone_position
    || current.microphone_qpc <= previous.microphone_qpc
  {
    return Some("microphone position or QPC did not increase");
  }
  if current.render_position <= previous.render_position
    || current.render_qpc <= previous.render_qpc
  {
    return Some("render position or QPC did not increase");
  }
  None
}

fn data_quality(
  evidence: &PreparedEvidence,
  segments: &[Vec<Observation>],
  excluded_intervals: Vec<ExcludedInterval>,
) -> DataQualityReport {
  let expected_periodic_events = (evidence.requested_duration_ms / SNAPSHOT_INTERVAL_MS) as u64;
  let observed_periodic_events = evidence
    .events
    .iter()
    .filter(|event| event.event == ValidationEventKind::Periodic)
    .count() as u64;
  let periodic_event_coverage = if expected_periodic_events == 0 {
    0.0
  } else {
    (observed_periodic_events as f64 / expected_periodic_events as f64).min(1.0)
  };
  let clean_segments = segments
    .iter()
    .map(|segment| CleanSegmentSummary {
      start_elapsed_ms: segment
        .first()
        .expect("segment has observations")
        .elapsed_ms,
      end_elapsed_ms: segment.last().expect("segment has observations").elapsed_ms,
      duration_ms: segment_duration(segment),
      observations: segment.len(),
    })
    .collect::<Vec<_>>();
  let usable_duration_ms = clean_segments
    .iter()
    .map(|segment| segment.duration_ms)
    .sum();
  let longest_clean_segment_ms = clean_segments
    .iter()
    .map(|segment| segment.duration_ms)
    .max()
    .unwrap_or(0);
  let required_usable_duration_ms = evidence.requested_duration_ms.saturating_mul(5) / 6;
  let mut reasons = Vec::new();
  if !evidence.authoritative {
    reasons.push("schema version 1 lacks authoritative monotonic duration metadata".to_owned());
  }
  if evidence.requested_duration_ms < MIN_GATE_DURATION_MS {
    reasons.push("requested duration is shorter than the 30-minute gate".to_owned());
  }
  if evidence.observed_duration_ms < evidence.requested_duration_ms {
    reasons.push("run ended before its requested duration".to_owned());
  }
  if periodic_event_coverage < MIN_COVERAGE {
    reasons.push(format!(
      "periodic event coverage {periodic_event_coverage:.3} is below {MIN_COVERAGE:.2}"
    ));
  }
  if usable_duration_ms < required_usable_duration_ms {
    reasons.push(format!(
      "usable clock duration {usable_duration_ms} ms is below required {required_usable_duration_ms} ms"
    ));
  }
  if longest_clean_segment_ms < MIN_LONGEST_SEGMENT_MS {
    reasons.push("no clean clock segment reaches ten minutes".to_owned());
  }
  DataQualityReport {
    expected_periodic_events,
    observed_periodic_events,
    periodic_event_coverage,
    usable_duration_ms,
    required_usable_duration_ms,
    longest_clean_segment_ms,
    clean_segments,
    excluded_intervals,
    gate_ready: reasons.is_empty(),
    reasons,
  }
}

fn segment_duration(segment: &[Observation]) -> u128 {
  segment
    .last()
    .expect("segment has observations")
    .elapsed_ms
    .saturating_sub(
      segment
        .first()
        .expect("segment has observations")
        .elapsed_ms,
    )
}

fn nominal_rate_hz(source: &SourceDescriptor) -> Option<f64> {
  source
    .native_format
    .as_ref()
    .map(|format| f64::from(format.sample_rate_hz))
    .filter(|rate| *rate > 0.0)
}

fn clock_windows(
  segments: &[Vec<Observation>],
  microphone_nominal_rate_hz: Option<f64>,
  render_nominal_rate_hz: Option<f64>,
) -> Vec<ClockWindowReport> {
  let (Some(microphone_nominal_rate_hz), Some(render_nominal_rate_hz)) =
    (microphone_nominal_rate_hz, render_nominal_rate_hz)
  else {
    return Vec::new();
  };
  let mut reports = Vec::new();
  for segment in segments {
    let mut start_index = 0;
    while start_index < segment.len() {
      let window_end = segment[start_index].elapsed_ms.saturating_add(WINDOW_MS);
      let end_index = segment[start_index..]
        .iter()
        .position(|observation| observation.elapsed_ms >= window_end)
        .map(|offset| start_index + offset);
      let Some(end_index) = end_index else {
        break;
      };
      let observations = &segment[start_index..=end_index];
      let duration_ms = observations
        .last()
        .expect("window has observations")
        .elapsed_ms
        .saturating_sub(observations[0].elapsed_ms);
      if duration_ms >= MIN_WINDOW_DURATION_MS && observations.len() >= MIN_WINDOW_OBSERVATIONS {
        if let Some(report) = analyze_window(
          observations,
          microphone_nominal_rate_hz,
          render_nominal_rate_hz,
        ) {
          reports.push(report);
        }
      }
      start_index = end_index;
    }
  }
  reports
}

fn analyze_window(
  observations: &[Observation],
  microphone_nominal_rate_hz: f64,
  render_nominal_rate_hz: f64,
) -> Option<ClockWindowReport> {
  let microphone = regression(
    observations
      .iter()
      .map(|item| (item.microphone_qpc, item.microphone_position)),
  )?;
  let render = regression(
    observations
      .iter()
      .map(|item| (item.render_qpc, item.render_position)),
  )?;
  if microphone.slope <= 0.0 || render.slope <= 0.0 {
    return None;
  }
  let microphone_rate_ratio = microphone.slope / microphone_nominal_rate_hz;
  let render_rate_ratio = render.slope / render_nominal_rate_hz;
  let relative_ppm = (render_rate_ratio / microphone_rate_ratio - 1.0) * 1_000_000.0;
  let uncertainty_ppm = 1_000_000.0
    * (microphone.slope_standard_error / microphone.slope
      + render.slope_standard_error / render.slope);
  let first = observations.first().expect("window has observations");
  let last = observations.last().expect("window has observations");
  Some(ClockWindowReport {
    start_elapsed_ms: first.elapsed_ms,
    end_elapsed_ms: last.elapsed_ms,
    duration_ms: last.elapsed_ms.saturating_sub(first.elapsed_ms),
    observations: observations.len(),
    microphone_rate_hz: microphone.slope,
    render_rate_hz: render.slope,
    relative_ppm,
    uncertainty_ppm,
    microphone_residual_rms_frames: microphone.residual_rms,
    render_residual_rms_frames: render.residual_rms,
    delta_start_100ns: first.current_delta_100ns,
    delta_end_100ns: last.current_delta_100ns,
    stale_render_increase: last
      .stale_render_frames
      .saturating_sub(first.stale_render_frames),
    silent_reference_increase: last
      .silent_render_references
      .saturating_sub(first.silent_render_references),
  })
}

fn regression(points: impl Iterator<Item = (u64, u64)>) -> Option<Regression> {
  let points = points.collect::<Vec<_>>();
  if points.len() < 3 {
    return None;
  }
  let first_qpc = points[0].0;
  let first_position = points[0].1;
  let values = points
    .iter()
    .map(|(qpc, position)| {
      (
        qpc.saturating_sub(first_qpc) as f64 / QPC_TICKS_PER_SECOND,
        position.saturating_sub(first_position) as f64,
      )
    })
    .collect::<Vec<_>>();
  let count = values.len() as f64;
  let mean_x = values.iter().map(|(x, _)| x).sum::<f64>() / count;
  let mean_y = values.iter().map(|(_, y)| y).sum::<f64>() / count;
  let sxx = values
    .iter()
    .map(|(x, _)| (x - mean_x).powi(2))
    .sum::<f64>();
  if sxx <= f64::EPSILON {
    return None;
  }
  let slope = values
    .iter()
    .map(|(x, y)| (x - mean_x) * (y - mean_y))
    .sum::<f64>()
    / sxx;
  let intercept = mean_y - slope * mean_x;
  let residual_sum_squares = values
    .iter()
    .map(|(x, y)| (y - (intercept + slope * x)).powi(2))
    .sum::<f64>();
  let residual_rms = (residual_sum_squares / count).sqrt();
  let variance = residual_sum_squares / (count - 2.0);
  let slope_standard_error = (variance / sxx).sqrt();
  if !slope.is_finite() || !slope_standard_error.is_finite() || !residual_rms.is_finite() {
    return None;
  }
  Some(Regression {
    slope,
    slope_standard_error,
    residual_rms,
  })
}

fn summarize_clock_analysis(
  windows: &[ClockWindowReport],
  requested_duration_ms: u128,
) -> ClockAnalysisReport {
  let ppm = windows
    .iter()
    .map(|window| window.relative_ppm)
    .collect::<Vec<_>>();
  let uncertainties = windows
    .iter()
    .map(|window| window.uncertainty_ppm)
    .collect::<Vec<_>>();
  let median_relative_ppm = median(&ppm);
  let median_uncertainty_ppm = median(&uncertainties);
  let median_absolute_deviation_ppm = median_relative_ppm.and_then(|center| {
    median(
      &ppm
        .iter()
        .map(|value| (value - center).abs())
        .collect::<Vec<_>>(),
    )
  });
  let decision_uncertainty = median_uncertainty_ppm
    .unwrap_or(0.0)
    .max(median_absolute_deviation_ppm.unwrap_or(0.0))
    .max(MIN_NUMERICAL_PPM);
  let conservative_drift_ppm =
    median_relative_ppm.map(|value| (value.abs() - decision_uncertainty).max(0.0));
  let positive = windows
    .iter()
    .filter(|window| window.relative_ppm > window.uncertainty_ppm.max(MIN_NUMERICAL_PPM))
    .count();
  let negative = windows
    .iter()
    .filter(|window| window.relative_ppm < -window.uncertainty_ppm.max(MIN_NUMERICAL_PPM))
    .count();
  let required_directional = ((windows.len() as f64) * 0.8).ceil() as usize;
  let persistent_direction = if windows.len() >= 3 && positive >= required_directional {
    Some("render_faster")
  } else if windows.len() >= 3 && negative >= required_directional {
    Some("render_slower")
  } else {
    None
  };
  let synchronization_delta_trend_ms_per_minute = delta_trend(windows);
  let predicted_five_ms_crossing_seconds = conservative_drift_ppm
    .filter(|ppm| *ppm > 0.0)
    .map(|ppm| 5_000.0 / ppm);
  let predicted_phase_ms_at_requested_duration = conservative_drift_ppm
    .map(|ppm| ppm * (requested_duration_ms as f64 / 1_000.0) / 1_000_000.0 * 1_000.0);
  ClockAnalysisReport {
    method: "non-overlapping five-minute OLS device-position/QPC windows with median and MAD",
    signed_ppm_definition: "((render_rate / render_nominal_rate) / (microphone_rate / microphone_nominal_rate) - 1) * 1,000,000; positive means render faster",
    window_duration_ms: WINDOW_MS,
    windows: windows.to_vec(),
    median_relative_ppm,
    median_absolute_deviation_ppm,
    median_uncertainty_ppm,
    conservative_drift_ppm,
    persistent_direction,
    synchronization_delta_trend_ms_per_minute,
    predicted_five_ms_crossing_seconds,
    predicted_phase_ms_at_requested_duration,
  }
}

fn median(values: &[f64]) -> Option<f64> {
  if values.is_empty() || values.iter().any(|value| !value.is_finite()) {
    return None;
  }
  let mut values = values.to_vec();
  values.sort_by(f64::total_cmp);
  let middle = values.len() / 2;
  if values.len().is_multiple_of(2) {
    Some(f64::midpoint(values[middle - 1], values[middle]))
  } else {
    Some(values[middle])
  }
}

fn delta_trend(windows: &[ClockWindowReport]) -> Option<f64> {
  let points = windows
    .iter()
    .filter_map(|window| {
      Some((
        window.start_elapsed_ms as f64 / 1_000.0,
        window.delta_start_100ns? as f64,
      ))
    })
    .chain(windows.last().and_then(|window| {
      Some((
        window.end_elapsed_ms as f64 / 1_000.0,
        window.delta_end_100ns? as f64,
      ))
    }))
    .collect::<Vec<_>>();
  if points.len() < 3 {
    return None;
  }
  let count = points.len() as f64;
  let mean_x = points.iter().map(|(x, _)| x).sum::<f64>() / count;
  let mean_y = points.iter().map(|(_, y)| y).sum::<f64>() / count;
  let sxx = points
    .iter()
    .map(|(x, _)| (x - mean_x).powi(2))
    .sum::<f64>();
  if sxx <= f64::EPSILON {
    return None;
  }
  let slope_100ns_per_second = points
    .iter()
    .map(|(x, y)| (x - mean_x) * (y - mean_y))
    .sum::<f64>()
    / sxx;
  Some(slope_100ns_per_second * 0.0001 * 60.0)
}

fn classify_drift(
  evidence: &PreparedEvidence,
  quality: &DataQualityReport,
  analysis: &ClockAnalysisReport,
) -> (DriftDisposition, Vec<String>) {
  if !quality.gate_ready {
    return (DriftDisposition::Inconclusive, quality.reasons.clone());
  }
  if analysis.windows.len() < 3 {
    return (
      DriftDisposition::Inconclusive,
      vec!["fewer than three eligible five-minute clock windows".to_owned()],
    );
  }
  let median = analysis.median_relative_ppm.unwrap_or(0.0);
  let uncertainty = analysis
    .median_uncertainty_ppm
    .unwrap_or(0.0)
    .max(analysis.median_absolute_deviation_ppm.unwrap_or(0.0))
    .max(MIN_NUMERICAL_PPM);
  let material_but_inconsistent =
    median.abs() > uncertainty && analysis.persistent_direction.is_none();
  if material_but_inconsistent {
    return (
      DriftDisposition::Inconclusive,
      vec!["clock windows show material but inconsistent drift direction".to_owned()],
    );
  }
  let recurring_consequence = match analysis.persistent_direction {
    Some("render_faster") => {
      analysis
        .windows
        .iter()
        .filter(|window| window.silent_reference_increase > 0)
        .count()
        >= 3
    }
    Some("render_slower") => {
      analysis
        .windows
        .iter()
        .filter(|window| window.stale_render_increase > 0)
        .count()
        >= 3
    }
    _ => false,
  };
  let synchronization_failure = evidence.events.last().is_some_and(|event| {
    event
      .snapshot
      .last_error
      .as_ref()
      .is_some_and(|error| error.kind == EngineErrorKind::SynchronizationFailure)
  });
  if (recurring_consequence || synchronization_failure) && analysis.persistent_direction.is_none() {
    return (
      DriftDisposition::Inconclusive,
      vec!["synchronization consequences lack consistent directional clock evidence".to_owned()],
    );
  }
  if let (Some(direction), Some(trend)) = (
    analysis.persistent_direction,
    analysis.synchronization_delta_trend_ms_per_minute,
  ) {
    let conflicts = (direction == "render_faster" && trend > 0.01)
      || (direction == "render_slower" && trend < -0.01);
    if conflicts && trend.abs() * (evidence.requested_duration_ms as f64 / 60_000.0) >= 1.0 {
      return (
        DriftDisposition::Inconclusive,
        vec!["clock-rate direction conflicts with the synchronization-delta trend".to_owned()],
      );
    }
  }
  let crosses_tolerance = analysis
    .predicted_phase_ms_at_requested_duration
    .is_some_and(|phase| phase >= ALIGNMENT_TOLERANCE_MS);
  if analysis.persistent_direction.is_some()
    && (crosses_tolerance || recurring_consequence || synchronization_failure)
  {
    let mut reasons = Vec::new();
    if crosses_tolerance {
      reasons.push(
        "conservative persistent drift reaches the 5 ms pairing tolerance during the gate"
          .to_owned(),
      );
    }
    if recurring_consequence {
      reasons.push(
        "drift-correlated synchronization maintenance recurs in at least three windows".to_owned(),
      );
    }
    if synchronization_failure {
      reasons
        .push("directional clock evidence precedes terminal synchronization failure".to_owned());
    }
    return (DriftDisposition::ClockDriftCompensationRequired, reasons);
  }
  (
    DriftDisposition::BoundedSynchronizerSufficient,
    vec![
      "measured clock behavior remains inside the existing bounded synchronizer contract"
        .to_owned(),
    ],
  )
}

#[allow(
  clippy::too_many_lines,
  reason = "the functional gate audits one fixed set of terminal safety counters"
)]
fn classify_functional(
  evidence: &PreparedEvidence,
  operator: Option<&OperatorObservations>,
) -> (FunctionalDisposition, Vec<String>) {
  let final_event = evidence
    .events
    .last()
    .expect("prepared evidence is nonempty");
  let snapshot = &final_event.snapshot;
  let started_snapshot = &evidence
    .events
    .first()
    .expect("prepared evidence is nonempty")
    .snapshot;
  let explained = operator
    .map(|value| value.explained_counters.iter().cloned().collect())
    .unwrap_or_default();
  let mut failures = Vec::new();
  let mut inconclusive = Vec::new();
  if operator.is_none() {
    inconclusive
      .push("operator observations are required for end-to-end functional acceptance".to_owned());
  }
  if evidence.terminal_event == ValidationEventKind::Failed || snapshot.last_error.is_some() {
    failures.push("the engine reported a terminal failure".to_owned());
  }
  if evidence.authoritative && evidence.observed_duration_ms < evidence.requested_duration_ms {
    failures.push("the run ended before its requested duration".to_owned());
  }
  if evidence.terminal_event == ValidationEventKind::Final && snapshot.state != EngineState::Stopped
  {
    failures.push("the final event does not report a stopped engine".to_owned());
  }
  push_counter_failure(
    &mut failures,
    "microphone_queue_overflows",
    snapshot
      .queue_overflows
      .saturating_sub(started_snapshot.queue_overflows),
    &explained,
  );
  push_counter_failure(
    &mut failures,
    "microphone_discarded_frames",
    snapshot
      .discarded_frames
      .saturating_sub(started_snapshot.discarded_frames),
    &explained,
  );
  push_counter_failure(
    &mut failures,
    "render_queue_overflows",
    snapshot
      .render_queue_overflows
      .saturating_sub(started_snapshot.render_queue_overflows),
    &explained,
  );
  push_counter_failure(
    &mut failures,
    "render_discarded_frames",
    snapshot
      .render_discarded_frames
      .saturating_sub(started_snapshot.render_discarded_frames),
    &explained,
  );
  push_counter_failure(
    &mut failures,
    "input_discontinuities",
    snapshot
      .discontinuities
      .saturating_sub(started_snapshot.discontinuities)
      .saturating_add(
        snapshot
          .render_discontinuities
          .saturating_sub(started_snapshot.render_discontinuities),
      ),
    &explained,
  );
  push_counter_failure(
    &mut failures,
    "timestamp_errors",
    snapshot
      .timestamp_errors
      .saturating_sub(started_snapshot.timestamp_errors)
      .saturating_add(
        snapshot
          .render_timestamp_errors
          .saturating_sub(started_snapshot.render_timestamp_errors),
      ),
    &explained,
  );
  push_counter_failure(
    &mut failures,
    "alignment_resets",
    snapshot
      .alignment_resets
      .saturating_sub(started_snapshot.alignment_resets),
    &explained,
  );
  push_counter_failure(
    &mut failures,
    "aec_resets_or_rebuilds",
    snapshot
      .aec_resets
      .saturating_sub(started_snapshot.aec_resets)
      .saturating_add(
        snapshot
          .aec_rebuilds
          .saturating_sub(started_snapshot.aec_rebuilds),
      ),
    &explained,
  );
  if snapshot
    .aec_invalid_outputs
    .saturating_sub(started_snapshot.aec_invalid_outputs)
    > 0
  {
    failures.push("AEC produced invalid output".to_owned());
  }
  if snapshot
    .processing_deadline_misses
    .saturating_sub(started_snapshot.processing_deadline_misses)
    > 0
  {
    failures.push("AEC processing missed the 10 ms deadline".to_owned());
  }
  if snapshot
    .sink_failures
    .saturating_sub(started_snapshot.sink_failures)
    > 0
  {
    failures.push("the virtual microphone sink reported a failure".to_owned());
  }
  if snapshot
    .aec_processed_frames
    .saturating_sub(started_snapshot.aec_processed_frames)
    == 0
    || snapshot
      .sink_accepted_frames
      .saturating_sub(started_snapshot.sink_accepted_frames)
      == 0
  {
    failures.push("the run produced no accepted AEC output frames".to_owned());
  }
  if snapshot.queue_depth != 0 || snapshot.render_queue_depth != 0 {
    failures.push("a stopped run retained queued audio".to_owned());
  }
  if has_growing_queue(&evidence.events, false) || has_growing_queue(&evidence.events, true) {
    failures
      .push("periodic evidence shows a queue growing across consecutive observations".to_owned());
  }
  let driver_start = evidence
    .events
    .iter()
    .find_map(|event| event.snapshot.sink_diagnostics_start.as_ref())
    .or_else(|| {
      evidence
        .events
        .iter()
        .find_map(|event| event.snapshot.sink_diagnostics_latest.as_ref())
    });
  let driver_latest = evidence
    .events
    .iter()
    .rev()
    .find_map(|event| event.snapshot.sink_diagnostics_latest.as_ref());
  if let Some((start, latest)) = driver_start.zip(driver_latest) {
    let rejected = latest.rejected_writes.saturating_sub(start.rejected_writes);
    let overflows = latest.overflows.saturating_sub(start.overflows);
    let discarded = latest
      .discarded_frames
      .saturating_sub(start.discarded_frames);
    let underruns = latest.underruns.saturating_sub(start.underruns);
    if rejected > 0 {
      failures.push("the driver rejected one or more writes".to_owned());
    }
    if overflows > 0 || discarded > 0 {
      failures.push("the driver overflowed or discarded frames".to_owned());
    }
    push_counter_failure(&mut failures, "driver_underruns", underruns, &explained);
  } else {
    inconclusive.push("start/latest driver diagnostics are unavailable".to_owned());
  }
  if let Some(operator) = operator {
    if !operator.client_continuously_consumed {
      failures.push("operator did not confirm continuous ordinary-client consumption".to_owned());
    }
    if !operator.render_active_during_scored_interval {
      failures.push("operator did not confirm active render during the scored interval".to_owned());
    }
    if !operator.no_stale_replay_or_unexplained_interruption {
      failures.push(
        "operator observed or could not rule out stale replay or unexplained interruption"
          .to_owned(),
      );
    }
    if !operator.explained_counters.is_empty()
      && operator
        .notes
        .as_deref()
        .is_none_or(|notes| notes.trim().is_empty())
    {
      failures.push("explained counters require nonempty operator notes".to_owned());
    }
  }
  if !failures.is_empty() {
    return (FunctionalDisposition::Failed, failures);
  }
  if !inconclusive.is_empty() {
    return (FunctionalDisposition::Inconclusive, inconclusive);
  }
  (
    FunctionalDisposition::Passed,
    vec![
      "metadata and required operator observations satisfy the functional-stability gate"
        .to_owned(),
    ],
  )
}

fn has_growing_queue(events: &[ValidationEvent], render: bool) -> bool {
  let mut previous = None;
  let mut consecutive_increases = 0_u8;
  for depth in events
    .iter()
    .filter(|event| event.event == ValidationEventKind::Periodic)
    .map(|event| {
      if render {
        event.snapshot.render_queue_depth
      } else {
        event.snapshot.queue_depth
      }
    })
  {
    if previous.is_some_and(|previous| depth > previous) {
      consecutive_increases = consecutive_increases.saturating_add(1);
      if consecutive_increases >= 3 {
        return true;
      }
    } else {
      consecutive_increases = 0;
    }
    previous = Some(depth);
  }
  false
}

fn push_counter_failure(
  failures: &mut Vec<String>,
  name: &str,
  count: u64,
  explained: &BTreeSet<String>,
) {
  if count > 0 && !explained.contains(name) {
    failures.push(format!(
      "counter {name} increased by {count} without an operator explanation"
    ));
  }
}

fn validated_existing_evidence_path(requested: &Path) -> Result<PathBuf> {
  if requested
    .components()
    .any(|component| matches!(component, Component::ParentDir))
  {
    bail!("evidence path must not contain parent-directory traversal");
  }
  let absolute = repository_absolute(requested)?;
  let canonical = fs::canonicalize(&absolute)
    .with_context(|| format!("failed to resolve existing evidence {}", absolute.display()))?;
  if !is_within_evidence_roots(&canonical)? {
    bail!("evidence must remain below artifacts/ or driver/windows/out/");
  }
  Ok(canonical)
}

fn validated_output_path(requested: &Path) -> Result<PathBuf> {
  if requested
    .components()
    .any(|component| matches!(component, Component::ParentDir))
  {
    bail!("report path must not contain parent-directory traversal");
  }
  let absolute = repository_absolute(requested)?;
  let parent = absolute.parent().context("report path has no parent")?;
  fs::create_dir_all(parent)
    .with_context(|| format!("failed to create report directory {}", parent.display()))?;
  let canonical_parent = fs::canonicalize(parent)
    .with_context(|| format!("failed to resolve report directory {}", parent.display()))?;
  if !is_within_evidence_roots(&canonical_parent)? {
    bail!("report must remain below artifacts/ or driver/windows/out/");
  }
  Ok(absolute)
}

fn repository_absolute(path: &Path) -> Result<PathBuf> {
  let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .and_then(Path::parent)
    .context("failed to resolve the MiniAEC repository root")?;
  Ok(if path.is_absolute() {
    path.to_path_buf()
  } else {
    repository.join(path)
  })
}

fn is_within_evidence_roots(path: &Path) -> Result<bool> {
  let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .and_then(Path::parent)
    .context("failed to resolve the MiniAEC repository root")?;
  for root in [
    repository.join("artifacts"),
    repository.join("driver").join("windows").join("out"),
  ] {
    if root.exists() && path.starts_with(fs::canonicalize(root)?) {
      return Ok(true);
    }
  }
  Ok(false)
}

#[cfg(test)]
mod tests {
  use std::io::Cursor;

  use mini_aec_engine::{
    EngineError, EngineSnapshot, InputRole, ProcessingMode, SinkTransportSnapshot,
    SinkTransportState, SourceDescriptor, SourceFormat, ValidationEvent,
  };

  use super::*;

  fn descriptor(role: InputRole, id: &str, sample_rate_hz: u32) -> SourceDescriptor {
    SourceDescriptor {
      role,
      endpoint_id: id.to_owned(),
      friendly_name: id.to_owned(),
      active: true,
      native_format: Some(SourceFormat {
        sample_rate_hz,
        channels: 1,
        bits_per_sample: 16,
      }),
    }
  }

  fn synthetic_evidence(window_ppm: &[f64]) -> PreparedEvidence {
    synthetic_evidence_with_rates(window_ppm, 48_000, 48_000)
  }

  fn synthetic_evidence_with_rates(
    window_ppm: &[f64],
    microphone_rate_hz: u32,
    render_rate_hz: u32,
  ) -> PreparedEvidence {
    let requested_duration_ms = window_ppm.len() as u128 * WINDOW_MS;
    let duration_seconds = (requested_duration_ms / 1_000) as u64;
    let microphone = descriptor(
      InputRole::Microphone,
      "synthetic-microphone",
      microphone_rate_hz,
    );
    let render = descriptor(
      InputRole::RenderLoopback,
      "synthetic-render",
      render_rate_hz,
    );
    let mut events = Vec::new();
    let base = EngineSnapshot {
      state: EngineState::RunningAec,
      mode: Some(ProcessingMode::Aec),
      source: Some(microphone.clone()),
      render_source: Some(render.clone()),
      run_id: Some(7),
      session_id: Some(11),
      aec_instance_id: Some(13),
      synchronization_epoch: 1,
      ..EngineSnapshot::default()
    };
    events.push(ValidationEvent {
      schema_version: 2,
      unix_ms: 1_000,
      monotonic_elapsed_ms: 0,
      requested_duration_ms,
      event: ValidationEventKind::Started,
      snapshot: base.clone(),
    });
    let mut render_position = 0.0;
    for second in 1..=duration_seconds {
      let window = ((second - 1) / 300) as usize;
      let ppm = window_ppm[window.min(window_ppm.len() - 1)];
      render_position += f64::from(render_rate_hz) * (1.0 + ppm / 1_000_000.0);
      let mut snapshot = base.clone();
      snapshot.last_device_position = Some(second * u64::from(microphone_rate_hz));
      snapshot.last_qpc_timestamp_100ns = Some(second * 10_000_000);
      snapshot.last_render_device_position = Some(render_position.round() as u64);
      snapshot.last_render_qpc_timestamp_100ns = Some(second * 10_000_000);
      snapshot.current_delta_100ns = Some(0);
      snapshot.paired_frames = second * 100;
      snapshot.aec_processed_frames = second * 100;
      snapshot.sink_accepted_frames = second * 100;
      let final_event = second == duration_seconds;
      if final_event {
        snapshot.state = EngineState::Stopped;
      }
      events.push(ValidationEvent {
        schema_version: 2,
        unix_ms: 1_000 + u128::from(second) * 1_000,
        monotonic_elapsed_ms: u128::from(second) * 1_000,
        requested_duration_ms,
        event: if final_event {
          ValidationEventKind::Final
        } else {
          ValidationEventKind::Periodic
        },
        snapshot,
      });
    }
    PreparedEvidence {
      event_schema_version: 2,
      authoritative: true,
      run_id: 7,
      microphone,
      render,
      requested_duration_ms,
      observed_duration_ms: requested_duration_ms,
      terminal_event: ValidationEventKind::Final,
      events,
    }
  }

  fn confirmed_operator() -> OperatorObservations {
    OperatorObservations {
      client_continuously_consumed: true,
      render_active_during_scored_interval: true,
      no_stale_replay_or_unexplained_interruption: true,
      explained_counters: Vec::new(),
      notes: None,
    }
  }

  fn transport_snapshot(accepted_frames: u64) -> SinkTransportSnapshot {
    SinkTransportSnapshot {
      schema_version: 2,
      state: SinkTransportState::Closed,
      active_session_id: None,
      last_accepted_sequence: accepted_frames.checked_sub(1),
      current_depth: 0,
      high_water_mark: 1,
      session_opens: 1,
      session_closes: 1,
      session_resets: 0,
      accepted_frames,
      rejected_writes: 0,
      underruns: 0,
      overflows: 0,
      discarded_frames: 0,
      driver_restarts: 0,
    }
  }

  fn add_sink_diagnostics(evidence: &mut PreparedEvidence) {
    let final_snapshot = &mut evidence
      .events
      .last_mut()
      .expect("synthetic final event")
      .snapshot;
    final_snapshot.sink_diagnostics_start = Some(transport_snapshot(0));
    final_snapshot.sink_diagnostics_latest =
      Some(transport_snapshot(final_snapshot.sink_accepted_frames));
  }

  fn report(
    evidence: &PreparedEvidence,
    operator: Option<OperatorObservations>,
  ) -> StabilityReport {
    analyze(
      evidence,
      operator,
      Some("synthetic".to_owned()),
      Path::new("synthetic/engine.jsonl"),
    )
  }

  fn serialized_evidence(evidence: &PreparedEvidence) -> Vec<u8> {
    let mut bytes = Vec::new();
    for event in &evidence.events {
      serde_json::to_writer(&mut bytes, event).expect("synthetic event serializes");
      bytes.push(b'\n');
    }
    bytes
  }

  #[test]
  fn stable_clocks_pass_drift_and_functional_gates() {
    let mut evidence = synthetic_evidence(&[0.0; 6]);
    add_sink_diagnostics(&mut evidence);
    let result = report(&evidence, Some(confirmed_operator()));
    assert_eq!(
      result.drift_disposition,
      DriftDisposition::BoundedSynchronizerSufficient
    );
    assert_eq!(result.functional_disposition, FunctionalDisposition::Passed);
    assert!(result.thirty_minute_accepted);
    assert_eq!(
      result.follow_up,
      FollowUpGuidance::MonitorEarlyVersionDiagnostics
    );
  }

  #[test]
  fn known_positive_and_negative_drift_are_signed_and_require_compensation() {
    for (ppm, direction) in [(10.0, "render_faster"), (-10.0, "render_slower")] {
      let mut evidence = synthetic_evidence(&[ppm; 6]);
      add_sink_diagnostics(&mut evidence);
      let result = report(&evidence, Some(confirmed_operator()));
      assert_eq!(result.clock_analysis.persistent_direction, Some(direction));
      assert_eq!(
        result.drift_disposition,
        DriftDisposition::ClockDriftCompensationRequired
      );
      assert!(!result.thirty_minute_accepted);
      assert_eq!(
        result.follow_up,
        FollowUpGuidance::ProposeClockDriftCompensation
      );
    }
  }

  #[test]
  fn different_nominal_sample_rates_are_normalized_before_ppm_comparison() {
    let mut evidence = synthetic_evidence_with_rates(&[0.0; 6], 48_000, 96_000);
    add_sink_diagnostics(&mut evidence);
    let result = report(&evidence, Some(confirmed_operator()));
    assert_eq!(
      result.drift_disposition,
      DriftDisposition::BoundedSynchronizerSufficient
    );
    assert!(result
      .clock_analysis
      .median_relative_ppm
      .is_some_and(|ppm| ppm.abs() < f64::EPSILON));
  }

  #[test]
  fn render_faster_rate_agrees_with_negative_render_minus_microphone_delta_trend() {
    let mut evidence = synthetic_evidence(&[10.0; 6]);
    for (index, event) in evidence.events.iter_mut().enumerate() {
      event.snapshot.current_delta_100ns = Some(
        -i64::try_from(index)
          .expect("synthetic event index fits i64")
          .saturating_mul(100),
      );
    }
    let result = report(&evidence, Some(confirmed_operator()));
    assert_eq!(
      result.drift_disposition,
      DriftDisposition::ClockDriftCompensationRequired
    );
  }

  #[test]
  fn render_faster_rate_conflicts_with_positive_render_minus_microphone_delta_trend() {
    let mut evidence = synthetic_evidence(&[10.0; 6]);
    for (index, event) in evidence.events.iter_mut().enumerate() {
      event.snapshot.current_delta_100ns = Some(
        i64::try_from(index)
          .expect("synthetic event index fits i64")
          .saturating_mul(100),
      );
    }
    let result = report(&evidence, Some(confirmed_operator()));
    assert_eq!(result.drift_disposition, DriftDisposition::Inconclusive);
  }

  #[test]
  fn inconsistent_windows_are_inconclusive() {
    let mut evidence = synthetic_evidence(&[12.0, -12.0, 12.0, -12.0, 12.0, -12.0]);
    add_sink_diagnostics(&mut evidence);
    let result = report(&evidence, Some(confirmed_operator()));
    assert_eq!(result.drift_disposition, DriftDisposition::Inconclusive);
  }

  #[test]
  fn discontinuity_and_render_silence_split_clean_segments() {
    let mut evidence = synthetic_evidence(&[0.0; 6]);
    evidence.events[600].snapshot.discontinuities = 1;
    let held_render = evidence.events[900].snapshot.last_render_device_position;
    for event in &mut evidence.events[901..910] {
      event.snapshot.last_render_device_position = held_render;
    }
    let result = report(&evidence, Some(confirmed_operator()));
    assert!(result.data_quality.clean_segments.len() >= 2);
    assert!(result
      .data_quality
      .excluded_intervals
      .iter()
      .any(|interval| {
        interval.reason.contains("discontinuity") || interval.reason.contains("render")
      }));
  }

  #[test]
  fn short_or_legacy_evidence_is_not_authoritative() {
    let mut evidence = synthetic_evidence(&[0.0; 2]);
    evidence.authoritative = false;
    evidence.event_schema_version = 1;
    evidence.requested_duration_ms = 0;
    let result = report(&evidence, Some(confirmed_operator()));
    assert_eq!(result.drift_disposition, DriftDisposition::Inconclusive);
    assert!(!result.authoritative);
  }

  #[test]
  fn recurring_directional_maintenance_requires_compensation() {
    let mut evidence = synthetic_evidence(&[10.0; 6]);
    for (index, event) in evidence.events.iter_mut().enumerate() {
      event.snapshot.silent_render_references = (index / 300) as u64;
    }
    let result = report(&evidence, Some(confirmed_operator()));
    assert_eq!(
      result.drift_disposition,
      DriftDisposition::ClockDriftCompensationRequired
    );
  }

  #[test]
  fn functional_failures_are_independent_from_drift() {
    let mut evidence = synthetic_evidence(&[0.0; 6]);
    let last = evidence.events.last_mut().expect("synthetic final event");
    last.snapshot.processing_deadline_misses = 1;
    last.snapshot.last_error = Some(EngineError::new(
      EngineErrorKind::WorkerFailure,
      "synthetic failure",
    ));
    evidence.terminal_event = ValidationEventKind::Failed;
    last.event = ValidationEventKind::Failed;
    let result = report(&evidence, Some(confirmed_operator()));
    assert_eq!(
      result.drift_disposition,
      DriftDisposition::BoundedSynchronizerSufficient
    );
    assert_eq!(result.functional_disposition, FunctionalDisposition::Failed);
  }

  #[test]
  fn missing_operator_observations_leave_functional_gate_inconclusive() {
    let result = report(&synthetic_evidence(&[0.0; 6]), None);
    assert_eq!(
      result.functional_disposition,
      FunctionalDisposition::Inconclusive
    );
    assert_eq!(
      result.follow_up,
      FollowUpGuidance::CompleteOperatorObservations
    );
  }

  #[test]
  fn counters_present_at_start_are_not_reported_as_run_increases() {
    let mut evidence = synthetic_evidence(&[0.0; 6]);
    for event in &mut evidence.events {
      event.snapshot.discontinuities = 1;
      event.snapshot.render_discontinuities = 1;
      event.snapshot.alignment_resets = 2;
      event.snapshot.aec_resets = 2;
      event.snapshot.aec_rebuilds = 2;
    }
    add_sink_diagnostics(&mut evidence);
    let result = report(&evidence, Some(confirmed_operator()));
    assert_eq!(result.functional_disposition, FunctionalDisposition::Passed);
  }

  #[test]
  fn earliest_available_driver_snapshot_is_used_when_start_snapshot_is_missing() {
    let mut evidence = synthetic_evidence(&[0.0; 6]);
    evidence
      .events
      .first_mut()
      .expect("synthetic started event")
      .snapshot
      .sink_diagnostics_latest = Some(transport_snapshot(100));
    let final_snapshot = &mut evidence
      .events
      .last_mut()
      .expect("synthetic final event")
      .snapshot;
    final_snapshot.sink_diagnostics_latest = Some(transport_snapshot(
      final_snapshot.sink_accepted_frames.saturating_add(100),
    ));

    let result = report(&evidence, Some(confirmed_operator()));

    assert_eq!(result.functional_disposition, FunctionalDisposition::Passed);
    assert!(result.thirty_minute_accepted);
  }

  #[test]
  fn output_path_rejects_parent_traversal_and_repository_content() {
    assert!(validated_output_path(Path::new("artifacts/../README.md")).is_err());
    assert!(validated_output_path(Path::new("README-report.json")).is_err());
  }

  #[test]
  fn parser_rejects_truncated_and_mixed_run_evidence() {
    assert!(parse_evidence(Cursor::new(b"{not-json}\n".as_slice())).is_err());

    let mut evidence = synthetic_evidence(&[0.0; 6]);
    evidence.events[10].snapshot.run_id = Some(99);
    assert!(parse_evidence(Cursor::new(serialized_evidence(&evidence))).is_err());
  }

  #[test]
  fn parser_accepts_aec_rebuild_and_segments_instance_change() {
    let mut evidence = synthetic_evidence(&[0.0; 6]);
    for event in &mut evidence.events[600..] {
      event.snapshot.aec_instance_id = Some(17);
    }

    let parsed = parse_evidence(Cursor::new(serialized_evidence(&evidence)))
      .expect("AEC rebuild stays within one coherent run");
    let result = report(&parsed, Some(confirmed_operator()));
    assert!(result
      .data_quality
      .excluded_intervals
      .iter()
      .any(|interval| interval.reason == "AEC instance identity changed"));
  }

  #[test]
  fn parser_rejects_mixed_sink_sessions() {
    let mut evidence = synthetic_evidence(&[0.0; 6]);
    evidence.events[10].snapshot.session_id = Some(12);
    assert!(parse_evidence(Cursor::new(serialized_evidence(&evidence))).is_err());
  }

  #[test]
  fn report_and_operator_observations_have_stable_json_shapes() {
    let operator = confirmed_operator();
    let operator_json = serde_json::to_vec(&operator).expect("operator observations serialize");
    let parsed_operator: OperatorObservations =
      serde_json::from_slice(&operator_json).expect("operator observations parse");
    let mut evidence = synthetic_evidence(&[0.0; 6]);
    add_sink_diagnostics(&mut evidence);
    let result = report(&evidence, Some(parsed_operator));
    let report_json = serde_json::to_value(result).expect("stability report serializes");
    assert_eq!(report_json["schema_version"], REPORT_SCHEMA_VERSION);
    assert_eq!(
      report_json["drift_disposition"],
      "bounded-synchronizer-sufficient"
    );
    assert_eq!(report_json["functional_disposition"], "passed");
    assert_eq!(report_json["thirty_minute_accepted"], true);
    assert_eq!(
      report_json["follow_up"],
      "monitor-early-version-diagnostics"
    );
    assert!(report_json.get("two_hour_eligible").is_none());
  }
}
