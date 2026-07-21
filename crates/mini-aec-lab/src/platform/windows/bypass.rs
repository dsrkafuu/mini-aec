use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use mini_aec_engine::windows::WindowsAudioInputFactory;
use mini_aec_engine::{
  Engine, EngineConfig, EngineState, ValidationEvent, ValidationEventKind, VirtualSinkFactory,
};
use mini_aec_transport::{SinkError, VirtualMicrophoneSink};
use mini_aec_windows_transport::WindowsVirtualMicrophoneSink;

use crate::platform::BypassConfig;

const VALIDATION_SCHEMA_VERSION: u16 = 1;
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

  println!("Bypass evidence: {}", run_dir.display());
  let engine = Engine::new(
    Arc::new(WindowsAudioInputFactory),
    Arc::new(WindowsSinkFactory),
  );
  engine
    .start(EngineConfig::new(config.microphone_endpoint_id))
    .context("failed to start the MiniAEC real-time bypass engine")?;
  write_event(&mut events, ValidationEventKind::Started, &engine)?;

  let deadline = Instant::now() + config.duration;
  while Instant::now() < deadline {
    thread::sleep(SNAPSHOT_INTERVAL.min(deadline.saturating_duration_since(Instant::now())));
    let snapshot = engine.snapshot();
    if snapshot.state == EngineState::Failed {
      write_event(&mut events, ValidationEventKind::Failed, &engine)?;
      let failure = snapshot.last_error.map_or_else(
        || "unknown engine failure".to_owned(),
        |error| error.to_string(),
      );
      let _ = engine.stop();
      events.flush().context("failed to flush bypass evidence")?;
      bail!("real-time bypass failed: {failure}");
    }
    write_event(&mut events, ValidationEventKind::Periodic, &engine)?;
  }

  engine.stop().context("failed to stop the bypass engine")?;
  write_event(&mut events, ValidationEventKind::Final, &engine)?;
  events.flush().context("failed to flush bypass evidence")?;
  println!("Bypass completed: {}", run_dir.display());
  Ok(())
}

fn write_event(
  writer: &mut BufWriter<File>,
  event: ValidationEventKind,
  engine: &Engine,
) -> Result<()> {
  serde_json::to_writer(
    &mut *writer,
    &ValidationEvent {
      schema_version: VALIDATION_SCHEMA_VERSION,
      unix_ms: unix_time_ms()?,
      event,
      snapshot: engine.snapshot(),
    },
  )?;
  writer.write_all(b"\n")?;
  writer.flush()?;
  Ok(())
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

  use super::validated_evidence_root;

  #[test]
  fn evidence_path_stays_in_ignored_private_or_validation_roots() {
    assert!(validated_evidence_root(Path::new("artifacts/bypass")).is_ok());
    assert!(validated_evidence_root(Path::new("driver/windows/out/validation/engine")).is_ok());
    assert!(validated_evidence_root(Path::new("testdata/private-audio")).is_err());
    assert!(validated_evidence_root(Path::new("artifacts/../testdata")).is_err());
  }
}
