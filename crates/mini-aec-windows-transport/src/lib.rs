//! Windows adapter for the private `MiniAEC` driver control interface.

use std::io;

use mini_aec_transport::{
  DiagnosticCounters, PcmFormat, PcmFrame, SessionConfig, SessionId, SessionState, SinkDiagnostics,
  SinkError, SinkErrorKind, VirtualMicrophoneSink, WriteReceipt, DIAGNOSTICS_SCHEMA_VERSION,
  FRAME_SAMPLES,
};

const DEVICE_PATH: &str = r"\\.\MiniAECTransport";
const PROTOCOL_MAGIC: u32 = 0x4345_414d;
const PROTOCOL_VERSION: u16 = 1;
const FRAME_BYTES: usize = FRAME_SAMPLES * size_of::<i16>();
const OPEN_REQUEST_SIZE: usize = 40;
const WRITE_HEADER_SIZE: usize = 44;
const WRITE_REQUEST_SIZE: usize = WRITE_HEADER_SIZE + FRAME_BYTES;
const CLOSE_REQUEST_SIZE: usize = 28;
const DIAGNOSTICS_SIZE: usize = 128;

const FILE_DEVICE_UNKNOWN: u32 = 0x22;
const FILE_READ_DATA: u32 = 0x0001;
const FILE_WRITE_DATA: u32 = 0x0002;
const METHOD_BUFFERED: u32 = 0;

const fn ctl_code(function: u32, access: u32) -> u32 {
  (FILE_DEVICE_UNKNOWN << 16) | (access << 14) | (function << 2) | METHOD_BUFFERED
}

const IOCTL_OPEN_SESSION: u32 = ctl_code(0x800, FILE_WRITE_DATA);
const IOCTL_WRITE_FRAME: u32 = ctl_code(0x801, FILE_WRITE_DATA);
const IOCTL_GET_DIAGNOSTICS: u32 = ctl_code(0x802, FILE_READ_DATA);
const IOCTL_CLOSE_SESSION: u32 = ctl_code(0x803, FILE_WRITE_DATA);

trait DeviceBackend: Send {
  fn control(&self, code: u32, input: &[u8], output: &mut [u8]) -> io::Result<usize>;
}

#[cfg(windows)]
mod platform {
  use std::ffi::c_void;
  use std::fs::{File, OpenOptions};
  use std::io;
  use std::os::windows::io::AsRawHandle;
  use std::ptr;

  use super::DeviceBackend;

  #[link(name = "kernel32")]
  unsafe extern "system" {
    fn DeviceIoControl(
      device: *mut c_void,
      control_code: u32,
      input: *const c_void,
      input_size: u32,
      output: *mut c_void,
      output_size: u32,
      bytes_returned: *mut u32,
      overlapped: *mut c_void,
    ) -> i32;
  }

  pub(super) struct FileDevice(File);

  impl FileDevice {
    pub(super) fn open(path: &str) -> io::Result<Self> {
      OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map(Self)
    }
  }

  impl DeviceBackend for FileDevice {
    fn control(&self, code: u32, input: &[u8], output: &mut [u8]) -> io::Result<usize> {
      let input_size = u32::try_from(input.len()).expect("MiniAEC requests fit in u32");
      let output_size = u32::try_from(output.len()).expect("MiniAEC responses fit in u32");
      let input_pointer = if input.is_empty() {
        ptr::null()
      } else {
        input.as_ptr().cast()
      };
      let output_pointer = if output.is_empty() {
        ptr::null_mut()
      } else {
        output.as_mut_ptr().cast()
      };
      let mut returned = 0_u32;
      // SAFETY: the file owns a live Windows handle; input and output point to buffers whose
      // lengths are passed exactly, and this synchronous call does not retain either pointer.
      let result = unsafe {
        DeviceIoControl(
          self.0.as_raw_handle(),
          code,
          input_pointer,
          input_size,
          output_pointer,
          output_size,
          &raw mut returned,
          ptr::null_mut(),
        )
      };
      if result == 0 {
        Err(io::Error::last_os_error())
      } else {
        Ok(returned as usize)
      }
    }
  }
}

/// `VirtualMicrophoneSink` backed by the `MiniAEC` driver's private control device.
pub struct WindowsVirtualMicrophoneSink {
  backend: Box<dyn DeviceBackend>,
  active_session: Option<SessionId>,
  last_sequence: Option<u64>,
}

impl WindowsVirtualMicrophoneSink {
  /// Opens the private `MiniAECTransport` control device.
  ///
  /// # Errors
  ///
  /// Returns a project-owned error if the driver is absent, access is denied, or another sender
  /// already owns the exclusive control handle.
  #[cfg(windows)]
  pub fn connect() -> Result<Self, SinkError> {
    let backend =
      platform::FileDevice::open(DEVICE_PATH).map_err(|error| map_io_error("open", &error))?;
    Ok(Self::with_backend(backend))
  }

  /// Reports that this adapter is Windows-only.
  ///
  /// # Errors
  ///
  /// Always returns `DriverUnavailable` on non-Windows targets.
  #[cfg(not(windows))]
  pub fn connect() -> Result<Self, SinkError> {
    Err(SinkError::new(
      SinkErrorKind::DriverUnavailable,
      "the MiniAEC driver transport is available only on Windows",
    ))
  }
}

impl WindowsVirtualMicrophoneSink {
  fn with_backend(backend: impl DeviceBackend + 'static) -> Self {
    Self {
      backend: Box::new(backend),
      active_session: None,
      last_sequence: None,
    }
  }

  fn invoke(
    &self,
    operation: &str,
    code: u32,
    input: &[u8],
    output: &mut [u8],
  ) -> Result<usize, SinkError> {
    self
      .backend
      .control(code, input, output)
      .map_err(|error| map_io_error(operation, &error))
  }
}

impl VirtualMicrophoneSink for WindowsVirtualMicrophoneSink {
  fn open_session(&mut self, config: SessionConfig) -> Result<(), SinkError> {
    if self.active_session.is_some() {
      return Err(SinkError::new(
        SinkErrorKind::InvalidState,
        "a MiniAEC sender session is already open on this adapter",
      ));
    }
    if config.format() != PcmFormat::MONO_48_KHZ_PCM16 {
      return Err(SinkError::new(
        SinkErrorKind::InvalidFormat,
        "the driver accepts only 48 kHz mono PCM16 in 10 ms frames",
      ));
    }

    let request = encode_open(config);
    self.invoke("open session", IOCTL_OPEN_SESSION, &request, &mut [])?;
    self.active_session = Some(config.session_id());
    self.last_sequence = None;
    Ok(())
  }

  fn write_frame(&mut self, frame: PcmFrame<'_>) -> Result<WriteReceipt, SinkError> {
    let session_id = self.active_session.ok_or_else(|| {
      SinkError::new(
        SinkErrorKind::InvalidState,
        "no MiniAEC sender session is open",
      )
    })?;
    let expected = self
      .last_sequence
      .map_or(0, |sequence| sequence.saturating_add(1));
    if frame.sequence() != expected {
      return Err(SinkError::new(
        SinkErrorKind::SequenceViolation,
        format!("expected frame {expected}, received {}", frame.sequence()),
      ));
    }

    let request = encode_frame(session_id, frame);
    self.invoke("write frame", IOCTL_WRITE_FRAME, &request, &mut [])?;
    self.last_sequence = Some(frame.sequence());
    Ok(WriteReceipt::accepted(frame.sequence()))
  }

  fn diagnostics(&self) -> Result<SinkDiagnostics, SinkError> {
    let mut response = [0_u8; DIAGNOSTICS_SIZE];
    let returned = self.invoke(
      "query diagnostics",
      IOCTL_GET_DIAGNOSTICS,
      &[],
      &mut response,
    )?;
    if returned != DIAGNOSTICS_SIZE {
      return Err(SinkError::new(
        SinkErrorKind::TransportFailure,
        format!("driver returned {returned} diagnostic bytes; expected {DIAGNOSTICS_SIZE}"),
      ));
    }
    decode_diagnostics(&response)
  }

  fn close_session(&mut self) -> Result<(), SinkError> {
    let Some(session_id) = self.active_session else {
      return Ok(());
    };
    let request = encode_close(session_id);
    self.invoke("close session", IOCTL_CLOSE_SESSION, &request, &mut [])?;
    self.active_session = None;
    self.last_sequence = None;
    Ok(())
  }
}

fn map_io_error(operation: &str, error: &io::Error) -> SinkError {
  let kind = match error.raw_os_error() {
    Some(2 | 3) => SinkErrorKind::DriverUnavailable,
    Some(5) => SinkErrorKind::AccessDenied,
    Some(32 | 170) => SinkErrorKind::Busy,
    Some(1306) => SinkErrorKind::VersionMismatch,
    Some(87) if operation == "write frame" => SinkErrorKind::RejectedWrite,
    Some(87 | 5023) => SinkErrorKind::InvalidState,
    _ => SinkErrorKind::TransportFailure,
  };
  SinkError::new(kind, format!("failed to {operation}: {error}"))
}

fn encode_open(config: SessionConfig) -> [u8; OPEN_REQUEST_SIZE] {
  let mut bytes = [0_u8; OPEN_REQUEST_SIZE];
  put_u32(&mut bytes, 0, PROTOCOL_MAGIC);
  put_u16(&mut bytes, 4, PROTOCOL_VERSION);
  put_u16(
    &mut bytes,
    6,
    u16::try_from(OPEN_REQUEST_SIZE).expect("open request size fits u16"),
  );
  put_u32(
    &mut bytes,
    8,
    u32::try_from(OPEN_REQUEST_SIZE).expect("open request size fits u32"),
  );
  bytes[12..28].copy_from_slice(&config.session_id().get().to_le_bytes());
  put_u32(&mut bytes, 28, config.format().sample_rate_hz());
  put_u16(&mut bytes, 32, config.format().channels());
  put_u16(&mut bytes, 34, config.format().bits_per_sample());
  put_u16(&mut bytes, 36, config.format().frame_samples_per_channel());
  bytes
}

fn encode_frame(session_id: SessionId, frame: PcmFrame<'_>) -> [u8; WRITE_REQUEST_SIZE] {
  let mut bytes = [0_u8; WRITE_REQUEST_SIZE];
  put_u32(&mut bytes, 0, PROTOCOL_MAGIC);
  put_u16(&mut bytes, 4, PROTOCOL_VERSION);
  put_u16(
    &mut bytes,
    6,
    u16::try_from(WRITE_HEADER_SIZE).expect("frame header size fits u16"),
  );
  put_u32(
    &mut bytes,
    8,
    u32::try_from(WRITE_REQUEST_SIZE).expect("frame request size fits u32"),
  );
  bytes[12..28].copy_from_slice(&session_id.get().to_le_bytes());
  put_u64(&mut bytes, 28, frame.sequence());
  put_u32(
    &mut bytes,
    36,
    u32::try_from(FRAME_BYTES).expect("frame payload size fits u32"),
  );
  let (sample_bytes, remainder) =
    bytes[WRITE_HEADER_SIZE..].as_chunks_mut::<{ size_of::<i16>() }>();
  debug_assert!(remainder.is_empty());
  for (destination, sample) in sample_bytes.iter_mut().zip(frame.samples()) {
    destination.copy_from_slice(&sample.to_le_bytes());
  }
  bytes
}

fn encode_close(session_id: SessionId) -> [u8; CLOSE_REQUEST_SIZE] {
  let mut bytes = [0_u8; CLOSE_REQUEST_SIZE];
  put_u32(&mut bytes, 0, PROTOCOL_MAGIC);
  put_u16(&mut bytes, 4, PROTOCOL_VERSION);
  put_u16(
    &mut bytes,
    6,
    u16::try_from(CLOSE_REQUEST_SIZE).expect("close request size fits u16"),
  );
  put_u32(
    &mut bytes,
    8,
    u32::try_from(CLOSE_REQUEST_SIZE).expect("close request size fits u32"),
  );
  bytes[12..28].copy_from_slice(&session_id.get().to_le_bytes());
  bytes
}

fn decode_diagnostics(bytes: &[u8; DIAGNOSTICS_SIZE]) -> Result<SinkDiagnostics, SinkError> {
  if get_u32(bytes, 0) != PROTOCOL_MAGIC {
    return Err(SinkError::new(
      SinkErrorKind::TransportFailure,
      "driver returned an invalid diagnostics magic",
    ));
  }
  if get_u16(bytes, 4) != PROTOCOL_VERSION || get_u16(bytes, 6) != DIAGNOSTICS_SCHEMA_VERSION {
    return Err(SinkError::new(
      SinkErrorKind::VersionMismatch,
      "driver diagnostics version does not match the MiniAEC adapter",
    ));
  }
  if get_u32(bytes, 8) as usize != DIAGNOSTICS_SIZE {
    return Err(SinkError::new(
      SinkErrorKind::TransportFailure,
      "driver returned an invalid diagnostics size",
    ));
  }
  let state = match get_u32(bytes, 12) {
    0 => SessionState::Closed,
    1 => SessionState::Open,
    value => {
      return Err(SinkError::new(
        SinkErrorKind::TransportFailure,
        format!("driver returned unknown session state {value}"),
      ));
    }
  };
  let session_value = u128::from_le_bytes(
    bytes[24..40]
      .try_into()
      .expect("fixed diagnostics session field"),
  );
  let active_session = if state == SessionState::Open {
    Some(SessionId::new(session_value).ok_or_else(|| {
      SinkError::new(
        SinkErrorKind::TransportFailure,
        "open driver session has the reserved zero identity",
      )
    })?)
  } else {
    None
  };
  let last_accepted_sequence = match get_u32(bytes, 48) {
    0 => None,
    1 => Some(get_u64(bytes, 40)),
    value => {
      return Err(SinkError::new(
        SinkErrorKind::TransportFailure,
        format!("driver returned invalid sequence-presence flag {value}"),
      ));
    }
  };

  Ok(SinkDiagnostics {
    schema_version: DIAGNOSTICS_SCHEMA_VERSION,
    state,
    active_session,
    last_accepted_sequence,
    current_depth: get_u32(bytes, 16),
    high_water_mark: get_u32(bytes, 20),
    counters: DiagnosticCounters {
      session_opens: get_u64(bytes, 56),
      session_closes: get_u64(bytes, 64),
      session_resets: get_u64(bytes, 72),
      accepted_frames: get_u64(bytes, 80),
      rejected_writes: get_u64(bytes, 88),
      underruns: get_u64(bytes, 96),
      overflows: get_u64(bytes, 104),
      discarded_frames: get_u64(bytes, 112),
      driver_restarts: get_u64(bytes, 120),
    },
  })
}

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
  bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
  bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
  bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn get_u16(bytes: &[u8], offset: usize) -> u16 {
  u16::from_le_bytes(
    bytes[offset..offset + 2]
      .try_into()
      .expect("fixed protocol field"),
  )
}

fn get_u32(bytes: &[u8], offset: usize) -> u32 {
  u32::from_le_bytes(
    bytes[offset..offset + 4]
      .try_into()
      .expect("fixed protocol field"),
  )
}

fn get_u64(bytes: &[u8], offset: usize) -> u64 {
  u64::from_le_bytes(
    bytes[offset..offset + 8]
      .try_into()
      .expect("fixed protocol field"),
  )
}

#[cfg(test)]
mod tests {
  use std::collections::{HashMap, VecDeque};
  use std::io;
  use std::sync::{Arc, Mutex};

  use mini_aec_transport::{
    PcmFrame, SessionConfig, SessionId, SinkErrorKind, VirtualMicrophoneSink, FRAME_SAMPLES,
  };

  use super::{
    decode_diagnostics, encode_close, encode_frame, encode_open, get_u16, get_u32, get_u64,
    DeviceBackend, WindowsVirtualMicrophoneSink, CLOSE_REQUEST_SIZE, DIAGNOSTICS_SIZE,
    IOCTL_CLOSE_SESSION, IOCTL_GET_DIAGNOSTICS, IOCTL_OPEN_SESSION, IOCTL_WRITE_FRAME,
    OPEN_REQUEST_SIZE, PROTOCOL_MAGIC, PROTOCOL_VERSION, WRITE_HEADER_SIZE, WRITE_REQUEST_SIZE,
  };

  type CallLog = Arc<Mutex<Vec<(u32, Vec<u8>)>>>;

  #[derive(Debug, Eq, PartialEq)]
  enum ModelError {
    Malformed,
    Version,
    InvalidSession,
    Sequence,
  }

  #[derive(Default)]
  struct DriverModel {
    session: Option<[u8; 16]>,
    next_sequence: u64,
    ring: VecDeque<(u64, [u8; 960])>,
    discarded_frames: u64,
  }

  impl DriverModel {
    fn open(&mut self, request: &[u8]) -> Result<(), ModelError> {
      if request.len() != OPEN_REQUEST_SIZE || get_u32(request, 0) != PROTOCOL_MAGIC {
        return Err(ModelError::Malformed);
      }
      if get_u16(request, 4) != PROTOCOL_VERSION {
        return Err(ModelError::Version);
      }
      let session: [u8; 16] = request[12..28]
        .try_into()
        .map_err(|_| ModelError::Malformed)?;
      if session == [0; 16] {
        return Err(ModelError::InvalidSession);
      }
      self.session = Some(session);
      self.next_sequence = 0;
      self.ring.clear();
      Ok(())
    }

    fn write(&mut self, request: &[u8]) -> Result<(), ModelError> {
      if request.len() != WRITE_REQUEST_SIZE
        || get_u32(request, 0) != PROTOCOL_MAGIC
        || get_u16(request, 6) as usize != WRITE_HEADER_SIZE
        || get_u32(request, 8) as usize != WRITE_REQUEST_SIZE
        || get_u32(request, 36) as usize != 960
      {
        return Err(ModelError::Malformed);
      }
      if get_u16(request, 4) != PROTOCOL_VERSION {
        return Err(ModelError::Version);
      }
      if self.session.as_ref().map(<[u8; 16]>::as_slice) != Some(&request[12..28]) {
        return Err(ModelError::InvalidSession);
      }
      let sequence = get_u64(request, 28);
      if sequence != self.next_sequence {
        return Err(ModelError::Sequence);
      }
      if self.ring.len() == 10 {
        assert!(self.ring.pop_front().is_some());
        self.discarded_frames += 1;
      }
      let pcm = request[WRITE_HEADER_SIZE..]
        .try_into()
        .map_err(|_| ModelError::Malformed)?;
      self.ring.push_back((sequence, pcm));
      self.next_sequence = self
        .next_sequence
        .checked_add(1)
        .ok_or(ModelError::Sequence)?;
      Ok(())
    }

    fn cleanup(&mut self) {
      self.session = None;
      self.next_sequence = 0;
      self.ring.clear();
    }
  }

  struct FakeDevice {
    calls: CallLog,
    errors: HashMap<u32, i32>,
    diagnostics: [u8; DIAGNOSTICS_SIZE],
  }

  impl Default for FakeDevice {
    fn default() -> Self {
      Self {
        calls: Arc::default(),
        errors: HashMap::new(),
        diagnostics: [0; DIAGNOSTICS_SIZE],
      }
    }
  }

  impl DeviceBackend for FakeDevice {
    fn control(&self, code: u32, input: &[u8], output: &mut [u8]) -> io::Result<usize> {
      self
        .calls
        .lock()
        .expect("call log lock")
        .push((code, input.to_vec()));
      if let Some(code) = self.errors.get(&code) {
        return Err(io::Error::from_raw_os_error(*code));
      }
      if code == IOCTL_GET_DIAGNOSTICS {
        output.copy_from_slice(&self.diagnostics);
        Ok(output.len())
      } else {
        Ok(0)
      }
    }
  }

  fn session(value: u128) -> SessionId {
    SessionId::new(value).expect("test session is nonzero")
  }

  fn valid_diagnostics(session_id: SessionId) -> [u8; DIAGNOSTICS_SIZE] {
    let mut bytes = [0_u8; DIAGNOSTICS_SIZE];
    super::put_u32(&mut bytes, 0, PROTOCOL_MAGIC);
    super::put_u16(&mut bytes, 4, PROTOCOL_VERSION);
    super::put_u16(&mut bytes, 6, 2);
    super::put_u32(
      &mut bytes,
      8,
      u32::try_from(DIAGNOSTICS_SIZE).expect("diagnostics size fits u32"),
    );
    super::put_u32(&mut bytes, 12, 1);
    super::put_u32(&mut bytes, 16, 3);
    super::put_u32(&mut bytes, 20, 7);
    bytes[24..40].copy_from_slice(&session_id.get().to_le_bytes());
    super::put_u64(&mut bytes, 40, 9);
    super::put_u32(&mut bytes, 48, 1);
    for (offset, value) in (56..=120).step_by(8).zip(1_u64..) {
      super::put_u64(&mut bytes, offset, value);
    }
    bytes
  }

  #[test]
  fn protocol_layout_matches_the_driver_header() {
    let id = session(0x1122_3344_5566_7788_99aa_bbcc_ddee_ff00);
    let open = encode_open(SessionConfig::new(id));
    assert_eq!(open.len(), OPEN_REQUEST_SIZE);
    assert_eq!(get_u32(&open, 0), PROTOCOL_MAGIC);
    assert_eq!(get_u16(&open, 4), PROTOCOL_VERSION);
    assert_eq!(get_u32(&open, 28), 48_000);

    let samples = std::array::from_fn(|index| i16::try_from(index).expect("sample fits"));
    let frame = encode_frame(id, PcmFrame::new(3, &samples));
    assert_eq!(frame.len(), WRITE_REQUEST_SIZE);
    assert_eq!(get_u16(&frame, 6) as usize, WRITE_HEADER_SIZE);
    assert_eq!(get_u64(&frame, 28), 3);
    assert_eq!(get_u32(&frame, 36), 960);
    assert_eq!(
      &frame[WRITE_HEADER_SIZE..WRITE_HEADER_SIZE + 2],
      &0_i16.to_le_bytes()
    );
    assert_eq!(
      &frame[WRITE_HEADER_SIZE + 2..WRITE_HEADER_SIZE + 4],
      &1_i16.to_le_bytes()
    );

    let close = encode_close(id);
    assert_eq!(close.len(), CLOSE_REQUEST_SIZE);
    assert_eq!(&close[12..28], &id.get().to_le_bytes());
  }

  #[test]
  fn adapter_opens_writes_queries_and_closes_one_session() {
    let id = session(7);
    let backend = FakeDevice {
      diagnostics: valid_diagnostics(id),
      ..FakeDevice::default()
    };
    let calls = Arc::clone(&backend.calls);
    let mut sink = WindowsVirtualMicrophoneSink::with_backend(backend);
    sink
      .open_session(SessionConfig::new(id))
      .expect("session opens");
    sink
      .write_frame(PcmFrame::new(0, &[0; FRAME_SAMPLES]))
      .expect("frame writes");
    let diagnostics = sink.diagnostics().expect("diagnostics decode");
    assert_eq!(diagnostics.active_session, Some(id));
    assert_eq!(diagnostics.last_accepted_sequence, Some(9));
    assert_eq!(diagnostics.current_depth, 3);
    assert_eq!(diagnostics.high_water_mark, 7);
    assert_eq!(diagnostics.counters.discarded_frames, 8);
    sink.close_session().expect("session closes");
    let codes: Vec<_> = calls
      .lock()
      .expect("call log lock")
      .iter()
      .map(|call| call.0)
      .collect();
    assert_eq!(
      codes,
      [
        IOCTL_OPEN_SESSION,
        IOCTL_WRITE_FRAME,
        IOCTL_GET_DIAGNOSTICS,
        IOCTL_CLOSE_SESSION
      ]
    );
  }

  #[test]
  fn adapter_enforces_sequence_before_crossing_the_driver_boundary() {
    let id = session(8);
    let backend = FakeDevice::default();
    let calls = Arc::clone(&backend.calls);
    let mut sink = WindowsVirtualMicrophoneSink::with_backend(backend);
    sink
      .open_session(SessionConfig::new(id))
      .expect("session opens");
    let error = sink
      .write_frame(PcmFrame::new(1, &[0; FRAME_SAMPLES]))
      .expect_err("skipped frame fails");
    assert_eq!(error.kind(), SinkErrorKind::SequenceViolation);
    assert_eq!(calls.lock().expect("call log lock").len(), 1);
  }

  #[test]
  fn win32_failures_map_to_project_owned_errors() {
    for (os_error, expected) in [
      (2, SinkErrorKind::DriverUnavailable),
      (5, SinkErrorKind::AccessDenied),
      (32, SinkErrorKind::Busy),
      (170, SinkErrorKind::Busy),
      (1306, SinkErrorKind::VersionMismatch),
    ] {
      let mut errors = HashMap::new();
      errors.insert(IOCTL_OPEN_SESSION, os_error);
      let mut sink = WindowsVirtualMicrophoneSink::with_backend(FakeDevice {
        errors,
        ..FakeDevice::default()
      });
      let error = sink
        .open_session(SessionConfig::new(session(10)))
        .expect_err("configured failure");
      assert_eq!(error.kind(), expected);
    }
  }

  #[test]
  fn diagnostics_rejects_malformed_and_mismatched_responses() {
    let id = session(11);
    let mut bytes = valid_diagnostics(id);
    bytes[0] = 0;
    assert_eq!(
      decode_diagnostics(&bytes).expect_err("bad magic").kind(),
      SinkErrorKind::TransportFailure
    );
    let mut bytes = valid_diagnostics(id);
    super::put_u16(&mut bytes, 6, 99);
    assert_eq!(
      decode_diagnostics(&bytes).expect_err("bad version").kind(),
      SinkErrorKind::VersionMismatch
    );
  }

  #[test]
  fn driver_boundary_model_validates_requests_wraparound_cleanup_and_isolation() {
    let first_session = session(21);
    let second_session = session(22);
    let mut model = DriverModel::default();
    let open = encode_open(SessionConfig::new(first_session));
    assert_eq!(
      model.open(&open[..OPEN_REQUEST_SIZE - 1]),
      Err(ModelError::Malformed)
    );
    let mut wrong_version = open;
    super::put_u16(&mut wrong_version, 4, PROTOCOL_VERSION + 1);
    assert_eq!(model.open(&wrong_version), Err(ModelError::Version));
    model.open(&open).expect("first model session opens");
    assert!(model.ring.pop_front().is_none());

    for sequence in 0_u64..12 {
      let samples = [i16::try_from(sequence).expect("small test sequence"); FRAME_SAMPLES];
      let request = encode_frame(first_session, PcmFrame::new(sequence, &samples));
      model.write(&request).expect("ordered frame is accepted");
    }
    assert_eq!(model.ring.len(), 10);
    assert_eq!(model.ring.front().map(|frame| frame.0), Some(2));
    assert_eq!(model.ring.back().map(|frame| frame.0), Some(11));
    assert_eq!(model.discarded_frames, 2);

    let stale = encode_frame(first_session, PcmFrame::new(12, &[0; FRAME_SAMPLES]));
    model.cleanup();
    assert!(model.ring.is_empty());
    assert_eq!(model.write(&stale), Err(ModelError::InvalidSession));
    model
      .open(&encode_open(SessionConfig::new(second_session)))
      .expect("new model session opens");
    assert!(model.ring.is_empty());
    let first_new_frame = encode_frame(second_session, PcmFrame::new(0, &[0; FRAME_SAMPLES]));
    model
      .write(&first_new_frame)
      .expect("new session restarts sequence at zero");
    assert_eq!(model.ring.front().map(|frame| frame.0), Some(0));
  }
}
