//! Windows WASAPI capture adapter for the project-owned engine input contracts.

use std::time::Duration;

use wasapi::{
  initialize_mta, AudioCaptureClient, AudioClient, Device, DeviceEnumerator, DeviceState,
  Direction, Handle, SampleType, StreamMode, WasapiError, WaveFormat,
};

use crate::{
  AudioInput, AudioInputFactory, InputRole, PacketMetadata, SourceDescriptor, SourceError,
  SourceErrorKind, SourceFormat, CHANNELS, SAMPLE_RATE_HZ,
};

/// Stateless factory for exact-ID Windows capture streams.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsAudioInputFactory;

impl AudioInputFactory for WindowsAudioInputFactory {
  fn resolve(&self, role: InputRole, endpoint_id: &str) -> Result<SourceDescriptor, SourceError> {
    initialize_mta().ok().map_err(|error| {
      SourceError::new(
        SourceErrorKind::CaptureFailure,
        format!("failed to initialize COM for endpoint resolution: {error}"),
      )
    })?;
    let enumerator =
      DeviceEnumerator::new().map_err(|error| map_wasapi("create the device enumerator", error))?;
    let device = find_device(&enumerator, role, endpoint_id)?;
    let descriptor = describe_device(&device, role)?;
    if descriptor.endpoint_id != endpoint_id {
      return Err(SourceError::new(
        SourceErrorKind::InvalidSource,
        "Windows returned a different endpoint identity than requested",
      ));
    }
    Ok(descriptor)
  }

  fn open(
    &self,
    role: InputRole,
    source: &SourceDescriptor,
  ) -> Result<Box<dyn AudioInput>, SourceError> {
    if source.role != role {
      return Err(SourceError::new(
        SourceErrorKind::InvalidSource,
        "endpoint descriptor role does not match the requested input role",
      ));
    }
    if !source.active {
      return Err(SourceError::new(
        SourceErrorKind::Unavailable,
        format!("capture endpoint {:?} is not active", source.friendly_name),
      ));
    }
    WindowsAudioInput::open(role, &source.endpoint_id)
      .map(|input| Box::new(input) as Box<dyn AudioInput>)
  }
}

/// Enumerates active Windows capture endpoints with exact IDs and native-format metadata.
///
/// # Errors
///
/// Returns a project-owned error if COM or endpoint enumeration fails.
pub fn enumerate_capture_endpoints() -> Result<Vec<SourceDescriptor>, SourceError> {
  initialize_mta().ok().map_err(|error| {
    SourceError::new(
      SourceErrorKind::CaptureFailure,
      format!("failed to initialize COM for endpoint enumeration: {error}"),
    )
  })?;
  let enumerator =
    DeviceEnumerator::new().map_err(|error| map_wasapi("create the device enumerator", error))?;
  let collection = enumerator
    .get_device_collection(&Direction::Capture)
    .map_err(|error| map_wasapi("enumerate capture endpoints", error))?;
  collection
    .into_iter()
    .map(|device| {
      let device = device.map_err(|error| map_wasapi("access a capture endpoint", error))?;
      describe_device(&device, InputRole::Microphone)
    })
    .collect()
}

/// Enumerates active Windows render endpoints with exact IDs and native-format metadata.
///
/// # Errors
///
/// Returns a project-owned error if COM or endpoint enumeration fails.
pub fn enumerate_render_endpoints() -> Result<Vec<SourceDescriptor>, SourceError> {
  initialize_mta().ok().map_err(|error| {
    SourceError::new(
      SourceErrorKind::CaptureFailure,
      format!("failed to initialize COM for render endpoint enumeration: {error}"),
    )
  })?;
  let enumerator =
    DeviceEnumerator::new().map_err(|error| map_wasapi("create the device enumerator", error))?;
  let collection = enumerator
    .get_device_collection(&Direction::Render)
    .map_err(|error| map_wasapi("enumerate render endpoints", error))?;
  collection
    .into_iter()
    .map(|device| {
      let device = device.map_err(|error| map_wasapi("access a render endpoint", error))?;
      describe_device(&device, InputRole::RenderLoopback)
    })
    .collect()
}

struct WindowsAudioInput {
  audio_client: AudioClient,
  capture_client: AudioCaptureClient,
  event: Handle,
  packet_bytes: Vec<u8>,
  stopped: bool,
}

impl WindowsAudioInput {
  fn open(role: InputRole, endpoint_id: &str) -> Result<Self, SourceError> {
    initialize_mta().ok().map_err(|error| {
      SourceError::new(
        SourceErrorKind::CaptureFailure,
        format!("failed to initialize COM in the capture worker: {error}"),
      )
    })?;
    let enumerator = DeviceEnumerator::new()
      .map_err(|error| map_wasapi("create the capture device enumerator", error))?;
    let device = find_device(&enumerator, role, endpoint_id)?;
    let current = describe_device(&device, role)?;
    if !current.active {
      return Err(SourceError::new(
        SourceErrorKind::Unavailable,
        format!(
          "capture endpoint {:?} is no longer active",
          current.friendly_name
        ),
      ));
    }
    let mut audio_client = device
      .get_iaudioclient()
      .map_err(|error| map_wasapi("create the WASAPI audio client", error))?;
    let requested = WaveFormat::new(
      32,
      32,
      &SampleType::Float,
      SAMPLE_RATE_HZ as usize,
      usize::from(CHANNELS),
      None,
    );
    let (_, minimum_period) = audio_client
      .get_device_period()
      .map_err(|error| map_wasapi("read the WASAPI device period", error))?;
    audio_client
      .initialize_client(
        &requested,
        &Direction::Capture,
        &StreamMode::EventsShared {
          autoconvert: true,
          buffer_duration_hns: minimum_period,
        },
      )
      .map_err(|error| map_wasapi("initialize the 48 kHz mono capture stream", error))?;
    let event = audio_client
      .set_get_eventhandle()
      .map_err(|error| map_wasapi("create the WASAPI capture event", error))?;
    let buffer_frames = audio_client
      .get_buffer_size()
      .map_err(|error| map_wasapi("read the WASAPI capture buffer size", error))?;
    let capture_client = audio_client
      .get_audiocaptureclient()
      .map_err(|error| map_wasapi("create the WASAPI capture client", error))?;
    let packet_bytes = vec![0_u8; buffer_frames as usize * size_of::<f32>()];
    audio_client
      .start_stream()
      .map_err(|error| map_wasapi("start the WASAPI capture stream", error))?;
    Ok(Self {
      audio_client,
      capture_client,
      event,
      packet_bytes,
      stopped: false,
    })
  }

  fn next_packet_frames(&self) -> Result<Option<u32>, SourceError> {
    self
      .capture_client
      .get_next_packet_size()
      .map_err(|error| map_wasapi("query the WASAPI capture packet size", error))
  }
}

impl AudioInput for WindowsAudioInput {
  fn read_packet(
    &mut self,
    samples: &mut [f32],
    timeout: Duration,
  ) -> Result<Option<PacketMetadata>, SourceError> {
    let mut packet_frames = self.next_packet_frames()?.unwrap_or(0);
    if packet_frames == 0 {
      let timeout_ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
      match self.event.wait_for_event(timeout_ms) {
        Ok(()) => {}
        Err(WasapiError::EventTimeout) => return Ok(None),
        Err(error) => return Err(map_wasapi("wait for the WASAPI capture event", error)),
      }
      packet_frames = self.next_packet_frames()?.unwrap_or(0);
      if packet_frames == 0 {
        return Ok(None);
      }
    }

    let frames = packet_frames as usize;
    if frames > samples.len() {
      return Err(SourceError::new(
        SourceErrorKind::CaptureFailure,
        format!(
          "WASAPI packet contains {frames} frames; engine packet capacity is {}",
          samples.len()
        ),
      ));
    }
    let required_bytes = frames * size_of::<f32>();
    if required_bytes > self.packet_bytes.len() {
      return Err(SourceError::new(
        SourceErrorKind::CaptureFailure,
        "WASAPI returned a packet larger than its preallocated endpoint buffer",
      ));
    }
    let (read_frames, info) = self
      .capture_client
      .read_from_device(&mut self.packet_bytes[..required_bytes])
      .map_err(|error| map_wasapi("read the WASAPI capture packet", error))?;
    let read_frames = read_frames as usize;
    if info.flags.silent {
      samples[..read_frames].fill(0.0);
    } else {
      let (packet_samples, remainder) =
        self.packet_bytes[..required_bytes].as_chunks::<{ size_of::<f32>() }>();
      debug_assert!(remainder.is_empty());
      for (destination, bytes) in samples[..read_frames].iter_mut().zip(packet_samples) {
        *destination = f32::from_le_bytes(*bytes);
      }
    }
    Ok(Some(PacketMetadata {
      frames: read_frames,
      silent: info.flags.silent,
      data_discontinuity: info.flags.data_discontinuity,
      timestamp_error: info.flags.timestamp_error,
      device_position: info.index,
      qpc_timestamp_100ns: info.timestamp,
    }))
  }

  fn stop(&mut self) -> Result<(), SourceError> {
    if self.stopped {
      return Ok(());
    }
    self
      .audio_client
      .stop_stream()
      .map_err(|error| map_wasapi("stop the WASAPI capture stream", error))?;
    self.stopped = true;
    Ok(())
  }
}

impl Drop for WindowsAudioInput {
  fn drop(&mut self) {
    let _ = self.stop();
  }
}

fn describe_device(device: &Device, role: InputRole) -> Result<SourceDescriptor, SourceError> {
  let endpoint_id = device
    .get_id()
    .map_err(|error| map_wasapi("read the capture endpoint ID", error))?;
  let friendly_name = device
    .get_friendlyname()
    .map_err(|error| map_wasapi("read the capture endpoint name", error))?;
  let active = device
    .get_state()
    .map_err(|error| map_wasapi("read the capture endpoint state", error))?
    == DeviceState::Active;
  let native_format = device.get_device_format().ok().map(|format| SourceFormat {
    sample_rate_hz: format.get_samplespersec(),
    channels: format.get_nchannels(),
    bits_per_sample: format.get_validbitspersample(),
  });
  Ok(SourceDescriptor {
    role,
    endpoint_id,
    friendly_name,
    active,
    native_format,
  })
}

fn find_device(
  enumerator: &DeviceEnumerator,
  role: InputRole,
  endpoint_id: &str,
) -> Result<Device, SourceError> {
  let direction = match role {
    InputRole::Microphone => Direction::Capture,
    InputRole::RenderLoopback => Direction::Render,
  };
  let collection = enumerator
    .get_device_collection(&direction)
    .map_err(|error| map_wasapi("enumerate requested endpoints", error))?;
  for device in &collection {
    let device = device.map_err(|error| map_wasapi("access a capture endpoint", error))?;
    let candidate_id = device
      .get_id()
      .map_err(|error| map_wasapi("read a capture endpoint ID", error))?;
    if candidate_id == endpoint_id {
      return Ok(device);
    }
  }
  Err(SourceError::new(
    SourceErrorKind::Unavailable,
    format!("{role:?} endpoint ID {endpoint_id:?} is unavailable or inactive"),
  ))
}

#[allow(
  clippy::needless_pass_by_value,
  reason = "map_err closures own the WASAPI error and this function consumes its diagnostic value"
)]
fn map_wasapi(operation: &str, error: WasapiError) -> SourceError {
  let kind = match &error {
    WasapiError::DeviceNotFound(_) | WasapiError::IllegalDeviceState(_) => {
      SourceErrorKind::Unavailable
    }
    WasapiError::Windows(error) => match error.code().0.cast_unsigned() {
      0x8007_0005 => SourceErrorKind::AccessDenied,
      0x8889_0004 | 0x8889_0026 | 0x8007_048f => SourceErrorKind::DeviceInvalidated,
      _ => SourceErrorKind::CaptureFailure,
    },
    _ => SourceErrorKind::CaptureFailure,
  };
  SourceError::new(kind, format!("failed to {operation}: {error}"))
}
