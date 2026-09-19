use mini_aec_output::FRAME_SAMPLES;

use crate::PacketMetadata;

pub(crate) struct FrameAccumulator {
  samples: [i16; FRAME_SAMPLES],
  len: usize,
}

impl Default for FrameAccumulator {
  fn default() -> Self {
    Self {
      samples: [0; FRAME_SAMPLES],
      len: 0,
    }
  }
}

impl FrameAccumulator {
  pub(crate) fn clear(&mut self) {
    self.samples.fill(0);
    self.len = 0;
  }

  pub(crate) fn push_packet(
    &mut self,
    samples: &[f32],
    metadata: PacketMetadata,
    mut emit: impl FnMut([i16; FRAME_SAMPLES]),
  ) -> u64 {
    if metadata.data_discontinuity || metadata.timestamp_error {
      self.clear();
    }

    let mut sanitized = 0_u64;
    for packet_sample in samples.iter().copied().take(metadata.frames) {
      let sample = if metadata.silent { 0.0 } else { packet_sample };
      let (converted, changed) = pcm16(sample);
      sanitized += u64::from(changed);
      self.samples[self.len] = converted;
      self.len += 1;

      if self.len == FRAME_SAMPLES {
        emit(self.samples);
        self.samples.fill(0);
        self.len = 0;
      }
    }
    sanitized
  }
}

#[allow(
  clippy::cast_possible_truncation,
  reason = "the sample is finite and clamped before the intentional PCM16 quantization"
)]
pub(crate) fn pcm16(sample: f32) -> (i16, bool) {
  if !sample.is_finite() {
    return (0, true);
  }
  let clamped = sample.clamp(-1.0, 1.0);
  let changed = !(-1.0..=1.0).contains(&sample);
  let converted = if clamped >= 0.0 {
    (clamped * f32::from(i16::MAX)).round() as i16
  } else {
    (clamped * 32_768.0).round() as i16
  };
  (converted, changed)
}

#[cfg(test)]
mod tests {
  use mini_aec_output::FRAME_SAMPLES;

  use super::{pcm16, FrameAccumulator};
  use crate::PacketMetadata;

  #[test]
  fn conversion_is_finite_clamped_and_deterministic() {
    assert_eq!(pcm16(0.0), (0, false));
    assert_eq!(pcm16(1.0), (i16::MAX, false));
    assert_eq!(pcm16(-1.0), (i16::MIN, false));
    assert_eq!(pcm16(2.0), (i16::MAX, true));
    assert_eq!(pcm16(-2.0), (i16::MIN, true));
    assert_eq!(pcm16(f32::NAN), (0, true));
    assert_eq!(pcm16(f32::INFINITY), (0, true));
    assert_eq!(pcm16(f32::NEG_INFINITY), (0, true));
  }

  #[test]
  fn accumulator_preserves_order_across_packet_boundaries() {
    let mut accumulator = FrameAccumulator::default();
    let first = vec![0.25; 111];
    let second = vec![0.5; FRAME_SAMPLES - first.len() + 27];
    let mut frames = Vec::new();
    accumulator.push_packet(
      &first,
      PacketMetadata {
        frames: first.len(),
        ..PacketMetadata::default()
      },
      |frame| frames.push(frame),
    );
    accumulator.push_packet(
      &second,
      PacketMetadata {
        frames: second.len(),
        ..PacketMetadata::default()
      },
      |frame| frames.push(frame),
    );
    assert_eq!(frames.len(), 1);
    assert!(frames[0][..111].iter().all(|sample| *sample == 8192));
    assert!(frames[0][111..].iter().all(|sample| *sample == 16_384));
  }

  #[test]
  fn silence_is_fresh_and_capture_flags_clear_partial_pcm() {
    for (data_discontinuity, timestamp_error) in [(true, false), (false, true)] {
      let mut accumulator = FrameAccumulator::default();
      let old = vec![0.75; 100];
      accumulator.push_packet(
        &old,
        PacketMetadata {
          frames: old.len(),
          ..PacketMetadata::default()
        },
        |_| panic!("partial packet must not emit"),
      );

      let ignored = vec![0.75; FRAME_SAMPLES];
      let mut frames = Vec::new();
      accumulator.push_packet(
        &ignored,
        PacketMetadata {
          frames: ignored.len(),
          silent: true,
          data_discontinuity,
          timestamp_error,
          ..PacketMetadata::default()
        },
        |frame| frames.push(frame),
      );
      assert_eq!(frames, vec![[0; FRAME_SAMPLES]]);
    }
  }
}
