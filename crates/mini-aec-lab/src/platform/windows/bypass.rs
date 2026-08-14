use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use mini_aec_engine::windows::WindowsAudioInputFactory;
use mini_aec_engine::{
  DefaultEchoCancellerFactory, Engine, EngineConfig, EngineState, ValidationEvent,
  ValidationEventKind, VirtualSinkFactory,
};
use mini_aec_transport::{SinkError, VirtualMicrophoneSink};
use mini_aec_windows_transport::WindowsVirtualMicrophoneSink;

use crate::platform::{BypassConfig, RealtimeAecConfig};

const VALIDATION_SCHEMA_VERSION: u16 = 2;
const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(1);

struct WindowsSinkFactory;

impl VirtualSinkFactory for WindowsSinkFactory {
  fn connect(&self) -> Result<Box<dyn VirtualMicrophoneSink>, SinkError> {
    WindowsVirtualMicrophoneSink::connect()
      .map(|sink| Box::new(sink) as Box<dyn VirtualMicrophoneSink>)
  }
}

pub fn bypass(config: BypassConfig) -> Result<()> {
  if config.duration.is_zero() {
    bail!("bypass duration must be greater than zero");
  }
  if config.microphone_endpoint_id.trim().is_empty() {
    bail!("an exact physical microphone endpoint ID is required");
  }

  let output_root = validated_evidence_root(&config.output_root)?;
  let run_dir = output_root.join(unix_time_ms()?.to_string());
  fs::create_dir_all(&run_dir)
    .with_context(|| format!("failed to create {}", run_dir.display()))?;
  let events_path = run_dir.join("engine.jsonl");
  let mut events = BufWriter::new(
    File::create(&events_path)
      .with_context(|| format!("failed to create {}", events_path.display()))?,
  );

  let engine = Engine::new(
    Arc::new(WindowsAudioInputFactory),
    Arc::new(WindowsSinkFactory),
  );
  run_engine(
    &engine,
    EngineConfig::bypass(config.microphone_endpoint_id),
    config.duration,
    &run_dir,
    &mut events,
    "real-time bypass",
  )?;
  Ok(())
}

pub fn realtime_aec(config: RealtimeAecConfig) -> Result<()> {
  if config.duration.is_zero() {
    bail!("real-time AEC duration must be greater than zero");
  }
  if config.microphone_endpoint_id.trim().is_empty() || config.render_endpoint_id.trim().is_empty()
  {
    bail!("exact physical microphone and render endpoint IDs are required");
  }
  let output_root = validated_evidence_root(&config.output_root)?;
  let run_dir = output_root.join(unix_time_ms()?.to_string());
  fs::create_dir_all(&run_dir)
    .with_context(|| format!("failed to create {}", run_dir.display()))?;
  let events_path = run_dir.join("engine.jsonl");
  let mut events = BufWriter::new(
    File::create(&events_path)
      .with_context(|| format!("failed to create {}", events_path.display()))?,
  );
  let engine = Engine::new_with_aec(
    Arc::new(WindowsAudioInputFactory),
    Arc::new(WindowsSinkFactory),
    Arc::new(DefaultEchoCancellerFactory),
  );
  run_engine(
    &engine,
    EngineConfig::aec(config.microphone_endpoint_id, config.render_endpoint_id),
    config.duration,
    &run_dir,
    &mut events,
    "real-time default AEC",
  )
}

fn run_engine(
  engine: &Engine,
  config: EngineConfig,
  duration: Duration,
  run_dir: &Path,
  events: &mut BufWriter<File>,
  label: &str,
) -> Result<()> {
  println!("{label} evidence: {}", run_dir.display());
  let run_started = Instant::now();
  engine
    .start(config)
    .with_context(|| format!("failed to start the MiniAEC {label} engine"))?;
  write_event(
    events,
    ValidationEventKind::Started,
    engine,
    Duration::ZERO,
    duration,
  )?;
  let deadline = run_started + duration;
  while Instant::now() < deadline {
    thread::sleep(SNAPSHOT_INTERVAL.min(deadline.saturating_duration_since(Instant::now())));
    let snapshot = engine.snapshot();
    if snapshot.state == EngineState::Failed {
      write_event(
        events,
        ValidationEventKind::Failed,
        engine,
        run_started.elapsed(),
        duration,
      )?;
      let failure = snapshot.last_error.map_or_else(
        || "unknown engine failure".to_owned(),
        |error| error.to_string(),
      );
      let _ = engine.stop();
      events
        .flush()
        .with_context(|| format!("failed to flush {label} evidence"))?;
      bail!("{label} failed: {failure}");
    }
    write_event(
      events,
      ValidationEventKind::Periodic,
      engine,
      run_started.elapsed(),
      duration,
    )?;
  }
  engine
    .stop()
    .with_context(|| format!("failed to stop the {label} engine"))?;
  write_event(
    events,
    ValidationEventKind::Final,
    engine,
    run_started.elapsed(),
    duration,
  )?;
  events
    .flush()
    .with_context(|| format!("failed to flush {label} evidence"))?;
  println!("{label} completed: {}", run_dir.display());
  Ok(())
}

fn write_event(
  writer: &mut BufWriter<File>,
  event: ValidationEventKind,
  engine: &Engine,
  monotonic_elapsed: Duration,
  requested_duration: Duration,
) -> Result<()> {
  let record = validation_event(
    event,
    engine.snapshot(),
    unix_time_ms()?,
    monotonic_elapsed,
    requested_duration,
  );
  serde_json::to_writer(&mut *writer, &record)?;
  writer.write_all(b"\n")?;
  writer.flush()?;
  Ok(())
}

fn validation_event(
  event: ValidationEventKind,
  snapshot: mini_aec_engine::EngineSnapshot,
  unix_ms: u128,
  monotonic_elapsed: Duration,
  requested_duration: Duration,
) -> ValidationEvent {
  ValidationEvent {
    schema_version: VALIDATION_SCHEMA_VERSION,
    unix_ms,
    monotonic_elapsed_ms: monotonic_elapsed.as_millis(),
    requested_duration_ms: requested_duration.as_millis(),
    event,
    snapshot,
  }
}

fn validated_evidence_root(requested: &Path) -> Result<PathBuf> {
  if requested
    .components()
    .any(|component| matches!(component, Component::ParentDir))
  {
    bail!("bypass evidence path must not contain parent-directory traversal");
  }
  let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
    .parent()
    .and_then(Path::parent)
    .context("failed to resolve the MiniAEC repository root")?;
  let absolute = if requested.is_absolute() {
    requested.to_path_buf()
  } else {
    repository.join(requested)
  };
  let artifacts = repository.join("artifacts");
  let driver_validation = repository.join("driver").join("windows").join("out");
  if !absolute.starts_with(&artifacts) && !absolute.starts_with(&driver_validation) {
    bail!(
      "bypass evidence must remain below {} or {}",
      artifacts.display(),
      driver_validation.display()
    );
  }
  Ok(absolute)
}

fn unix_time_ms() -> Result<u128> {
  Ok(
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .context("system clock is before the Unix epoch")?
      .as_millis(),
  )
}

#[cfg(test)]
mod tests {
  use std::path::Path;
  use std::time::Duration;

  use mini_aec_engine::{EngineSnapshot, ValidationEvent, ValidationEventKind};

  use super::{validated_evidence_root, validation_event};

  #[test]
  fn evidence_path_stays_in_ignored_private_or_validation_roots() {
    assert!(validated_evidence_root(Path::new("artifacts/bypass")).is_ok());
    assert!(validated_evidence_root(Path::new("driver/windows/out/validation/engine")).is_ok());
    assert!(validated_evidence_root(Path::new("testdata/private-audio")).is_err());
    assert!(validated_evidence_root(Path::new("artifacts/../testdata")).is_err());
  }

  #[test]
  fn schema_v2_event_records_monotonic_and_requested_duration() {
    let event = validation_event(
      ValidationEventKind::Started,
      EngineSnapshot::default(),
      123,
      Duration::ZERO,
      Duration::from_mins(30),
    );
    let serialized = serde_json::to_string(&event).expect("schema v2 event serializes");
    let parsed: ValidationEvent =
      serde_json::from_str(&serialized).expect("schema v2 event parses");
    assert_eq!(parsed.schema_version, 2);
    assert_eq!(parsed.monotonic_elapsed_ms, 0);
    assert_eq!(parsed.requested_duration_ms, 1_800_000);
  }

  #[test]
  fn schema_v1_event_defaults_new_timing_fields_for_diagnostics() {
    let serialized = serde_json::json!({
      "schema_version": 1,
      "unix_ms": 123,
      "event": "started",
      "snapshot": EngineSnapshot::default(),
    });
    let parsed: ValidationEvent =
      serde_json::from_value(serialized).expect("schema v1 event remains readable");
    assert_eq!(parsed.monotonic_elapsed_ms, 0);
    assert_eq!(parsed.requested_duration_ms, 0);
  }
}
