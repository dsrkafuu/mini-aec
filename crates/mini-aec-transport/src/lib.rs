//! Project-owned contract between `MiniAEC` PCM producers and Windows virtual microphone adapters.
//!
//! Windows transport details such as `WaveRT` endpoint identifiers, `IOCTL` values, shared-memory
//! layouts, and `SysVAD` types belong in adapter crates and must not cross this boundary.

use std::error::Error;
use std::fmt::{self, Display, Formatter};

/// Version of the diagnostics snapshot contract.
pub const DIAGNOSTICS_SCHEMA_VERSION: u16 = 1;

/// Number of PCM samples in one 10 ms mono frame.
pub const FRAME_SAMPLES: usize = 480;

/// Fixed PCM format accepted by `MiniAEC Microphone` validation transports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcmFormat {
  sample_rate_hz: u32,
  channels: u16,
  bits_per_sample: u16,
  frame_samples_per_channel: u16,
}

impl PcmFormat {
  /// `MiniAEC`'s fixed 48 kHz mono PCM16, 10 ms frame format.
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

/// Nonzero identity for one producer process session.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SessionId(u128);

impl SessionId {
  /// Creates an identity, returning `None` for the reserved zero value.
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

/// Request to begin a fixed-format producer session.
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

/// One fixed-size PCM frame submitted atomically to a sink.
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

/// Confirmation that a complete frame was accepted by the transport.
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

/// Project-level error categories shared by all Windows transport adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SinkErrorKind {
  AccessDenied,
  DriverUnavailable,
  VersionMismatch,
  InvalidState,
  InvalidFormat,
  SequenceViolation,
  RejectedWrite,
  TransportFailure,
}

/// Actionable transport error without adapter-specific Windows types.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SinkError {
  kind: SinkErrorKind,
  message: Box<str>,
}

impl SinkError {
  #[must_use]
  pub fn new(kind: SinkErrorKind, message: impl Into<Box<str>>) -> Self {
    Self {
      kind,
      message: message.into(),
    }
  }

  #[must_use]
  pub const fn kind(&self) -> SinkErrorKind {
    self.kind
  }

  #[must_use]
  pub fn message(&self) -> &str {
    &self.message
  }
}

impl Display for SinkError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    write!(formatter, "{:?}: {}", self.kind, self.message)
  }
}

impl Error for SinkError {}

/// Observable lifecycle state of one adapter instance.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SessionState {
  #[default]
  Closed,
  Open,
}

/// Monotonic counters common to both transport candidates.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticCounters {
  pub session_opens: u64,
  pub session_closes: u64,
  pub session_resets: u64,
  pub accepted_frames: u64,
  pub rejected_writes: u64,
  pub underruns: u64,
  pub overflows: u64,
  pub driver_restarts: u64,
}

/// Versioned diagnostic snapshot emitted by every transport adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SinkDiagnostics {
  pub schema_version: u16,
  pub state: SessionState,
  pub active_session: Option<SessionId>,
  pub last_accepted_sequence: Option<u64>,
  pub counters: DiagnosticCounters,
}

impl Default for SinkDiagnostics {
  fn default() -> Self {
    Self {
      schema_version: DIAGNOSTICS_SCHEMA_VERSION,
      state: SessionState::Closed,
      active_session: None,
      last_accepted_sequence: None,
      counters: DiagnosticCounters::default(),
    }
  }
}

/// Replaceable sink implemented by each Windows virtual microphone transport candidate.
///
/// A sink accepts exactly one open session at a time. Writes are whole-frame and atomic, and
/// sequences must be strictly monotonic within a session. `close_session` is idempotent so cleanup
/// can safely run after partial failures.
pub trait VirtualMicrophoneSink: Send {
  /// Opens one producer session using the fixed PCM contract.
  ///
  /// # Errors
  ///
  /// Returns an error when the adapter is already open, the driver is unavailable, access is
  /// denied, or the driver does not support the required format or protocol version.
  fn open_session(&mut self, config: SessionConfig) -> Result<(), SinkError>;

  /// Atomically submits one frame with a strictly monotonic sequence number.
  ///
  /// # Errors
  ///
  /// Returns an error when no session is open, the sequence is invalid, or the transport rejects
  /// or fails the write.
  fn write_frame(&mut self, frame: PcmFrame<'_>) -> Result<WriteReceipt, SinkError>;

  /// Reads the current versioned transport diagnostics snapshot.
  ///
  /// # Errors
  ///
  /// Returns an error when the adapter cannot query the underlying transport state.
  fn diagnostics(&self) -> Result<SinkDiagnostics, SinkError>;

  /// Closes the active producer session, or succeeds without action when already closed.
  ///
  /// # Errors
  ///
  /// Returns an error when the underlying transport cannot close or reset its active session.
  fn close_session(&mut self) -> Result<(), SinkError>;
}

#[cfg(test)]
mod tests {
  use super::{PcmFormat, SessionConfig, SessionId, SinkError, SinkErrorKind};

  #[test]
  fn fixed_format_contract_is_explicit() {
    let format = PcmFormat::MONO_48_KHZ_PCM16;
    assert_eq!(format.sample_rate_hz(), 48_000);
    assert_eq!(format.channels(), 1);
    assert_eq!(format.bits_per_sample(), 16);
    assert_eq!(format.frame_samples_per_channel(), 480);
  }

  #[test]
  fn session_config_cannot_select_another_format() {
    let session_id = SessionId::new(7).expect("test session is nonzero");
    let config = SessionConfig::new(session_id);
    assert_eq!(config.session_id(), session_id);
    assert_eq!(config.format(), PcmFormat::MONO_48_KHZ_PCM16);
  }

  #[test]
  fn adapter_error_mapping_preserves_category_and_context() {
    let cases = [
      SinkErrorKind::AccessDenied,
      SinkErrorKind::DriverUnavailable,
      SinkErrorKind::VersionMismatch,
      SinkErrorKind::InvalidState,
      SinkErrorKind::InvalidFormat,
      SinkErrorKind::SequenceViolation,
      SinkErrorKind::RejectedWrite,
      SinkErrorKind::TransportFailure,
    ];

    for kind in cases {
      let error = SinkError::new(kind, "adapter context");
      assert_eq!(error.kind(), kind);
      assert_eq!(error.message(), "adapter context");
      assert!(error.to_string().contains("adapter context"));
    }
  }
}
