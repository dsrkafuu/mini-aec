//! Exact-ID VB-CABLE preflight and event-driven shared-mode WASAPI rendering.

use std::time::{Duration, Instant};

use mini_aec_output::{
  resolve_pair, AudioOutput, DiagnosticCounters, EndpointRole, OutputDiagnostics,
  OutputEndpointDescriptor, OutputError, OutputErrorKind, OutputFormat, OutputPair,
  OutputSampleType, PcmFrame, SessionConfig, SessionState, WriteReceipt, FRAME_SAMPLES,
};
use wasapi::{
  initialize_mta, AudioClient, AudioClock, AudioRenderClient, Device, DeviceEnumerator,
  DeviceState, Direction, Handle, SampleType, StreamMode, WasapiError, WaveFormat,
};

const OUTPUT_WAIT: Duration = Duration::from_millis(100);

/// Resolves the explicitly configured VB-CABLE pair without a default-device fallback.
///
/// # Errors
/// Returns an actionable prerequisite or endpoint error.
pub fn resolve_vb_cable_pair(
  playback_id: &str,
  recording_id: &str,
) -> Result<OutputPair, OutputError> {
  initialize_mta().ok().map_err(|error| {
    OutputError::new(
      OutputErrorKind::RenderFailure,
      format!("failed to initialize COM for VB-CABLE preflight: {error}"),
    )
  })?;
  let enumerator =
    DeviceEnumerator::new().map_err(|error| output_error("create the device enumerator", error))?;
  let inventory = [
    describe_exact(
      &enumerator,
      Direction::Render,
      EndpointRole::Playback,
      playback_id,
    )?,
    describe_exact(
      &enumerator,
      Direction::Capture,
      EndpointRole::Recording,
      recording_id,
    )?,
  ];
  resolve_pair(&inventory, playback_id, recording_id)
}

/// Enumerates active VB-CABLE playback/recording pairs using exact endpoint identities.
///
/// The returned inventory is deliberately limited to pairs whose endpoint metadata contains the
/// corroborating `VB-Audio` adapter-family marker. Friendly names are retained for presentation,
/// but are never used to select or pair endpoints.
///
/// # Errors
/// Returns an actionable enumeration or endpoint-metadata error.
pub fn enumerate_vb_cable_pairs() -> Result<Vec<OutputPair>, OutputError> {
  initialize_mta().ok().map_err(|error| {
    OutputError::new(
      OutputErrorKind::RenderFailure,
      format!("failed to initialize COM for VB-CABLE inventory: {error}"),
    )
  })?;
  let enumerator =
    DeviceEnumerator::new().map_err(|error| output_error("create the device enumerator", error))?;
  let playback =
    enumerate_output_endpoints(&enumerator, Direction::Render, EndpointRole::Playback)?;
  let recording =
    enumerate_output_endpoints(&enumerator, Direction::Capture, EndpointRole::Recording)?;
  let mut pairs = Vec::new();
  for playback_endpoint in playback {
    if !is_vb_audio_family(&playback_endpoint.device_family) {
      continue;
    }
    for recording_endpoint in &recording {
      if !is_vb_audio_family(&recording_endpoint.device_family)
        || !playback_endpoint
          .device_family
          .eq_ignore_ascii_case(&recording_endpoint.device_family)
      {
        continue;
      }
      let inventory = [playback_endpoint.clone(), recording_endpoint.clone()];
      if let Ok(pair) = resolve_pair(
        &inventory,
        &playback_endpoint.endpoint_id,
        &recording_endpoint.endpoint_id,
      ) {
        pairs.push(pair);
      }
    }
  }
  Ok(pairs)
}

fn enumerate_output_endpoints(
  enumerator: &DeviceEnumerator,
  direction: Direction,
  role: EndpointRole,
) -> Result<Vec<OutputEndpointDescriptor>, OutputError> {
  let collection = enumerator
    .get_device_collection(&direction)
    .map_err(|error| output_error("enumerate VB-CABLE endpoints", error))?;
  collection
    .into_iter()
    .map(|device| {
      let device = device.map_err(|error| output_error("access a VB-CABLE endpoint", error))?;
      describe_device(&device, role)
    })
    .collect()
}

fn is_vb_audio_family(family: &str) -> bool {
  family.to_ascii_lowercase().contains("vb-audio")
}

fn describe_exact(
  enumerator: &DeviceEnumerator,
  direction: Direction,
  role: EndpointRole,
  endpoint_id: &str,
) -> Result<OutputEndpointDescriptor, OutputError> {
  let device = find_active_device(enumerator, direction, endpoint_id)?;
  describe_device(&device, role)
}

fn find_active_device(
  enumerator: &DeviceEnumerator,
  direction: Direction,
  endpoint_id: &str,
) -> Result<Device, OutputError> {
  if let Some(device) = find_in_direction(enumerator, direction, endpoint_id)? {
    return Ok(device);
  }
  let opposite = match direction {
    Direction::Render => Direction::Capture,
    Direction::Capture => Direction::Render,
  };
  if find_in_direction(enumerator, opposite, endpoint_id)?.is_some() {
    return Err(OutputError::new(
      OutputErrorKind::InvalidEndpoint,
      "configured endpoint has the wrong Windows data-flow role",
    ));
  }
  Err(OutputError::new(
    OutputErrorKind::PrerequisiteMissing,
    format!("configured endpoint {endpoint_id:?} is unavailable or inactive"),
  ))
}

fn find_in_direction(
  enumerator: &DeviceEnumerator,
  direction: Direction,
  endpoint_id: &str,
) -> Result<Option<Device>, OutputError> {
  let collection = enumerator
    .get_device_collection(&direction)
    .map_err(|error| output_error("enumerate active audio endpoints", error))?;
  for device in &collection {
    let device = device.map_err(|error| output_error("access an audio endpoint", error))?;
    let candidate_id = device
      .get_id()
      .map_err(|error| output_error("read endpoint identity", error))?;
    if candidate_id == endpoint_id {
      return Ok(Some(device));
    }
  }
  Ok(None)
}

fn describe_device(
  device: &Device,
  role: EndpointRole,
) -> Result<OutputEndpointDescriptor, OutputError> {
  Ok(OutputEndpointDescriptor {
    endpoint_id: device
      .get_id()
      .map_err(|error| output_error("read endpoint identity", error))?,
    friendly_name: device
      .get_friendlyname()
      .map_err(|error| output_error("read endpoint display name", error))?,
    role,
    active: device
      .get_state()
      .map_err(|error| output_error("read endpoint state", error))?
      == DeviceState::Active,
    device_family: device
      .get_interface_friendlyname()
      .map_err(|error| output_error("read endpoint adapter metadata", error))?,
  })
}

pub struct WindowsVbCableOutput {
  pair: OutputPair,
  stream: Option<RenderStream>,
  diagnostics: OutputDiagnostics,
  next_sequence: u64,
}

impl WindowsVbCableOutput {
  #[must_use]
  pub fn new(pair: OutputPair) -> Self {
    Self {
      pair,
      stream: None,
      diagnostics: OutputDiagnostics::default(),
      next_sequence: 0,
    }
  }
}

impl AudioOutput for WindowsVbCableOutput {
  fn open_session(&mut self, config: SessionConfig) -> Result<(), OutputError> {
    if self.stream.is_some() {
      return Err(OutputError::new(
        OutputErrorKind::InvalidState,
        "output session is already open",
      ));
    }
    let stream = RenderStream::open(&self.pair.playback.endpoint_id)?;
    self.diagnostics = OutputDiagnostics {
      state: SessionState::Open,
      active_session: Some(config.session_id()),
      negotiated_format: Some(stream.converter.format),
      output_clock_frequency: Some(stream.clock_frequency),
      counters: DiagnosticCounters {
        session_opens: 1,
        ..DiagnosticCounters::default()
      },
      ..OutputDiagnostics::default()
    };
    self.next_sequence = 0;
    self.stream = Some(stream);
    Ok(())
  }

  fn write_frame(&mut self, frame: PcmFrame<'_>) -> Result<WriteReceipt, OutputError> {
    if frame.sequence() != self.next_sequence {
      self.diagnostics.counters.rejected_writes += 1;
      return Err(OutputError::new(
        OutputErrorKind::SequenceViolation,
        "output frame sequence is not monotonic",
      ));
    }
    let stream = self
      .stream
      .as_mut()
      .ok_or_else(|| OutputError::new(OutputErrorKind::InvalidState, "output session is closed"))?;
    let requires_conversion = stream.converter.requires_conversion;
    let progress = match stream.write(frame.samples()) {
      Ok(progress) => progress,
      Err(error) => {
        self.diagnostics.counters.output_failures += 1;
        if error.kind() == OutputErrorKind::DeviceInvalidated {
          self.diagnostics.counters.endpoint_invalidations += 1;
        }
        return Err(error);
      }
    };
    if requires_conversion {
      self.diagnostics.counters.converted_frames += 1;
    }
    if progress.underrun && self.diagnostics.counters.accepted_frames > 0 {
      self.diagnostics.counters.underruns += 1;
    }
    self.next_sequence = self.next_sequence.saturating_add(1);
    self.diagnostics.last_accepted_sequence = Some(frame.sequence());
    self.diagnostics.current_padding_frames = progress.padding;
    self.diagnostics.current_depth = progress.padding;
    self.diagnostics.high_water_mark = self.diagnostics.high_water_mark.max(progress.padding);
    self.diagnostics.rendered_frames = self
      .diagnostics
      .rendered_frames
      .saturating_add(u64::from(stream.converter.output_frames));
    self.diagnostics.output_clock_position = Some(progress.clock_position);
    self.diagnostics.output_clock_qpc_position = Some(progress.clock_qpc_position);
    self.diagnostics.counters.accepted_frames += 1;
    Ok(WriteReceipt::accepted(frame.sequence()))
  }

  fn diagnostics(&self) -> Result<OutputDiagnostics, OutputError> {
    Ok(self.diagnostics.clone())
  }

  fn close_session(&mut self) -> Result<(), OutputError> {
    if let Some(mut stream) = self.stream.take() {
      stream.stop()?;
      self.diagnostics.counters.session_closes += 1;
    }
    self.next_sequence = 0;
    self.diagnostics.state = SessionState::Closed;
    self.diagnostics.active_session = None;
    self.diagnostics.current_depth = 0;
    self.diagnostics.current_padding_frames = 0;
    Ok(())
  }
}

impl Drop for WindowsVbCableOutput {
  fn drop(&mut self) {
    let _ = self.close_session();
  }
}

struct RenderStream {
  audio_client: AudioClient,
  render_client: AudioRenderClient,
  audio_clock: AudioClock,
  clock_frequency: u64,
  buffer_frames: u32,
  event: Handle,
  converter: FrameConverter,
  underrun_grace: Duration,
  last_write_at: Option<Instant>,
  started: bool,
  stopped: bool,
}

impl RenderStream {
  fn open(endpoint_id: &str) -> Result<Self, OutputError> {
    initialize_mta().ok().map_err(|error| {
      OutputError::new(
        OutputErrorKind::RenderFailure,
        format!("failed to initialize COM in the output worker: {error}"),
      )
    })?;
    let enumerator = DeviceEnumerator::new()
      .map_err(|error| output_error("create the output device enumerator", error))?;
    let device = find_active_device(&enumerator, Direction::Render, endpoint_id)?;
    let mut audio_client = device
      .get_iaudioclient()
      .map_err(|error| output_error("create the WASAPI render client", error))?;
    let mix = audio_client
      .get_mixformat()
      .map_err(|error| output_error("read the shared-mode mix format", error))?;
    let converter = FrameConverter::new(&mix)?;
    let (default_period, _) = audio_client
      .get_device_period()
      .map_err(|error| output_error("read the output device period", error))?;
    audio_client
      .initialize_client(
        &mix,
        &Direction::Render,
        &StreamMode::EventsShared {
          autoconvert: false,
          buffer_duration_hns: default_period,
        },
      )
      .map_err(|error| output_error("initialize the shared-mode render stream", error))?;
    let event = audio_client
      .set_get_eventhandle()
      .map_err(|error| output_error("create the WASAPI render event", error))?;
    let render_client = audio_client
      .get_audiorenderclient()
      .map_err(|error| output_error("create the WASAPI render service", error))?;
    let audio_clock = audio_client
      .get_audioclock()
      .map_err(|error| output_error("create the WASAPI output clock", error))?;
    let clock_frequency = audio_clock
      .get_frequency()
      .map_err(|error| output_error("read the WASAPI output clock frequency", error))?;
    let buffer_frames = audio_client
      .get_buffer_size()
      .map_err(|error| output_error("read the WASAPI output buffer capacity", error))?;
    let prefill_frames = buffer_frames.min(converter.output_frames.saturating_mul(2));
    let underrun_grace = Duration::from_nanos(
      u64::from(prefill_frames)
        .saturating_mul(1_000_000_000)
        .checked_div(u64::from(converter.format.sample_rate_hz))
        .unwrap_or_default(),
    );
    Ok(Self {
      audio_client,
      render_client,
      audio_clock,
      clock_frequency,
      buffer_frames,
      event,
      converter,
      underrun_grace,
      last_write_at: None,
      started: false,
      stopped: false,
    })
  }

  fn write(&mut self, samples: &[i16; FRAME_SAMPLES]) -> Result<RenderProgress, OutputError> {
    self.converter.convert(samples);
    let frames = self.converter.output_frames;
    let available_before_wait = self
      .audio_client
      .get_available_space_in_frames()
      .map_err(|error| output_error("query output buffer space", error))?;
    let write_started_at = Instant::now();
    let underrun = self.started
      && self.buffer_frames == available_before_wait
      && self.last_write_at.is_some_and(|last_write| {
        write_started_at.duration_since(last_write) > self.underrun_grace
      });
    let deadline = write_started_at + OUTPUT_WAIT;
    let mut available = available_before_wait;
    while available < frames {
      let remaining = deadline.saturating_duration_since(Instant::now());
      if remaining.is_zero() {
        return Err(OutputError::new(
          OutputErrorKind::RejectedWrite,
          format!(
            "timed out with {available} WASAPI frames available; one complete converted frame requires {frames}"
          ),
        ));
      }
      let timeout_ms = u32::try_from(remaining.as_millis().max(1)).unwrap_or(u32::MAX);
      match self.event.wait_for_event(timeout_ms) {
        Ok(()) => {}
        Err(WasapiError::EventTimeout) => {
          return Err(OutputError::new(
            OutputErrorKind::RejectedWrite,
            "timed out waiting for bounded WASAPI output space",
          ));
        }
        Err(error) => return Err(output_error("wait for the WASAPI render event", error)),
      }
      available = self
        .audio_client
        .get_available_space_in_frames()
        .map_err(|error| output_error("query output buffer space after wake", error))?;
    }
    self
      .render_client
      .write_to_device(frames as usize, &self.converter.bytes, None)
      .map_err(|error| output_error("render one complete frame", error))?;
    let padding = self
      .audio_client
      .get_current_padding()
      .map_err(|error| output_error("read output padding", error))?;
    let prefill_target = self.buffer_frames.min(frames.saturating_mul(2));
    if !self.started && padding >= prefill_target {
      self
        .audio_client
        .start_stream()
        .map_err(|error| output_error("start the prefilled WASAPI render stream", error))?;
      self.started = true;
    }
    self.last_write_at = Some(write_started_at);
    let (clock_position, clock_qpc_position) = if self.started {
      self
        .audio_clock
        .get_position()
        .map_err(|error| output_error("read the WASAPI output clock position", error))?
    } else {
      (0, 0)
    };
    Ok(RenderProgress {
      padding,
      underrun,
      clock_position,
      clock_qpc_position,
    })
  }

  fn stop(&mut self) -> Result<(), OutputError> {
    if self.stopped {
      return Ok(());
    }
    if self.started {
      self
        .audio_client
        .stop_stream()
        .map_err(|error| output_error("stop the WASAPI render stream", error))?;
    }
    self
      .audio_client
      .reset_stream()
      .map_err(|error| output_error("reset the WASAPI render stream", error))?;
    self.converter.clear();
    self.stopped = true;
    Ok(())
  }
}

struct RenderProgress {
  padding: u32,
  underrun: bool,
  clock_position: u64,
  clock_qpc_position: u64,
}

struct FrameConverter {
  format: OutputFormat,
  output_frames: u32,
  bytes: Vec<u8>,
  requires_conversion: bool,
}

impl FrameConverter {
  fn new(format: &WaveFormat) -> Result<Self, OutputError> {
    let sample_rate_hz = format.get_samplespersec();
    let channels = format.get_nchannels();
    let bits_per_sample = format.get_bitspersample();
    let sample_type = match format
      .get_subformat()
      .map_err(|error| output_error("read output sample type", error))?
    {
      SampleType::Float => OutputSampleType::Float,
      SampleType::Int => OutputSampleType::Integer,
    };
    if sample_rate_hz == 0 || !sample_rate_hz.is_multiple_of(100) || channels == 0 || channels > 8 {
      return Err(OutputError::new(
        OutputErrorKind::InvalidFormat,
        format!("unsupported output mix format: {sample_rate_hz} Hz, {channels} channels"),
      ));
    }
    match (sample_type, bits_per_sample) {
      (OutputSampleType::Float, 32) | (OutputSampleType::Integer, 16 | 24 | 32) => {}
      _ => {
        return Err(OutputError::new(
          OutputErrorKind::InvalidFormat,
          format!(
            "unsupported output sample representation: {sample_type:?} {bits_per_sample}-bit"
          ),
        ));
      }
    }
    let output_frames = sample_rate_hz / 100;
    let bytes_per_sample = usize::from(bits_per_sample / 8);
    let bytes = vec![0; output_frames as usize * usize::from(channels) * bytes_per_sample];
    Ok(Self {
      format: OutputFormat {
        sample_rate_hz,
        channels,
        bits_per_sample,
        sample_type,
      },
      output_frames,
      bytes,
      requires_conversion: sample_rate_hz != 48_000
        || channels != 1
        || bits_per_sample != 16
        || sample_type != OutputSampleType::Integer,
    })
  }

  fn convert(&mut self, input: &[i16; FRAME_SAMPLES]) {
    let frames = self.output_frames;
    let channels = usize::from(self.format.channels);
    let bytes_per_sample = usize::from(self.format.bits_per_sample / 8);
    for output_index in 0..frames {
      let numerator =
        u64::from(output_index) * u64::try_from(FRAME_SAMPLES).expect("fixed frame size fits u64");
      let left =
        usize::try_from(numerator / u64::from(frames)).expect("resampled source index fits usize");
      let remainder =
        u32::try_from(numerator % u64::from(frames)).expect("resampling remainder fits u32");
      let right = (left + 1).min(FRAME_SAMPLES - 1);
      let left_sample = f64::from(input[left]);
      let sample = left_sample
        + (f64::from(input[right]) - left_sample) * f64::from(remainder) / f64::from(frames);
      for channel in 0..channels {
        let output_index = usize::try_from(output_index).expect("output frame index fits usize");
        let offset = (output_index * channels + channel) * bytes_per_sample;
        encode_sample(
          sample,
          self.format,
          &mut self.bytes[offset..offset + bytes_per_sample],
        );
      }
    }
  }

  fn clear(&mut self) {
    self.bytes.fill(0);
  }
}

#[allow(
  clippy::cast_possible_truncation,
  reason = "values are explicitly normalized, rounded and clamped to the negotiated representation"
)]
fn encode_sample(sample: f64, format: OutputFormat, output: &mut [u8]) {
  let normalized = (sample / f64::from(i16::MAX)).clamp(-1.0, 1.0);
  match (format.sample_type, format.bits_per_sample) {
    (OutputSampleType::Float, 32) => {
      output.copy_from_slice(&(normalized as f32).to_le_bytes());
    }
    (OutputSampleType::Integer, 16) => {
      let value = sample
        .round()
        .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16;
      output.copy_from_slice(&value.to_le_bytes());
    }
    (OutputSampleType::Integer, 24) => {
      let value = (normalized * 8_388_607.0)
        .round()
        .clamp(-8_388_608.0, 8_388_607.0) as i32;
      output.copy_from_slice(&value.to_le_bytes()[..3]);
    }
    (OutputSampleType::Integer, 32) => {
      let value = (normalized * f64::from(i32::MAX))
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
      output.copy_from_slice(&value.to_le_bytes());
    }
    _ => unreachable!("FrameConverter validates supported output formats"),
  }
}

#[allow(
  clippy::needless_pass_by_value,
  reason = "WASAPI errors are consumed into stable project errors"
)]
fn output_error(operation: &str, error: WasapiError) -> OutputError {
  let kind = match &error {
    WasapiError::DeviceNotFound(_) | WasapiError::IllegalDeviceState(_) => {
      OutputErrorKind::PrerequisiteMissing
    }
    WasapiError::Windows(error) => match error.code().0.cast_unsigned() {
      0x8007_0005 => OutputErrorKind::AccessDenied,
      0x8889_0004 | 0x8889_0026 | 0x8007_048f => OutputErrorKind::DeviceInvalidated,
      _ => OutputErrorKind::RenderFailure,
    },
    WasapiError::UnsupportedFormat => OutputErrorKind::InvalidFormat,
    _ => OutputErrorKind::RenderFailure,
  };
  OutputError::new(kind, format!("failed to {operation}: {error}"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn conversion_preserves_order_duration_channels_and_finite_values() {
    let formats = [
      WaveFormat::new(16, 16, &SampleType::Int, 48_000, 1, None),
      WaveFormat::new(32, 32, &SampleType::Float, 48_000, 2, None),
      WaveFormat::new(24, 24, &SampleType::Int, 96_000, 2, None),
    ];
    let input = std::array::from_fn(|index| i16::try_from(index * 32).unwrap_or(i16::MAX));
    for format in formats {
      let mut converter = FrameConverter::new(&format).expect("format is supported");
      let expected_len = converter.output_frames as usize
        * usize::from(converter.format.channels)
        * usize::from(converter.format.bits_per_sample / 8);
      converter.convert(&input);
      assert_eq!(converter.bytes.len(), expected_len);
      assert!(converter.bytes.iter().any(|byte| *byte != 0));
      let bytes_per_sample = usize::from(converter.format.bits_per_sample / 8);
      let channels = usize::from(converter.format.channels);
      let mut previous = f64::NEG_INFINITY;
      for frame in converter.bytes.chunks_exact(bytes_per_sample * channels) {
        let first = decode_sample(&frame[..bytes_per_sample], converter.format);
        assert!(first.is_finite());
        assert!(first >= previous, "resampling must preserve ramp order");
        previous = first;
        for channel in 1..channels {
          let start = channel * bytes_per_sample;
          let sample = decode_sample(&frame[start..start + bytes_per_sample], converter.format);
          assert!(sample.is_finite());
          assert!((sample - first).abs() < f64::EPSILON);
        }
      }
    }
  }

  fn decode_sample(bytes: &[u8], format: OutputFormat) -> f64 {
    match (format.sample_type, format.bits_per_sample) {
      (OutputSampleType::Float, 32) => f64::from(f32::from_le_bytes(
        bytes.try_into().expect("four float bytes"),
      )),
      (OutputSampleType::Integer, 16) => f64::from(i16::from_le_bytes(
        bytes.try_into().expect("two integer bytes"),
      )),
      (OutputSampleType::Integer, 24) => {
        let sign = if bytes[2] & 0x80 == 0 { 0 } else { 0xff };
        f64::from(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], sign]))
      }
      (OutputSampleType::Integer, 32) => f64::from(i32::from_le_bytes(
        bytes.try_into().expect("four integer bytes"),
      )),
      _ => unreachable!("test only decodes supported formats"),
    }
  }

  #[test]
  fn fresh_converter_and_clear_do_not_retain_prior_pcm() {
    let format = WaveFormat::new(32, 32, &SampleType::Float, 48_000, 2, None);
    let mut converter = FrameConverter::new(&format).expect("format is supported");
    converter.convert(&[i16::MAX; FRAME_SAMPLES]);
    assert!(converter.bytes.iter().any(|byte| *byte != 0));
    converter.clear();
    assert!(converter.bytes.iter().all(|byte| *byte == 0));
    converter.convert(&[0; FRAME_SAMPLES]);
    assert!(converter.bytes.iter().all(|byte| *byte == 0));
  }
}
