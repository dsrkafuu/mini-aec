use std::error::Error;
use std::fmt::{self, Display, Formatter};

use mini_aec_transport::FRAME_SAMPLES;
use webrtc_audio_processing::config::EchoCanceller as WebRtcEchoCanceller;
use webrtc_audio_processing::{Config, Processor};

/// Project-owned AEC failure without WebRTC types in the public contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EchoCancellerError {
  message: String,
}

impl EchoCancellerError {
  #[must_use]
  pub fn new(message: impl Into<String>) -> Self {
    Self {
      message: message.into(),
    }
  }

  #[must_use]
  pub fn message(&self) -> &str {
    &self.message
  }
}

impl Display for EchoCancellerError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.message)
  }
}

impl Error for EchoCancellerError {}

/// Frame-oriented replaceable echo-canceller boundary.
pub trait EchoCanceller {
  /// Processes one render frame before one capture frame and writes finite output.
  ///
  /// # Errors
  ///
  /// Returns a project-owned error when render or capture processing fails.
  fn process(
    &mut self,
    render: &[f32; FRAME_SAMPLES],
    capture: &[f32; FRAME_SAMPLES],
    output: &mut [f32; FRAME_SAMPLES],
  ) -> Result<(), EchoCancellerError>;
}

/// Creates one fresh echo canceller on the processing worker.
pub trait EchoCancellerFactory: Send + Sync {
  /// Creates a new upstream-default AEC instance.
  ///
  /// # Errors
  ///
  /// Returns a project-owned error if the processor cannot be constructed.
  fn create(&self) -> Result<Box<dyn EchoCanceller>, EchoCancellerError>;
}

/// Factory for the frozen WebRTC M131 upstream-default full echo canceller.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultEchoCancellerFactory;

impl EchoCancellerFactory for DefaultEchoCancellerFactory {
  fn create(&self) -> Result<Box<dyn EchoCanceller>, EchoCancellerError> {
    DefaultEchoCanceller::new().map(|canceller| Box::new(canceller) as Box<dyn EchoCanceller>)
  }
}

struct DefaultEchoCanceller {
  processor: Processor,
  render_channels: Vec<Vec<f32>>,
  capture_channels: Vec<Vec<f32>>,
}

impl DefaultEchoCanceller {
  fn new() -> Result<Self, EchoCancellerError> {
    let processor = Processor::new(crate::SAMPLE_RATE_HZ)
      .map_err(|error| EchoCancellerError::new(format!("failed to create WebRTC AEC: {error}")))?;
    processor.set_config(Config {
      echo_canceller: Some(WebRtcEchoCanceller::Full {
        stream_delay_ms: None,
      }),
      ..Config::default()
    });
    Ok(Self {
      processor,
      render_channels: vec![vec![0.0; FRAME_SAMPLES]],
      capture_channels: vec![vec![0.0; FRAME_SAMPLES]],
    })
  }
}

impl EchoCanceller for DefaultEchoCanceller {
  fn process(
    &mut self,
    render: &[f32; FRAME_SAMPLES],
    capture: &[f32; FRAME_SAMPLES],
    output: &mut [f32; FRAME_SAMPLES],
  ) -> Result<(), EchoCancellerError> {
    self.render_channels[0].copy_from_slice(render);
    self
      .processor
      .process_render_frame(&mut self.render_channels)
      .map_err(|error| {
        EchoCancellerError::new(format!("WebRTC render processing failed: {error}"))
      })?;
    self.capture_channels[0].copy_from_slice(capture);
    self
      .processor
      .process_capture_frame(&mut self.capture_channels)
      .map_err(|error| {
        EchoCancellerError::new(format!("WebRTC capture processing failed: {error}"))
      })?;
    if self.capture_channels[0]
      .iter()
      .any(|sample| !sample.is_finite())
    {
      return Err(EchoCancellerError::new(
        "WebRTC produced a non-finite capture sample",
      ));
    }
    for (destination, sample) in output.iter_mut().zip(&self.capture_channels[0]) {
      *destination = sample.clamp(-1.0, 1.0);
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use mini_aec_transport::FRAME_SAMPLES;

  use super::{DefaultEchoCancellerFactory, EchoCancellerFactory};

  #[test]
  fn default_adapter_processes_finite_synthetic_frames_and_resets_by_recreation() {
    let factory = DefaultEchoCancellerFactory;
    let mut first = factory.create().expect("default AEC is created");
    let render = [0.0; FRAME_SAMPLES];
    let capture = [0.125; FRAME_SAMPLES];
    let mut output = [0.0; FRAME_SAMPLES];
    first
      .process(&render, &capture, &mut output)
      .expect("default AEC processes a frame");
    assert!(output.iter().all(|sample| sample.is_finite()));

    let mut reset = factory.create().expect("fresh AEC is reconstructed");
    reset
      .process(&render, &capture, &mut output)
      .expect("reconstructed AEC processes a frame");
    assert!(output.iter().all(|sample| sample.is_finite()));
  }
}
