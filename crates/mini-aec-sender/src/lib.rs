//! Deterministic validation signal shared by `MiniAEC` transport candidates.

use mini_aec_transport::{PcmFrame, SessionId, FRAME_SAMPLES};

pub const FRAMES_PER_SECOND: u64 = 100;
pub const MARKER_PERIOD_FRAMES: u64 = 500;
pub const MARKER_DURATION_FRAMES: u64 = 20;

const BASE_PERIOD_SAMPLES: u64 = 128;
const BASE_AMPLITUDE: i32 = 3_000;
const MARKER_PERIOD_SAMPLES: u64 = 32;
const MARKER_AMPLITUDE: i32 = 8_000;

/// One generated frame and its correlation metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedFrame {
  session_id: SessionId,
  sequence: u64,
  marker_active: bool,
  samples: [i16; FRAME_SAMPLES],
}

impl GeneratedFrame {
  #[must_use]
  pub const fn session_id(&self) -> SessionId {
    self.session_id
  }

  #[must_use]
  pub const fn sequence(&self) -> u64 {
    self.sequence
  }

  #[must_use]
  pub const fn marker_active(&self) -> bool {
    self.marker_active
  }

  #[must_use]
  pub const fn samples(&self) -> &[i16; FRAME_SAMPLES] {
    &self.samples
  }

  #[must_use]
  pub const fn as_pcm_frame(&self) -> PcmFrame<'_> {
    PcmFrame::new(self.sequence, &self.samples)
  }
}

/// Byte-deterministic fixed-format signal generator.
#[derive(Clone, Debug)]
pub struct ValidationSignal {
  session_id: SessionId,
  next_sequence: u64,
}

impl ValidationSignal {
  #[must_use]
  pub const fn new(session_id: SessionId) -> Self {
    Self {
      session_id,
      next_sequence: 0,
    }
  }

  #[must_use]
  pub const fn session_id(&self) -> SessionId {
    self.session_id
  }

  #[must_use]
  pub const fn next_sequence(&self) -> u64 {
    self.next_sequence
  }

  /// Generates the next 10 ms frame and advances the sequence with wrapping semantics.
  pub fn next_frame(&mut self) -> GeneratedFrame {
    let sequence = self.next_sequence;
    let marker_active = sequence % MARKER_PERIOD_FRAMES < MARKER_DURATION_FRAMES;
    let mut samples = [0_i16; FRAME_SAMPLES];

    for (offset, sample) in samples.iter_mut().enumerate() {
      let sample_index = sequence
        .wrapping_mul(FRAME_SAMPLES as u64)
        .wrapping_add(offset as u64);
      let base = triangle_sample(sample_index, BASE_PERIOD_SAMPLES, BASE_AMPLITUDE);
      let marker = if marker_active {
        square_sample(sample_index, MARKER_PERIOD_SAMPLES, MARKER_AMPLITUDE)
      } else {
        0
      };
      *sample = clamp_to_i16(base + marker);
    }

    self.next_sequence = self.next_sequence.wrapping_add(1);
    GeneratedFrame {
      session_id: self.session_id,
      sequence,
      marker_active,
      samples,
    }
  }
}

fn triangle_sample(sample_index: u64, period: u64, amplitude: i32) -> i32 {
  let phase = sample_index % period;
  let half = period / 2;
  let phase = i32::try_from(phase).expect("validation periods fit in i32");
  let half = i32::try_from(half).expect("validation periods fit in i32");
  if phase < half {
    -amplitude + (2 * amplitude * phase / half)
  } else {
    amplitude - (2 * amplitude * (phase - half) / half)
  }
}

fn square_sample(sample_index: u64, period: u64, amplitude: i32) -> i32 {
  if sample_index % period < period / 2 {
    amplitude
  } else {
    -amplitude
  }
}

#[allow(clippy::cast_possible_truncation)]
fn clamp_to_i16(value: i32) -> i16 {
  value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

#[cfg(test)]
mod tests {
  use mini_aec_transport::SessionId;

  use super::{ValidationSignal, MARKER_DURATION_FRAMES, MARKER_PERIOD_FRAMES};

  fn session(value: u128) -> SessionId {
    SessionId::new(value).expect("test session is nonzero")
  }

  #[test]
  fn signal_is_byte_deterministic() {
    let mut first = ValidationSignal::new(session(1));
    let mut second = ValidationSignal::new(session(1));

    for expected_sequence in 0..600 {
      let first_frame = first.next_frame();
      let second_frame = second.next_frame();
      assert_eq!(first_frame.sequence(), expected_sequence);
      assert_eq!(first_frame, second_frame);
    }
  }

  #[test]
  fn first_frame_has_known_integer_waveform_samples() {
    let frame = ValidationSignal::new(session(1)).next_frame();
    assert_eq!(frame.samples()[0], 5_000);
    assert_eq!(frame.samples()[1], 5_093);
    assert_eq!(frame.samples()[16], -9_500);
    assert_eq!(frame.samples()[32], 8_000);
    assert_eq!(frame.samples()[64], 11_000);
    assert_eq!(frame.samples()[127], -10_906);
  }

  #[test]
  fn marker_schedule_repeats_every_five_seconds() {
    let mut signal = ValidationSignal::new(session(1));
    for sequence in 0..=MARKER_PERIOD_FRAMES {
      let frame = signal.next_frame();
      let expected = sequence < MARKER_DURATION_FRAMES || sequence == MARKER_PERIOD_FRAMES;
      assert_eq!(frame.marker_active(), expected, "sequence {sequence}");
    }
  }

  #[test]
  fn a_new_session_resets_the_frame_sequence() {
    let mut first = ValidationSignal::new(session(1));
    assert_eq!(first.next_frame().sequence(), 0);
    assert_eq!(first.next_frame().sequence(), 1);

    let mut second = ValidationSignal::new(session(2));
    assert_eq!(second.next_frame().sequence(), 0);
    assert_eq!(second.session_id(), session(2));
  }
}
