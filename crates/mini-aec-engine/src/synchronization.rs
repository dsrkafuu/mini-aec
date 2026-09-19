use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use mini_aec_output::FRAME_SAMPLES;

use crate::PacketMetadata;

pub(crate) const QPC_TICKS_PER_SECOND: u64 = 10_000_000;
#[cfg(test)]
pub(crate) const FRAME_DURATION_100NS: u64 = 100_000;
pub(crate) const ALIGNMENT_TOLERANCE_100NS: u64 = 50_000;
pub(crate) const MAXIMUM_SKEW_100NS: u64 = 1_000_000;
pub(crate) const TIMED_QUEUE_CAPACITY: usize = 8;
pub(crate) const MAX_CONSECUTIVE_RENDER_MISSES: u32 = 50;
pub(crate) const HEALTHY_RECOVERY_FRAMES: u32 = 10;

#[derive(Clone, Copy, Debug)]
pub(crate) struct TimedFrame {
  pub samples: [f32; FRAME_SAMPLES],
  pub qpc_timestamp_100ns: u64,
  pub device_position: u64,
  pub reset_epoch: bool,
}

pub(crate) struct TimedFrameAccumulator {
  samples: [f32; FRAME_SAMPLES],
  len: usize,
  frame_qpc: u64,
  frame_position: u64,
  reset_epoch: bool,
}

impl Default for TimedFrameAccumulator {
  fn default() -> Self {
    Self {
      samples: [0.0; FRAME_SAMPLES],
      len: 0,
      frame_qpc: 0,
      frame_position: 0,
      reset_epoch: false,
    }
  }
}

impl TimedFrameAccumulator {
  pub(crate) fn clear(&mut self) {
    self.samples.fill(0.0);
    self.len = 0;
    self.reset_epoch = false;
  }

  pub(crate) fn push_packet(
    &mut self,
    samples: &[f32],
    metadata: PacketMetadata,
    mut emit: impl FnMut(TimedFrame),
  ) -> u64 {
    if metadata.data_discontinuity || metadata.timestamp_error {
      self.clear();
      self.reset_epoch = true;
    }

    let mut sanitized = 0_u64;
    for (offset, packet_sample) in samples.iter().copied().take(metadata.frames).enumerate() {
      if self.len == 0 {
        let offset = u64::try_from(offset).expect("packet offset fits u64");
        self.frame_qpc = metadata
          .qpc_timestamp_100ns
          .saturating_add(sample_offset_100ns(offset));
        self.frame_position = metadata.device_position.saturating_add(offset);
      }
      let sample = if metadata.silent { 0.0 } else { packet_sample };
      let finite = if sample.is_finite() {
        sample.clamp(-1.0, 1.0)
      } else {
        0.0
      };
      sanitized += u64::from(!sample.is_finite() || !(-1.0..=1.0).contains(&sample));
      self.samples[self.len] = finite;
      self.len += 1;
      if self.len == FRAME_SAMPLES {
        emit(TimedFrame {
          samples: self.samples,
          qpc_timestamp_100ns: self.frame_qpc,
          device_position: self.frame_position,
          reset_epoch: self.reset_epoch,
        });
        self.samples.fill(0.0);
        self.len = 0;
        self.reset_epoch = false;
      }
    }
    sanitized
  }
}

fn sample_offset_100ns(samples: u64) -> u64 {
  let numerator = u128::from(samples) * u128::from(QPC_TICKS_PER_SECOND);
  u64::try_from(
    (numerator + u128::from(crate::SAMPLE_RATE_HZ / 2)) / u128::from(crate::SAMPLE_RATE_HZ),
  )
  .expect("frame offset fits u64")
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TimedQueueMetrics {
  pub depth: u32,
  pub high_water: u32,
  pub overflows: u64,
  pub discarded: u64,
}

struct TimedQueueState {
  frames: VecDeque<TimedFrame>,
  producer_done: bool,
  failed: bool,
  metrics: TimedQueueMetrics,
}

pub(crate) struct TimedFrameQueue {
  state: Mutex<TimedQueueState>,
  changed: Condvar,
}

#[allow(
  clippy::large_enum_variant,
  reason = "boxing a fixed audio frame would allocate on the real-time dequeue path"
)]
pub(crate) enum TimedPopResult {
  Frame(TimedFrame),
  Timeout,
  Finished,
}

impl TimedFrameQueue {
  pub(crate) fn new() -> Self {
    Self {
      state: Mutex::new(TimedQueueState {
        frames: VecDeque::with_capacity(TIMED_QUEUE_CAPACITY),
        producer_done: false,
        failed: false,
        metrics: TimedQueueMetrics::default(),
      }),
      changed: Condvar::new(),
    }
  }

  #[allow(
    clippy::large_types_passed_by_value,
    reason = "the queue takes ownership of one fixed frame without heap allocation"
  )]
  pub(crate) fn push(&self, frame: TimedFrame) -> TimedQueueMetrics {
    let mut state = self.state.lock().expect("timed queue lock");
    if state.failed || state.producer_done {
      return state.metrics;
    }
    if state.frames.len() == TIMED_QUEUE_CAPACITY {
      let _ = state.frames.pop_front();
      state.metrics.overflows += 1;
      state.metrics.discarded += 1;
    }
    state.frames.push_back(frame);
    state.metrics.depth = u32::try_from(state.frames.len()).expect("timed queue depth fits u32");
    state.metrics.high_water = state.metrics.high_water.max(state.metrics.depth);
    let metrics = state.metrics;
    drop(state);
    self.changed.notify_one();
    metrics
  }

  pub(crate) fn pop(&self, timeout: Duration) -> TimedPopResult {
    let mut state = self.state.lock().expect("timed queue lock");
    if state.frames.is_empty() && !state.producer_done && !state.failed {
      let (guard, _) = self
        .changed
        .wait_timeout(state, timeout)
        .expect("timed queue wait");
      state = guard;
    }
    if state.failed {
      TimedPopResult::Finished
    } else if let Some(frame) = state.frames.pop_front() {
      state.metrics.depth = u32::try_from(state.frames.len()).expect("timed queue depth fits u32");
      TimedPopResult::Frame(frame)
    } else if state.producer_done {
      TimedPopResult::Finished
    } else {
      TimedPopResult::Timeout
    }
  }

  pub(crate) fn try_pop(&self) -> Option<TimedFrame> {
    let mut state = self.state.lock().expect("timed queue lock");
    let frame = state.frames.pop_front();
    state.metrics.depth = u32::try_from(state.frames.len()).expect("timed queue depth fits u32");
    frame
  }

  pub(crate) fn finish_producer(&self) {
    self.state.lock().expect("timed queue lock").producer_done = true;
    self.changed.notify_all();
  }

  pub(crate) fn fail(&self) {
    let mut state = self.state.lock().expect("timed queue lock");
    state.failed = true;
    state.frames.clear();
    state.metrics.depth = 0;
    drop(state);
    self.changed.notify_all();
  }

  pub(crate) fn clear(&self) {
    let mut state = self.state.lock().expect("timed queue lock");
    state.frames.clear();
    state.metrics.depth = 0;
  }

  pub(crate) fn metrics(&self) -> TimedQueueMetrics {
    self.state.lock().expect("timed queue lock").metrics
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use super::{
    sample_offset_100ns, TimedFrame, TimedFrameAccumulator, TimedFrameQueue, TimedPopResult,
    FRAME_DURATION_100NS, TIMED_QUEUE_CAPACITY,
  };
  use crate::PacketMetadata;

  #[test]
  fn frame_timestamps_follow_packet_offsets_across_boundaries() {
    let mut accumulator = TimedFrameAccumulator::default();
    let mut frames = Vec::new();
    accumulator.push_packet(
      &[0.25; 200],
      PacketMetadata {
        frames: 200,
        device_position: 1_000,
        qpc_timestamp_100ns: 5_000_000,
        ..PacketMetadata::default()
      },
      |frame| frames.push(frame),
    );
    accumulator.push_packet(
      &[0.5; 760],
      PacketMetadata {
        frames: 760,
        device_position: 1_200,
        qpc_timestamp_100ns: 5_000_000 + sample_offset_100ns(200),
        ..PacketMetadata::default()
      },
      |frame| frames.push(frame),
    );
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].qpc_timestamp_100ns, 5_000_000);
    assert_eq!(
      frames[1].qpc_timestamp_100ns,
      5_000_000 + FRAME_DURATION_100NS
    );
    assert_eq!(frames[1].device_position, 1_480);
  }

  #[test]
  fn discontinuity_marks_only_the_first_new_frame() {
    let mut accumulator = TimedFrameAccumulator::default();
    let mut frames = Vec::new();
    accumulator.push_packet(
      &[0.25; 100],
      PacketMetadata {
        frames: 100,
        ..PacketMetadata::default()
      },
      |_| {},
    );
    accumulator.push_packet(
      &[0.5; 960],
      PacketMetadata {
        frames: 960,
        data_discontinuity: true,
        ..PacketMetadata::default()
      },
      |frame| frames.push(frame),
    );
    assert!(frames[0].reset_epoch);
    assert!(!frames[1].reset_epoch);
  }

  #[test]
  fn timed_queue_is_bounded_and_latest_wins() {
    let queue = TimedFrameQueue::new();
    for value in 0..TIMED_QUEUE_CAPACITY + 2 {
      queue.push(TimedFrame {
        samples: [f32::from(u16::try_from(value).expect("test value fits u16")); 480],
        qpc_timestamp_100ns: value as u64,
        device_position: value as u64,
        reset_epoch: false,
      });
    }
    let metrics = queue.metrics();
    assert_eq!(
      metrics.depth,
      u32::try_from(TIMED_QUEUE_CAPACITY).expect("queue capacity fits u32")
    );
    assert_eq!(metrics.overflows, 2);
    assert_eq!(metrics.discarded, 2);
    assert!(matches!(
      queue.pop(Duration::ZERO),
      TimedPopResult::Frame(frame) if frame.device_position == 2
    ));
  }
}
