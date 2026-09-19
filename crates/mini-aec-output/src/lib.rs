//! Platform-neutral contract between the `MiniAEC` engine and an external audio bridge.

use std::error::Error;
use std::fmt::{self, Display, Formatter};

pub const DIAGNOSTICS_SCHEMA_VERSION: u16 = 3;
pub const FRAME_SAMPLES: usize = 480;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcmFormat {
  sample_rate_hz: u32,
  channels: u16,
  bits_per_sample: u16,
  frame_samples_per_channel: u16,
}

impl PcmFormat {
  pub const MONO_48_KHZ_PCM16: Self = Self {
    sample_rate_hz: 48_000,
    channels: 1,
    bits_per_sample: 16,
    frame_samples_per_channel: 480,
  };
  #[must_use]
  pub const fn sample_rate_hz(self) -> u32 {
    self.sample_rate_hz
  }
  #[must_use]
  pub const fn channels(self) -> u16 {
    self.channels
  }
  #[must_use]
  pub const fn bits_per_sample(self) -> u16 {
    self.bits_per_sample
  }
  #[must_use]
  pub const fn frame_samples_per_channel(self) -> u16 {
    self.frame_samples_per_channel
  }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SessionId(u128);

impl SessionId {
  #[must_use]
  pub const fn new(value: u128) -> Option<Self> {
    if value == 0 {
      None
    } else {
      Some(Self(value))
    }
  }
  #[must_use]
  pub const fn get(self) -> u128 {
    self.0
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionConfig {
  session_id: SessionId,
  format: PcmFormat,
}

impl SessionConfig {
  #[must_use]
  pub const fn new(session_id: SessionId) -> Self {
    Self {
      session_id,
      format: PcmFormat::MONO_48_KHZ_PCM16,
    }
  }
  #[must_use]
  pub const fn session_id(self) -> SessionId {
    self.session_id
  }
  #[must_use]
  pub const fn format(self) -> PcmFormat {
    self.format
  }
}

#[derive(Clone, Copy, Debug)]
pub struct PcmFrame<'a> {
  sequence: u64,
  samples: &'a [i16; FRAME_SAMPLES],
}

impl<'a> PcmFrame<'a> {
  #[must_use]
  pub const fn new(sequence: u64, samples: &'a [i16; FRAME_SAMPLES]) -> Self {
    Self { sequence, samples }
  }
  #[must_use]
  pub const fn sequence(self) -> u64 {
    self.sequence
  }
  #[must_use]
  pub const fn samples(self) -> &'a [i16; FRAME_SAMPLES] {
    self.samples
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WriteReceipt {
  sequence: u64,
}

impl WriteReceipt {
  #[must_use]
  pub const fn accepted(sequence: u64) -> Self {
    Self { sequence }
  }
  #[must_use]
  pub const fn sequence(self) -> u64 {
    self.sequence
  }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndpointRole {
  Playback,
  Recording,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputEndpointDescriptor {
  pub endpoint_id: String,
  pub friendly_name: String,
  pub role: EndpointRole,
  pub active: bool,
  /// Stable adapter-family evidence, independent from the user-visible endpoint label.
  pub device_family: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputPair {
  pub playback: OutputEndpointDescriptor,
  pub recording: OutputEndpointDescriptor,
}

/// Resolve exact endpoint identities without default-device or friendly-name fallback.
///
/// # Errors
/// Returns an actionable error if either identity is missing, duplicated, inactive, has the wrong
/// role, or lacks matching VB-Audio adapter-family evidence.
pub fn resolve_pair(
  inventory: &[OutputEndpointDescriptor],
  playback_id: &str,
  recording_id: &str,
) -> Result<OutputPair, OutputError> {
  if playback_id.trim().is_empty() || recording_id.trim().is_empty() {
    return Err(OutputError::new(
      OutputErrorKind::InvalidEndpoint,
      "exact CABLE Input and CABLE Output endpoint IDs are required",
    ));
  }
  if playback_id == recording_id {
    return Err(OutputError::new(
      OutputErrorKind::InvalidEndpoint,
      "CABLE Input and CABLE Output must be distinct endpoint identities",
    ));
  }
  let select = |id: &str| -> Result<OutputEndpointDescriptor, OutputError> {
    let matches: Vec<_> = inventory
      .iter()
      .filter(|endpoint| endpoint.endpoint_id == id)
      .collect();
    match matches.as_slice() {
      [] => Err(OutputError::new(
        OutputErrorKind::PrerequisiteMissing,
        format!("configured VB-CABLE endpoint {id:?} is unavailable"),
      )),
      [endpoint] => Ok((*endpoint).clone()),
      _ => Err(OutputError::new(
        OutputErrorKind::AmbiguousEndpoints,
        format!("configured endpoint identity {id:?} is ambiguous"),
      )),
    }
  };
  let playback = select(playback_id)?;
  let recording = select(recording_id)?;
  if playback.role != EndpointRole::Playback || recording.role != EndpointRole::Recording {
    return Err(OutputError::new(
      OutputErrorKind::InvalidEndpoint,
      "configured VB-CABLE endpoints have the wrong Windows data-flow roles",
    ));
  }
  if !playback.active || !recording.active {
    return Err(OutputError::new(
      OutputErrorKind::PrerequisiteMissing,
      "configured VB-CABLE endpoint pair is not active",
    ));
  }
  let family = playback.device_family.trim();
  if family.is_empty()
    || !family.eq_ignore_ascii_case(recording.device_family.trim())
    || !family.to_ascii_lowercase().contains("vb-audio")
  {
    return Err(OutputError::new(
      OutputErrorKind::InvalidEndpoint,
      "configured endpoints do not share corroborating VB-Audio adapter metadata",
    ));
  }
  Ok(OutputPair {
    playback,
    recording,
  })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OutputErrorKind {
  AccessDenied,
  PrerequisiteMissing,
  AmbiguousEndpoints,
  InvalidEndpoint,
  DeviceInvalidated,
  InvalidState,
  InvalidFormat,
  SequenceViolation,
  RejectedWrite,
  RenderFailure,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputError {
  kind: OutputErrorKind,
  message: Box<str>,
}

impl OutputError {
  #[must_use]
  pub fn new(kind: OutputErrorKind, message: impl Into<Box<str>>) -> Self {
    Self {
      kind,
      message: message.into(),
    }
  }
  #[must_use]
  pub const fn kind(&self) -> OutputErrorKind {
    self.kind
  }
  #[must_use]
  pub fn message(&self) -> &str {
    &self.message
  }
}

impl Display for OutputError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    write!(formatter, "{:?}: {}", self.kind, self.message)
  }
}
impl Error for OutputError {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SessionState {
  #[default]
  Closed,
  Open,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputSampleType {
  Integer,
  Float,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputFormat {
  pub sample_rate_hz: u32,
  pub channels: u16,
  pub bits_per_sample: u16,
  pub sample_type: OutputSampleType,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticCounters {
  pub session_opens: u64,
  pub session_closes: u64,
  pub accepted_frames: u64,
  pub rejected_writes: u64,
  pub underruns: u64,
  pub overflows: u64,
  pub discarded_frames: u64,
  pub converted_frames: u64,
  pub endpoint_invalidations: u64,
  pub output_failures: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputDiagnostics {
  pub schema_version: u16,
  pub state: SessionState,
  pub active_session: Option<SessionId>,
  pub last_accepted_sequence: Option<u64>,
  pub current_depth: u32,
  pub high_water_mark: u32,
  pub current_padding_frames: u32,
  pub rendered_frames: u64,
  pub output_clock_frequency: Option<u64>,
  pub output_clock_position: Option<u64>,
  pub output_clock_qpc_position: Option<u64>,
  pub negotiated_format: Option<OutputFormat>,
  pub counters: DiagnosticCounters,
}

impl Default for OutputDiagnostics {
  fn default() -> Self {
    Self {
      schema_version: DIAGNOSTICS_SCHEMA_VERSION,
      state: SessionState::Closed,
      active_session: None,
      last_accepted_sequence: None,
      current_depth: 0,
      high_water_mark: 0,
      current_padding_frames: 0,
      rendered_frames: 0,
      output_clock_frequency: None,
      output_clock_position: None,
      output_clock_qpc_position: None,
      negotiated_format: None,
      counters: DiagnosticCounters::default(),
    }
  }
}

pub trait AudioOutput {
  /// Opens a fresh output session with empty adapter state.
  ///
  /// # Errors
  /// Returns an output error when the endpoint or negotiated format cannot start.
  fn open_session(&mut self, config: SessionConfig) -> Result<(), OutputError>;

  /// Submits one complete, monotonically sequenced engine frame.
  ///
  /// # Errors
  /// Returns an output error on sequence, bounded-capacity, invalidation or render failure.
  fn write_frame(&mut self, frame: PcmFrame<'_>) -> Result<WriteReceipt, OutputError>;

  /// Returns metadata-only output health for the active or most recent session.
  ///
  /// # Errors
  /// Returns an output error when native diagnostics cannot be queried safely.
  fn diagnostics(&self) -> Result<OutputDiagnostics, OutputError>;

  /// Stops and resets the output session. Repeated calls must be safe.
  ///
  /// # Errors
  /// Returns an output error when native resources cannot be stopped or reset cleanly.
  fn close_session(&mut self) -> Result<(), OutputError>;
}

#[cfg(test)]
mod tests {
  use super::*;

  fn endpoint(id: &str, role: EndpointRole) -> OutputEndpointDescriptor {
    OutputEndpointDescriptor {
      endpoint_id: id.to_owned(),
      friendly_name: format!("renamed {id}"),
      role,
      active: true,
      device_family: "VB-Audio Virtual Cable".to_owned(),
    }
  }

  #[test]
  fn exact_pair_resolution_ignores_renamed_display_labels() {
    let pair = resolve_pair(
      &[
        endpoint("input-id", EndpointRole::Playback),
        endpoint("output-id", EndpointRole::Recording),
      ],
      "input-id",
      "output-id",
    )
    .expect("pair resolves");
    assert_eq!(pair.playback.endpoint_id, "input-id");
    assert_eq!(pair.recording.endpoint_id, "output-id");
  }

  #[test]
  fn pair_resolution_rejects_missing_inactive_wrong_role_and_ambiguous_candidates() {
    let valid = [
      endpoint("input-id", EndpointRole::Playback),
      endpoint("output-id", EndpointRole::Recording),
    ];
    assert_eq!(
      resolve_pair(&valid, "missing", "output-id")
        .expect_err("missing fails")
        .kind(),
      OutputErrorKind::PrerequisiteMissing
    );
    let mut inactive = valid.clone();
    inactive[1].active = false;
    assert_eq!(
      resolve_pair(&inactive, "input-id", "output-id")
        .expect_err("inactive fails")
        .kind(),
      OutputErrorKind::PrerequisiteMissing
    );
    let wrong = [
      endpoint("input-id", EndpointRole::Recording),
      endpoint("output-id", EndpointRole::Playback),
    ];
    assert_eq!(
      resolve_pair(&wrong, "input-id", "output-id")
        .expect_err("roles fail")
        .kind(),
      OutputErrorKind::InvalidEndpoint
    );
    let ambiguous = [
      endpoint("input-id", EndpointRole::Playback),
      endpoint("input-id", EndpointRole::Playback),
      endpoint("output-id", EndpointRole::Recording),
    ];
    assert_eq!(
      resolve_pair(&ambiguous, "input-id", "output-id")
        .expect_err("ambiguity fails")
        .kind(),
      OutputErrorKind::AmbiguousEndpoints
    );
  }

  #[test]
  fn pair_resolution_never_uses_defaults_or_friendly_name_fallback() {
    let inventory = [
      endpoint("explicit-input", EndpointRole::Playback),
      endpoint("explicit-output", EndpointRole::Recording),
    ];
    assert_eq!(
      resolve_pair(&inventory, "CABLE Input", "CABLE Output")
        .expect_err("labels are not IDs")
        .kind(),
      OutputErrorKind::PrerequisiteMissing
    );
  }
}
