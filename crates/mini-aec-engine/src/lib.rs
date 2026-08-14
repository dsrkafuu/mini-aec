//! Tauri-independent real-time engine for the `MiniAEC Microphone` product path.
//!
//! This crate owns lifecycle, normalization, framing, bounded buffering and diagnostics. Windows,
//! CLI and UI details are adapters around the project-owned contracts exposed here.

mod aec;
mod framing;
mod queue;
mod runtime;
mod synchronization;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
#[cfg(windows)]
pub mod windows;

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::time::Duration;

use mini_aec_transport::{
  SessionState, SinkDiagnostics, SinkError, SinkErrorKind, VirtualMicrophoneSink,
};
use serde::{Deserialize, Serialize};

pub use aec::{
  DefaultEchoCancellerFactory, EchoCanceller, EchoCancellerError, EchoCancellerFactory,
};
pub use runtime::Engine;

/// Public capture endpoint exposed by the `MiniAEC` driver.
pub const PUBLIC_CAPTURE_ENDPOINT_NAME: &str = "MiniAEC Microphone";
/// Fixed engine sample rate.
pub const SAMPLE_RATE_HZ: u32 = 48_000;
/// Fixed engine channel count.
pub const CHANNELS: u16 = 1;
/// Maximum time an input read may wait before observing cancellation.
pub const INPUT_WAIT: Duration = Duration::from_millis(100);

/// Product processing mode selected explicitly by the controller.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingMode {
  Bypass,
  Aec,
}

/// Role of one explicitly selected physical Windows endpoint.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputRole {
  Microphone,
  RenderLoopback,
}

/// Configuration for one explicit real-time engine run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EngineConfig {
  pub mode: ProcessingMode,
  pub microphone_endpoint_id: String,
  pub render_endpoint_id: Option<String>,
}

impl EngineConfig {
  /// Creates an explicit physical-microphone bypass configuration.
  #[must_use]
  pub fn new(source_endpoint_id: impl Into<String>) -> Self {
    Self::bypass(source_endpoint_id)
  }

  #[must_use]
  pub fn bypass(microphone_endpoint_id: impl Into<String>) -> Self {
    Self {
      mode: ProcessingMode::Bypass,
      microphone_endpoint_id: microphone_endpoint_id.into(),
      render_endpoint_id: None,
    }
  }

  #[must_use]
  pub fn aec(
    microphone_endpoint_id: impl Into<String>,
    render_endpoint_id: impl Into<String>,
  ) -> Self {
    Self {
      mode: ProcessingMode::Aec,
      microphone_endpoint_id: microphone_endpoint_id.into(),
      render_endpoint_id: Some(render_endpoint_id.into()),
    }
  }
}

/// Low-frequency commands accepted by the reusable engine controller.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EngineCommand {
  Start(EngineConfig),
  Stop,
  Restart(EngineConfig),
}

/// Observable lifecycle state of the real-time engine.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineState {
  #[default]
  Stopped,
  Starting,
  RunningBypass,
  RunningAec,
  Degraded,
  Stopping,
  Failed,
}

/// Observable reason an AEC run is temporarily impaired.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DegradationReason {
  RenderReferenceMissing,
  AecReset,
  ProcessingDeadline,
  QueuePressure,
}

/// Native metadata retained for the explicitly selected capture endpoint.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceFormat {
  pub sample_rate_hz: u32,
  pub channels: u16,
  pub bits_per_sample: u16,
}

/// Project-owned capture endpoint identity and display metadata.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceDescriptor {
  pub role: InputRole,
  pub endpoint_id: String,
  pub friendly_name: String,
  pub active: bool,
  pub native_format: Option<SourceFormat>,
}

/// Metadata associated with one capture packet written into a caller-owned buffer.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PacketMetadata {
  pub frames: usize,
  pub silent: bool,
  pub data_discontinuity: bool,
  pub timestamp_error: bool,
  pub device_position: u64,
  pub qpc_timestamp_100ns: u64,
}

/// Project-owned physical capture failures.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceErrorKind {
  InvalidSource,
  Unavailable,
  AccessDenied,
  DeviceInvalidated,
  CaptureFailure,
}

/// Actionable capture error without WASAPI or Windows types in the public contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceError {
  pub kind: SourceErrorKind,
  pub message: String,
}

impl SourceError {
  #[must_use]
  pub fn new(kind: SourceErrorKind, message: impl Into<String>) -> Self {
    Self {
      kind,
      message: message.into(),
    }
  }
}

impl Display for SourceError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    write!(formatter, "{:?}: {}", self.kind, self.message)
  }
}

impl Error for SourceError {}

/// One bounded, normalized 48 kHz mono capture stream.
pub trait AudioInput {
  /// Waits for at most `timeout`, writes mono `f32` samples into `samples`, and returns packet
  /// metadata. `Ok(None)` means the finite wait elapsed without a packet.
  ///
  /// # Errors
  ///
  /// Returns an actionable source error when capture fails or the endpoint is invalidated.
  fn read_packet(
    &mut self,
    samples: &mut [f32],
    timeout: Duration,
  ) -> Result<Option<PacketMetadata>, SourceError>;

  /// Stops the native stream. Implementations must make repeated calls safe.
  ///
  /// # Errors
  ///
  /// Returns an actionable source error when the native stream cannot stop cleanly.
  fn stop(&mut self) -> Result<(), SourceError>;
}

/// Resolves an exact endpoint identity and opens it on the capture worker thread.
pub trait AudioInputFactory: Send + Sync {
  /// Resolves one exact endpoint identity without selecting a fallback.
  ///
  /// # Errors
  ///
  /// Returns an actionable source error when the endpoint is absent, inactive or inaccessible.
  fn resolve(&self, role: InputRole, endpoint_id: &str) -> Result<SourceDescriptor, SourceError>;

  /// Opens the resolved endpoint on the calling capture worker thread.
  ///
  /// # Errors
  ///
  /// Returns an actionable source error when the stream cannot be initialized.
  fn open(
    &self,
    role: InputRole,
    source: &SourceDescriptor,
  ) -> Result<Box<dyn AudioInput>, SourceError>;
}

/// Creates one virtual microphone adapter for each engine run.
pub trait VirtualSinkFactory: Send + Sync {
  /// Connects one adapter instance for a new engine run.
  ///
  /// # Errors
  ///
  /// Returns a transport error when the driver is absent, inaccessible or busy.
  fn connect(&self) -> Result<Box<dyn VirtualMicrophoneSink>, SinkError>;
}

/// Stable error categories reported by the engine controller and snapshots.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineErrorKind {
  InvalidConfiguration,
  InvalidSource,
  SourceUnavailable,
  SourceAccessDenied,
  SourceInvalidated,
  SourceFailure,
  RenderUnavailable,
  RenderAccessDenied,
  RenderInvalidated,
  RenderFailure,
  SynchronizationFailure,
  EchoCancellerFailure,
  DriverUnavailable,
  SinkAccessDenied,
  SenderBusy,
  VersionMismatch,
  RejectedWrite,
  SinkFailure,
  WorkerFailure,
  StopTimeout,
}

/// Actionable terminal error for one engine run.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EngineError {
  pub kind: EngineErrorKind,
  pub message: String,
}

impl EngineError {
  #[must_use]
  pub fn new(kind: EngineErrorKind, message: impl Into<String>) -> Self {
    Self {
      kind,
      message: message.into(),
    }
  }

  pub(crate) fn from_source(role: InputRole, error: SourceError) -> Self {
    let kind = match (role, error.kind) {
      (_, SourceErrorKind::InvalidSource) => EngineErrorKind::InvalidSource,
      (InputRole::Microphone, SourceErrorKind::Unavailable) => EngineErrorKind::SourceUnavailable,
      (InputRole::Microphone, SourceErrorKind::AccessDenied) => EngineErrorKind::SourceAccessDenied,
      (InputRole::Microphone, SourceErrorKind::DeviceInvalidated) => {
        EngineErrorKind::SourceInvalidated
      }
      (InputRole::Microphone, SourceErrorKind::CaptureFailure) => EngineErrorKind::SourceFailure,
      (InputRole::RenderLoopback, SourceErrorKind::Unavailable) => {
        EngineErrorKind::RenderUnavailable
      }
      (InputRole::RenderLoopback, SourceErrorKind::AccessDenied) => {
        EngineErrorKind::RenderAccessDenied
      }
      (InputRole::RenderLoopback, SourceErrorKind::DeviceInvalidated) => {
        EngineErrorKind::RenderInvalidated
      }
      (InputRole::RenderLoopback, SourceErrorKind::CaptureFailure) => {
        EngineErrorKind::RenderFailure
      }
    };
    Self::new(kind, error.message)
  }

  pub(crate) fn from_sink(error: &SinkError) -> Self {
    let kind = match error.kind() {
      SinkErrorKind::AccessDenied => EngineErrorKind::SinkAccessDenied,
      SinkErrorKind::DriverUnavailable => EngineErrorKind::DriverUnavailable,
      SinkErrorKind::Busy => EngineErrorKind::SenderBusy,
      SinkErrorKind::VersionMismatch => EngineErrorKind::VersionMismatch,
      SinkErrorKind::RejectedWrite | SinkErrorKind::SequenceViolation => {
        EngineErrorKind::RejectedWrite
      }
      _ => EngineErrorKind::SinkFailure,
    };
    Self::new(kind, error.message())
  }
}

impl Display for EngineError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    write!(formatter, "{:?}: {}", self.kind, self.message)
  }
}

impl Error for EngineError {}

/// Metadata-only snapshot of the current or most recently completed run.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct EngineSnapshot {
  pub state: EngineState,
  pub mode: Option<ProcessingMode>,
  pub source: Option<SourceDescriptor>,
  pub render_source: Option<SourceDescriptor>,
  pub run_id: Option<u128>,
  pub session_id: Option<u128>,
  pub aec_instance_id: Option<u128>,
  pub captured_packets: u64,
  pub captured_samples: u64,
  pub silent_packets: u64,
  pub discontinuities: u64,
  pub timestamp_errors: u64,
  pub sanitized_samples: u64,
  pub output_frames: u64,
  pub queue_depth: u32,
  pub queue_high_water: u32,
  pub queue_overflows: u64,
  pub discarded_frames: u64,
  pub sink_accepted_frames: u64,
  pub sink_failures: u64,
  pub sink_diagnostics_start: Option<SinkTransportSnapshot>,
  pub sink_diagnostics_latest: Option<SinkTransportSnapshot>,
  pub last_device_position: Option<u64>,
  pub last_qpc_timestamp_100ns: Option<u64>,
  pub render_packets: u64,
  pub render_samples: u64,
  pub render_silent_packets: u64,
  pub render_discontinuities: u64,
  pub render_timestamp_errors: u64,
  pub render_sanitized_samples: u64,
  pub render_frames: u64,
  pub render_queue_depth: u32,
  pub render_queue_high_water: u32,
  pub render_queue_overflows: u64,
  pub render_discarded_frames: u64,
  pub last_render_device_position: Option<u64>,
  pub last_render_qpc_timestamp_100ns: Option<u64>,
  pub synchronization_epoch: u64,
  pub synchronization_origin_qpc_100ns: Option<u64>,
  pub current_delta_100ns: Option<i64>,
  pub maximum_absolute_skew_100ns: u64,
  pub paired_frames: u64,
  pub silent_render_references: u64,
  pub stale_render_frames: u64,
  pub alignment_resets: u64,
  pub aec_processed_frames: u64,
  pub aec_resets: u64,
  pub aec_rebuilds: u64,
  pub aec_invalid_outputs: u64,
  pub processing_deadline_misses: u64,
  pub processing_time: ProcessingTimeSnapshot,
  pub degradation_reason: Option<DegradationReason>,
  pub last_error: Option<EngineError>,
}

/// Bounded integer processing-time percentiles in microseconds.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProcessingTimeSnapshot {
  pub samples: u64,
  pub p50_us: u64,
  pub p95_us: u64,
  pub p99_us: u64,
  pub maximum_us: u64,
}

/// Engine-owned serialization of the virtual sink's versioned transport diagnostics.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SinkTransportSnapshot {
  pub schema_version: u16,
  pub state: SinkTransportState,
  pub active_session_id: Option<u128>,
  pub last_accepted_sequence: Option<u64>,
  pub current_depth: u32,
  pub high_water_mark: u32,
  pub session_opens: u64,
  pub session_closes: u64,
  pub session_resets: u64,
  pub accepted_frames: u64,
  pub rejected_writes: u64,
  pub underruns: u64,
  pub overflows: u64,
  pub discarded_frames: u64,
  pub driver_restarts: u64,
}

impl From<SinkDiagnostics> for SinkTransportSnapshot {
  fn from(diagnostics: SinkDiagnostics) -> Self {
    Self {
      schema_version: diagnostics.schema_version,
      state: diagnostics.state.into(),
      active_session_id: diagnostics
        .active_session
        .map(mini_aec_transport::SessionId::get),
      last_accepted_sequence: diagnostics.last_accepted_sequence,
      current_depth: diagnostics.current_depth,
      high_water_mark: diagnostics.high_water_mark,
      session_opens: diagnostics.counters.session_opens,
      session_closes: diagnostics.counters.session_closes,
      session_resets: diagnostics.counters.session_resets,
      accepted_frames: diagnostics.counters.accepted_frames,
      rejected_writes: diagnostics.counters.rejected_writes,
      underruns: diagnostics.counters.underruns,
      overflows: diagnostics.counters.overflows,
      discarded_frames: diagnostics.counters.discarded_frames,
      driver_restarts: diagnostics.counters.driver_restarts,
    }
  }
}

/// Transport session state without exposing adapter-specific types in persisted evidence.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SinkTransportState {
  Closed,
  Open,
}

impl From<SessionState> for SinkTransportState {
  fn from(state: SessionState) -> Self {
    match state {
      SessionState::Closed => Self::Closed,
      SessionState::Open => Self::Open,
    }
  }
}

/// Metadata-only JSONL record emitted by validation controllers.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ValidationEvent {
  pub schema_version: u16,
  pub unix_ms: u128,
  #[serde(default)]
  pub monotonic_elapsed_ms: u128,
  #[serde(default)]
  pub requested_duration_ms: u128,
  pub event: ValidationEventKind,
  pub snapshot: EngineSnapshot,
}

/// Reason a metadata-only validation snapshot was recorded.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationEventKind {
  Started,
  Periodic,
  Final,
  Failed,
}

pub(crate) fn source_is_public_endpoint(source: &SourceDescriptor) -> bool {
  source
    .friendly_name
    .trim()
    .eq_ignore_ascii_case(PUBLIC_CAPTURE_ENDPOINT_NAME)
}
