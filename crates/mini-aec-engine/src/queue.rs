use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use mini_aec_output::FRAME_SAMPLES;

pub(crate) const QUEUE_CAPACITY: usize = 4;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct QueueMetrics {
  pub depth: u32,
  pub high_water: u32,
  pub overflows: u64,
  pub discarded: u64,
}

struct QueueState {
  frames: VecDeque<[i16; FRAME_SAMPLES]>,
  producer_done: bool,
  failed: bool,
  metrics: QueueMetrics,
}

pub(crate) struct FrameQueue {
  state: Mutex<QueueState>,
  changed: Condvar,
}

#[allow(
  clippy::large_enum_variant,
  reason = "boxing a fixed audio frame would add a heap allocation to the real-time path"
)]
pub(crate) enum PopResult {
  Frame([i16; FRAME_SAMPLES]),
  Timeout,
  Finished,
}

impl FrameQueue {
  pub(crate) fn new() -> Self {
    Self {
      state: Mutex::new(QueueState {
        frames: VecDeque::with_capacity(QUEUE_CAPACITY),
        producer_done: false,
        failed: false,
        metrics: QueueMetrics::default(),
      }),
      changed: Condvar::new(),
    }
  }

  #[allow(
    clippy::large_types_passed_by_value,
    reason = "moving the fixed frame into preallocated queue storage avoids copying or allocating"
  )]
  pub(crate) fn push(&self, frame: [i16; FRAME_SAMPLES]) -> QueueMetrics {
    let mut state = self.state.lock().expect("frame queue lock");
    if state.failed || state.producer_done {
      return state.metrics;
    }
    if state.frames.len() == QUEUE_CAPACITY {
      let _ = state.frames.pop_front();
      state.metrics.overflows += 1;
      state.metrics.discarded += 1;
    }
    state.frames.push_back(frame);
    state.metrics.depth = u32::try_from(state.frames.len()).expect("queue depth fits u32");
    state.metrics.high_water = state.metrics.high_water.max(state.metrics.depth);
    let metrics = state.metrics;
    drop(state);
    self.changed.notify_one();
    metrics
  }

  pub(crate) fn pop(&self, timeout: Duration) -> PopResult {
    let mut state = self.state.lock().expect("frame queue lock");
    if state.frames.is_empty() && !state.producer_done && !state.failed {
      let (guard, _) = self
        .changed
        .wait_timeout(state, timeout)
        .expect("frame queue wait");
      state = guard;
    }
    if state.failed {
      return PopResult::Finished;
    }
    if let Some(frame) = state.frames.pop_front() {
      state.metrics.depth = u32::try_from(state.frames.len()).expect("queue depth fits u32");
      PopResult::Frame(frame)
    } else if state.producer_done {
      PopResult::Finished
    } else {
      PopResult::Timeout
    }
  }

  pub(crate) fn finish_producer(&self) {
    self.state.lock().expect("frame queue lock").producer_done = true;
    self.changed.notify_all();
  }

  pub(crate) fn fail(&self) {
    let mut state = self.state.lock().expect("frame queue lock");
    state.failed = true;
    state.frames.clear();
    state.metrics.depth = 0;
    drop(state);
    self.changed.notify_all();
  }

  pub(crate) fn clear(&self) -> QueueMetrics {
    let mut state = self.state.lock().expect("frame queue lock");
    state.frames.clear();
    state.metrics.depth = 0;
    state.metrics
  }

  pub(crate) fn metrics(&self) -> QueueMetrics {
    self.state.lock().expect("frame queue lock").metrics
  }
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use super::{FrameQueue, PopResult, QUEUE_CAPACITY};

  #[test]
  fn latest_wins_queue_wraps_and_keeps_complete_frames() {
    let queue = FrameQueue::new();
    for value in 0_i16..6 {
      queue.push([value; 480]);
    }
    let metrics = queue.metrics();
    let capacity = u32::try_from(QUEUE_CAPACITY).expect("test queue capacity fits u32");
    assert_eq!(metrics.depth, capacity);
    assert_eq!(metrics.high_water, capacity);
    assert_eq!(metrics.overflows, 2);
    assert_eq!(metrics.discarded, 2);

    let mut values = Vec::new();
    for _ in 0..QUEUE_CAPACITY {
      match queue.pop(Duration::ZERO) {
        PopResult::Frame(frame) => values.push(frame[0]),
        PopResult::Timeout | PopResult::Finished => panic!("frame expected"),
      }
    }
    assert_eq!(values, [2, 3, 4, 5]);
  }

  #[test]
  fn failure_clears_unread_pcm() {
    let queue = FrameQueue::new();
    queue.push([7; 480]);
    queue.fail();
    assert!(matches!(queue.pop(Duration::ZERO), PopResult::Finished));
    assert_eq!(queue.metrics().depth, 0);
  }
}
