use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mini_aec_transport::{PcmFrame, SessionConfig, SessionId, VirtualMicrophoneSink};

use crate::framing::FrameAccumulator;
use crate::queue::{FrameQueue, PopResult, QueueMetrics};
use crate::{
  source_is_public_endpoint, AudioInputFactory, EngineCommand, EngineConfig, EngineError,
  EngineErrorKind, EngineSnapshot, EngineState, SourceDescriptor, VirtualSinkFactory, INPUT_WAIT,
};

const START_TIMEOUT: Duration = Duration::from_secs(5);
const JOIN_TIMEOUT: Duration = Duration::from_secs(3);
const SINK_WAIT: Duration = Duration::from_millis(100);
const PACKET_CAPACITY: usize = 16_384;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

struct Shared {
  snapshot: Mutex<EngineSnapshot>,
  stop_requested: AtomicBool,
  terminal_failure: AtomicBool,
}

impl Shared {
  fn record_failure(&self, error: EngineError, queue: &FrameQueue) {
    if self
      .terminal_failure
      .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
      .is_ok()
    {
      self.stop_requested.store(true, Ordering::Release);
      queue.fail();
      let mut snapshot = self.snapshot.lock().expect("engine snapshot lock");
      snapshot.state = EngineState::Failed;
      snapshot.last_error = Some(error);
      snapshot.queue_depth = 0;
    }
  }

  fn update_queue(&self, metrics: QueueMetrics) {
    let mut snapshot = self.snapshot.lock().expect("engine snapshot lock");
    snapshot.queue_depth = metrics.depth;
    snapshot.queue_high_water = metrics.high_water;
    snapshot.queue_overflows = metrics.overflows;
    snapshot.discarded_frames = metrics.discarded;
  }
}

struct RunHandles {
  queue: Arc<FrameQueue>,
  capture: Option<JoinHandle<()>>,
  sink: Option<JoinHandle<()>>,
}

/// Controller for one reusable physical-microphone bypass engine.
pub struct Engine {
  source_factory: Arc<dyn AudioInputFactory>,
  sink_factory: Arc<dyn VirtualSinkFactory>,
  shared: Arc<Shared>,
  run: Mutex<Option<RunHandles>>,
}

impl Engine {
  #[must_use]
  pub fn new(
    source_factory: Arc<dyn AudioInputFactory>,
    sink_factory: Arc<dyn VirtualSinkFactory>,
  ) -> Self {
    Self {
      source_factory,
      sink_factory,
      shared: Arc::new(Shared {
        snapshot: Mutex::new(EngineSnapshot::default()),
        stop_requested: AtomicBool::new(false),
        terminal_failure: AtomicBool::new(false),
      }),
      run: Mutex::new(None),
    }
  }

  /// Applies one low-frequency lifecycle command.
  ///
  /// # Errors
  ///
  /// Returns an actionable project-owned error when the transition, source or sink fails.
  pub fn command(&self, command: EngineCommand) -> Result<EngineSnapshot, EngineError> {
    match command {
      EngineCommand::Start(config) => self.start(config)?,
      EngineCommand::Stop => self.stop()?,
      EngineCommand::Restart(config) => {
        self.stop()?;
        self.start(config)?;
      }
    }
    Ok(self.snapshot())
  }

  /// Starts one bypass run with an explicitly selected capture endpoint.
  ///
  /// # Errors
  ///
  /// Returns an error for invalid state, endpoint resolution, recursive source selection, source
  /// startup or virtual sink startup. A failed run remains terminal until an explicit stop/start or
  /// restart command.
  ///
  /// # Panics
  ///
  /// Panics only if an engine synchronization primitive was poisoned by another thread panic.
  #[allow(
    clippy::too_many_lines,
    reason = "startup keeps source and sink readiness plus paired cleanup visible in one transition"
  )]
  pub fn start(&self, config: EngineConfig) -> Result<(), EngineError> {
    let EngineConfig { source_endpoint_id } = config;
    if source_endpoint_id.trim().is_empty() {
      return Err(EngineError::new(
        EngineErrorKind::InvalidConfiguration,
        "a non-empty physical capture endpoint ID is required",
      ));
    }

    let state = self
      .shared
      .snapshot
      .lock()
      .expect("engine snapshot lock")
      .state;
    match state {
      EngineState::RunningBypass | EngineState::Starting | EngineState::Stopping => {
        return Err(EngineError::new(
          EngineErrorKind::InvalidConfiguration,
          format!("cannot start the engine while it is {state:?}"),
        ));
      }
      EngineState::Failed => self.cleanup_previous_run()?,
      EngineState::Stopped => {}
    }

    let source = self
      .source_factory
      .resolve(source_endpoint_id.trim())
      .map_err(EngineError::from_source)?;
    validate_source(&source, source_endpoint_id.trim())?;

    let run_id = new_identity();
    let session_value = new_identity();
    let session_id = SessionId::new(session_value).expect("generated identity is nonzero");
    {
      let mut snapshot = self.shared.snapshot.lock().expect("engine snapshot lock");
      *snapshot = EngineSnapshot {
        state: EngineState::Starting,
        source: Some(source.clone()),
        run_id: Some(run_id),
        session_id: Some(session_value),
        ..EngineSnapshot::default()
      };
    }
    self.shared.stop_requested.store(false, Ordering::Release);
    self.shared.terminal_failure.store(false, Ordering::Release);

    let queue = Arc::new(FrameQueue::new());
    let (sink_ready_tx, sink_ready_rx) = mpsc::sync_channel(1);
    let sink = spawn_sink_worker(
      Arc::clone(&self.sink_factory),
      session_id,
      Arc::clone(&queue),
      Arc::clone(&self.shared),
      sink_ready_tx,
    )?;
    let mut handles = RunHandles {
      queue: Arc::clone(&queue),
      capture: None,
      sink: Some(sink),
    };
    if let Err(error) = wait_ready(&sink_ready_rx, "virtual sink") {
      self.shared.record_failure(error.clone(), &queue);
      let _ = join_run(&mut handles);
      *self.run.lock().expect("engine run lock") = Some(handles);
      return Err(error);
    }

    let (capture_ready_tx, capture_ready_rx) = mpsc::sync_channel(1);
    let capture = match spawn_capture_worker(
      Arc::clone(&self.source_factory),
      source,
      Arc::clone(&queue),
      Arc::clone(&self.shared),
      capture_ready_tx,
    ) {
      Ok(capture) => capture,
      Err(error) => {
        self.shared.record_failure(error.clone(), &queue);
        let _ = join_run(&mut handles);
        *self.run.lock().expect("engine run lock") = Some(handles);
        return Err(error);
      }
    };
    handles.capture = Some(capture);
    *self.run.lock().expect("engine run lock") = Some(handles);

    if let Err(error) = wait_ready(&capture_ready_rx, "physical capture") {
      self.shared.record_failure(error.clone(), &queue);
      self.cleanup_previous_run()?;
      return Err(error);
    }
    let mut snapshot = self.shared.snapshot.lock().expect("engine snapshot lock");
    if self.shared.terminal_failure.load(Ordering::Acquire) {
      return Err(snapshot.last_error.clone().unwrap_or_else(|| {
        EngineError::new(
          EngineErrorKind::WorkerFailure,
          "engine startup worker failed",
        )
      }));
    }
    snapshot.state = EngineState::RunningBypass;
    Ok(())
  }

  /// Stops the active run, drains complete frames on normal stop and clears all buffered PCM.
  ///
  /// # Errors
  ///
  /// Returns an error if a worker cannot finish within the bounded join interval.
  ///
  /// # Panics
  ///
  /// Panics only if an engine synchronization primitive was poisoned by another thread panic.
  pub fn stop(&self) -> Result<(), EngineError> {
    let state = self
      .shared
      .snapshot
      .lock()
      .expect("engine snapshot lock")
      .state;
    if state == EngineState::Stopped && self.run.lock().expect("engine run lock").is_none() {
      return Ok(());
    }
    if state != EngineState::Failed {
      self
        .shared
        .snapshot
        .lock()
        .expect("engine snapshot lock")
        .state = EngineState::Stopping;
    }
    self.shared.stop_requested.store(true, Ordering::Release);
    self.cleanup_previous_run()?;
    let mut snapshot = self.shared.snapshot.lock().expect("engine snapshot lock");
    snapshot.state = EngineState::Stopped;
    snapshot.queue_depth = 0;
    Ok(())
  }

  /// Returns a metadata-only snapshot without exposing PCM.
  ///
  /// # Panics
  ///
  /// Panics only if an engine synchronization primitive was poisoned by another thread panic.
  #[must_use]
  pub fn snapshot(&self) -> EngineSnapshot {
    let mut snapshot = self
      .shared
      .snapshot
      .lock()
      .expect("engine snapshot lock")
      .clone();
    if let Some(run) = self.run.lock().expect("engine run lock").as_ref() {
      let metrics = run.queue.metrics();
      snapshot.queue_depth = metrics.depth;
      snapshot.queue_high_water = metrics.high_water;
      snapshot.queue_overflows = metrics.overflows;
      snapshot.discarded_frames = metrics.discarded;
    }
    snapshot
  }

  fn cleanup_previous_run(&self) -> Result<(), EngineError> {
    self.shared.stop_requested.store(true, Ordering::Release);
    let Some(mut handles) = self.run.lock().expect("engine run lock").take() else {
      return Ok(());
    };
    join_run(&mut handles)?;
    let metrics = handles.queue.clear();
    self.shared.update_queue(metrics);
    Ok(())
  }
}

impl Drop for Engine {
  fn drop(&mut self) {
    let _ = self.stop();
  }
}

fn validate_source(source: &SourceDescriptor, requested_id: &str) -> Result<(), EngineError> {
  if source.endpoint_id != requested_id {
    return Err(EngineError::new(
      EngineErrorKind::InvalidSource,
      "the capture adapter did not resolve the exact requested endpoint ID",
    ));
  }
  if !source.active {
    return Err(EngineError::new(
      EngineErrorKind::SourceUnavailable,
      format!("capture endpoint {:?} is not active", source.friendly_name),
    ));
  }
  if source_is_public_endpoint(source) {
    return Err(EngineError::new(
      EngineErrorKind::InvalidSource,
      "MiniAEC Microphone cannot be selected as its own physical source",
    ));
  }
  Ok(())
}

fn spawn_sink_worker(
  factory: Arc<dyn VirtualSinkFactory>,
  session_id: SessionId,
  queue: Arc<FrameQueue>,
  shared: Arc<Shared>,
  ready: mpsc::SyncSender<Result<(), EngineError>>,
) -> Result<JoinHandle<()>, EngineError> {
  thread::Builder::new()
    .name("mini-aec-sink".to_owned())
    .spawn(move || sink_worker(factory.as_ref(), session_id, &queue, &shared, &ready))
    .map_err(|error| {
      EngineError::new(
        EngineErrorKind::WorkerFailure,
        format!("failed to spawn virtual sink worker: {error}"),
      )
    })
}

fn sink_worker(
  factory: &dyn VirtualSinkFactory,
  session_id: SessionId,
  queue: &FrameQueue,
  shared: &Shared,
  ready: &mpsc::SyncSender<Result<(), EngineError>>,
) {
  let mut sink = match factory.connect() {
    Ok(sink) => sink,
    Err(error) => {
      let error = EngineError::from_sink(&error);
      let _ = ready.send(Err(error));
      return;
    }
  };
  if let Err(error) = sink.open_session(SessionConfig::new(session_id)) {
    let error = EngineError::from_sink(&error);
    let _ = ready.send(Err(error));
    return;
  }
  match sink.diagnostics() {
    Ok(diagnostics) => {
      let diagnostics = crate::SinkTransportSnapshot::from(diagnostics);
      let mut snapshot = shared.snapshot.lock().expect("engine snapshot lock");
      snapshot.sink_diagnostics_start = Some(diagnostics.clone());
      snapshot.sink_diagnostics_latest = Some(diagnostics);
    }
    Err(error) => {
      let error = EngineError::from_sink(&error);
      let _ = sink.close_session();
      let _ = ready.send(Err(error));
      return;
    }
  }
  if ready.send(Ok(())).is_err() {
    let _ = sink.close_session();
    return;
  }

  let mut sequence = 0_u64;
  loop {
    match queue.pop(SINK_WAIT) {
      PopResult::Frame(samples) => {
        let frame = PcmFrame::new(sequence, &samples);
        match sink.write_frame(frame) {
          Ok(_) => {
            sequence = sequence.saturating_add(1);
            shared
              .snapshot
              .lock()
              .expect("engine snapshot lock")
              .sink_accepted_frames += 1;
            if sequence.is_multiple_of(100) {
              if let Err(error) = refresh_sink_diagnostics(sink.as_ref(), shared) {
                shared
                  .snapshot
                  .lock()
                  .expect("engine snapshot lock")
                  .sink_failures += 1;
                shared.record_failure(error, queue);
                break;
              }
            }
          }
          Err(error) => {
            shared
              .snapshot
              .lock()
              .expect("engine snapshot lock")
              .sink_failures += 1;
            shared.record_failure(EngineError::from_sink(&error), queue);
            break;
          }
        }
      }
      PopResult::Timeout => {}
      PopResult::Finished => break,
    }
  }

  if let Err(error) = refresh_sink_diagnostics(sink.as_ref(), shared) {
    if !shared.terminal_failure.load(Ordering::Acquire) {
      shared
        .snapshot
        .lock()
        .expect("engine snapshot lock")
        .sink_failures += 1;
      shared.record_failure(error, queue);
    }
  }

  if let Err(error) = sink.close_session() {
    shared
      .snapshot
      .lock()
      .expect("engine snapshot lock")
      .sink_failures += 1;
    shared.record_failure(EngineError::from_sink(&error), queue);
  }
}

fn refresh_sink_diagnostics(
  sink: &dyn VirtualMicrophoneSink,
  shared: &Shared,
) -> Result<(), EngineError> {
  let diagnostics = sink
    .diagnostics()
    .map_err(|error| EngineError::from_sink(&error))?;
  shared
    .snapshot
    .lock()
    .expect("engine snapshot lock")
    .sink_diagnostics_latest = Some(diagnostics.into());
  Ok(())
}

fn spawn_capture_worker(
  factory: Arc<dyn AudioInputFactory>,
  source: SourceDescriptor,
  queue: Arc<FrameQueue>,
  shared: Arc<Shared>,
  ready: mpsc::SyncSender<Result<(), EngineError>>,
) -> Result<JoinHandle<()>, EngineError> {
  thread::Builder::new()
    .name("mini-aec-capture".to_owned())
    .spawn(move || capture_worker(factory.as_ref(), &source, &queue, &shared, &ready))
    .map_err(|error| {
      EngineError::new(
        EngineErrorKind::WorkerFailure,
        format!("failed to spawn physical capture worker: {error}"),
      )
    })
}

fn capture_worker(
  factory: &dyn AudioInputFactory,
  source: &SourceDescriptor,
  queue: &FrameQueue,
  shared: &Shared,
  ready: &mpsc::SyncSender<Result<(), EngineError>>,
) {
  let mut input = match factory.open(source) {
    Ok(input) => input,
    Err(error) => {
      let error = EngineError::from_source(error);
      let _ = ready.send(Err(error));
      queue.finish_producer();
      return;
    }
  };
  if ready.send(Ok(())).is_err() {
    let _ = input.stop();
    queue.finish_producer();
    return;
  }

  let mut packet = vec![0.0_f32; PACKET_CAPACITY];
  let mut accumulator = FrameAccumulator::default();
  while !shared.stop_requested.load(Ordering::Acquire) {
    match input.read_packet(&mut packet, INPUT_WAIT) {
      Ok(Some(metadata)) => {
        if metadata.frames > packet.len() {
          shared.record_failure(
            EngineError::new(
              EngineErrorKind::SourceFailure,
              format!(
                "capture packet contains {} frames; preallocated capacity is {}",
                metadata.frames,
                packet.len()
              ),
            ),
            queue,
          );
          break;
        }
        {
          let mut snapshot = shared.snapshot.lock().expect("engine snapshot lock");
          snapshot.captured_packets += 1;
          snapshot.captured_samples += metadata.frames as u64;
          snapshot.silent_packets += u64::from(metadata.silent);
          snapshot.discontinuities += u64::from(metadata.data_discontinuity);
          snapshot.timestamp_errors += u64::from(metadata.timestamp_error);
          snapshot.last_device_position = Some(metadata.device_position);
          snapshot.last_qpc_timestamp_100ns = Some(metadata.qpc_timestamp_100ns);
        }
        let sanitized = accumulator.push_packet(&packet, metadata, |frame| {
          let metrics = queue.push(frame);
          let mut snapshot = shared.snapshot.lock().expect("engine snapshot lock");
          snapshot.output_frames += 1;
          snapshot.queue_depth = metrics.depth;
          snapshot.queue_high_water = metrics.high_water;
          snapshot.queue_overflows = metrics.overflows;
          snapshot.discarded_frames = metrics.discarded;
        });
        shared
          .snapshot
          .lock()
          .expect("engine snapshot lock")
          .sanitized_samples += sanitized;
      }
      Ok(None) => {}
      Err(error) => {
        shared.record_failure(EngineError::from_source(error), queue);
        break;
      }
    }
  }
  accumulator.clear();
  if let Err(error) = input.stop() {
    if !shared.terminal_failure.load(Ordering::Acquire) {
      shared.record_failure(EngineError::from_source(error), queue);
    }
  }
  queue.finish_producer();
}

fn wait_ready(
  receiver: &Receiver<Result<(), EngineError>>,
  worker: &str,
) -> Result<(), EngineError> {
  match receiver.recv_timeout(START_TIMEOUT) {
    Ok(result) => result,
    Err(mpsc::RecvTimeoutError::Timeout) => Err(EngineError::new(
      EngineErrorKind::WorkerFailure,
      format!("{worker} did not become ready within {START_TIMEOUT:?}"),
    )),
    Err(mpsc::RecvTimeoutError::Disconnected) => Err(EngineError::new(
      EngineErrorKind::WorkerFailure,
      format!("{worker} ended before reporting readiness"),
    )),
  }
}

fn join_run(handles: &mut RunHandles) -> Result<(), EngineError> {
  let mut first_error = None;
  if let Some(capture) = handles.capture.take() {
    if let Err(error) = join_finite(capture, "capture") {
      handles.queue.fail();
      first_error = Some(error);
    }
  } else {
    handles.queue.finish_producer();
  }
  if let Some(sink) = handles.sink.take() {
    if let Err(error) = join_finite(sink, "sink") {
      first_error.get_or_insert(error);
    }
  }
  first_error.map_or(Ok(()), Err)
}

fn join_finite(handle: JoinHandle<()>, worker: &str) -> Result<(), EngineError> {
  let deadline = Instant::now() + JOIN_TIMEOUT;
  while !handle.is_finished() && Instant::now() < deadline {
    thread::sleep(Duration::from_millis(5));
  }
  if !handle.is_finished() {
    return Err(EngineError::new(
      EngineErrorKind::StopTimeout,
      format!("{worker} worker did not stop within {JOIN_TIMEOUT:?}"),
    ));
  }
  handle.join().map_err(|_| {
    EngineError::new(
      EngineErrorKind::WorkerFailure,
      format!("{worker} worker panicked"),
    )
  })
}

fn new_identity() -> u128 {
  let counter = u128::from(NEXT_ID.fetch_add(1, Ordering::Relaxed));
  let time = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |duration| duration.as_nanos());
  let high = (time ^ u128::from(std::process::id())) & u128::from(u64::MAX);
  let value = (high << 64) | counter;
  if value == 0 {
    1
  } else {
    value
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;
  use std::thread;
  use std::time::{Duration, Instant};

  use mini_aec_transport::{SinkError, SinkErrorKind, FRAME_SAMPLES};

  use super::Engine;
  use crate::test_support::{FakeAudioInputFactory, FakeSinkFactory, FakeSinkPlan, SourceStep};
  use crate::{
    EngineConfig, EngineErrorKind, EngineState, PacketMetadata, SourceDescriptor, SourceError,
    SourceErrorKind,
  };

  fn descriptor() -> SourceDescriptor {
    SourceDescriptor {
      endpoint_id: "physical-id".to_owned(),
      friendly_name: "Synthetic Microphone".to_owned(),
      active: true,
      native_format: None,
    }
  }

  fn packet(value: f32, frames: usize) -> SourceStep {
    SourceStep::Packet {
      samples: vec![value; frames],
      metadata: PacketMetadata {
        frames,
        ..PacketMetadata::default()
      },
    }
  }

  fn engine(source: &FakeAudioInputFactory, sink: &FakeSinkFactory) -> Engine {
    Engine::new(Arc::new(source.clone()), Arc::new(sink.clone()))
  }

  fn wait_for(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() && Instant::now() < deadline {
      thread::sleep(Duration::from_millis(1));
    }
    assert!(condition(), "condition did not become true before timeout");
  }

  #[test]
  fn normal_start_stop_and_cleanup_are_idempotent() {
    let source = FakeAudioInputFactory::new(descriptor());
    source.push_run([packet(0.25, FRAME_SAMPLES)]);
    let sink = FakeSinkFactory::default();
    let engine = engine(&source, &sink);

    engine
      .start(EngineConfig::new("physical-id"))
      .expect("engine starts");
    assert_eq!(engine.snapshot().state, EngineState::RunningBypass);
    wait_for(|| engine.snapshot().sink_accepted_frames == 1);
    engine.stop().expect("engine stops");
    engine.stop().expect("second stop is harmless");

    let snapshot = engine.snapshot();
    assert_eq!(snapshot.state, EngineState::Stopped);
    assert_eq!(snapshot.queue_depth, 0);
    assert_eq!(
      snapshot
        .sink_diagnostics_latest
        .as_ref()
        .map(|diagnostics| diagnostics.accepted_frames),
      Some(1)
    );
    assert_eq!(source.stop_count(), 1);
    assert_eq!(sink.records()[0].close_calls, 1);
  }

  #[test]
  fn failed_start_closes_the_sink_and_reports_terminal_error() {
    let source = FakeAudioInputFactory::new(descriptor());
    source.push_open_error(SourceError::new(
      SourceErrorKind::AccessDenied,
      "capture denied",
    ));
    let sink = FakeSinkFactory::default();
    let engine = engine(&source, &sink);

    let error = engine
      .start(EngineConfig::new("physical-id"))
      .expect_err("source startup fails");
    assert_eq!(error.kind, EngineErrorKind::SourceAccessDenied);
    assert_eq!(engine.snapshot().state, EngineState::Failed);
    assert_eq!(sink.records()[0].close_calls, 1);
    engine.stop().expect("failed run cleanup succeeds");
    assert_eq!(engine.snapshot().state, EngineState::Stopped);
  }

  #[test]
  fn recursive_source_is_rejected_before_sink_connection() {
    let mut source_descriptor = descriptor();
    source_descriptor.friendly_name = crate::PUBLIC_CAPTURE_ENDPOINT_NAME.to_owned();
    let source = FakeAudioInputFactory::new(source_descriptor);
    let sink = FakeSinkFactory::default();
    let engine = engine(&source, &sink);
    let error = engine
      .start(EngineConfig::new("physical-id"))
      .expect_err("recursive source is rejected");
    assert_eq!(error.kind, EngineErrorKind::InvalidSource);
    assert!(sink.records().is_empty());
  }

  #[test]
  fn sink_categories_remain_actionable() {
    for (sink_kind, engine_kind) in [
      (
        SinkErrorKind::DriverUnavailable,
        EngineErrorKind::DriverUnavailable,
      ),
      (
        SinkErrorKind::AccessDenied,
        EngineErrorKind::SinkAccessDenied,
      ),
      (SinkErrorKind::Busy, EngineErrorKind::SenderBusy),
      (
        SinkErrorKind::VersionMismatch,
        EngineErrorKind::VersionMismatch,
      ),
    ] {
      let source = FakeAudioInputFactory::new(descriptor());
      let sink = FakeSinkFactory::default();
      sink.push_plan(FakeSinkPlan {
        connect_error: Some(SinkError::new(sink_kind, "scripted sink failure")),
        ..FakeSinkPlan::default()
      });
      let engine = engine(&source, &sink);
      let error = engine
        .start(EngineConfig::new("physical-id"))
        .expect_err("sink startup fails");
      assert_eq!(error.kind, engine_kind);
      assert_eq!(engine.snapshot().state, EngineState::Failed);
    }
  }

  #[test]
  fn source_failure_is_terminal_and_explicit_restart_is_isolated() {
    let source = FakeAudioInputFactory::new(descriptor());
    source.push_run([
      packet(0.25, FRAME_SAMPLES),
      SourceStep::Delay(Duration::from_millis(20)),
      SourceStep::Failure(SourceError::new(
        SourceErrorKind::DeviceInvalidated,
        "device removed",
      )),
    ]);
    source.push_run([packet(0.5, FRAME_SAMPLES)]);
    let sink = FakeSinkFactory::default();
    let engine = engine(&source, &sink);

    engine
      .start(EngineConfig::new("physical-id"))
      .expect("first run starts");
    wait_for(|| engine.snapshot().state == EngineState::Failed);
    let first = engine.snapshot();
    assert_eq!(
      first.last_error.as_ref().map(|error| error.kind),
      Some(EngineErrorKind::SourceInvalidated)
    );

    engine
      .start(EngineConfig::new("physical-id"))
      .expect("explicit second start recovers");
    wait_for(|| engine.snapshot().sink_accepted_frames == 1);
    let second = engine.snapshot();
    assert_ne!(first.run_id, second.run_id);
    assert_ne!(first.session_id, second.session_id);
    engine.stop().expect("second run stops");

    let records = sink.records();
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].accepted, vec![(0, 16_384)]);
  }

  #[test]
  fn backpressure_discards_oldest_but_sink_sequences_remain_gap_free() {
    let source = FakeAudioInputFactory::new(descriptor());
    source.push_run([SourceStep::RepeatedPacket {
      samples: vec![0.25; FRAME_SAMPLES],
      metadata: PacketMetadata {
        frames: FRAME_SAMPLES,
        ..PacketMetadata::default()
      },
      remaining: 100,
    }]);
    let sink = FakeSinkFactory::default();
    sink.push_plan(FakeSinkPlan {
      write_delay: Duration::from_millis(1),
      ..FakeSinkPlan::default()
    });
    let engine = engine(&source, &sink);
    engine
      .start(EngineConfig::new("physical-id"))
      .expect("engine starts");
    wait_for(|| engine.snapshot().output_frames == 100);
    engine.stop().expect("engine drains and stops");

    let snapshot = engine.snapshot();
    assert!(snapshot.queue_overflows > 0);
    assert_eq!(snapshot.queue_overflows, snapshot.discarded_frames);
    assert!(snapshot.queue_high_water <= 4);
    let accepted = &sink.records()[0].accepted;
    assert!(accepted
      .iter()
      .enumerate()
      .all(|(index, (sequence, _))| *sequence == u64::try_from(index).expect("index fits u64")));
  }

  #[test]
  fn rejected_sink_write_fails_both_sides_and_clears_pcm() {
    let source = FakeAudioInputFactory::new(descriptor());
    source.push_run([SourceStep::RepeatedPacket {
      samples: vec![0.5; FRAME_SAMPLES],
      metadata: PacketMetadata {
        frames: FRAME_SAMPLES,
        ..PacketMetadata::default()
      },
      remaining: 20,
    }]);
    let sink = FakeSinkFactory::default();
    sink.push_plan(FakeSinkPlan {
      write_error_at: Some((2, SinkError::new(SinkErrorKind::RejectedWrite, "rejected"))),
      ..FakeSinkPlan::default()
    });
    let engine = engine(&source, &sink);
    engine
      .start(EngineConfig::new("physical-id"))
      .expect("engine starts");
    wait_for(|| engine.snapshot().state == EngineState::Failed);
    let snapshot = engine.snapshot();
    assert_eq!(snapshot.queue_depth, 0);
    assert_eq!(snapshot.sink_failures, 1);
    assert_eq!(
      snapshot.last_error.as_ref().map(|error| error.kind),
      Some(EngineErrorKind::RejectedWrite)
    );
    engine.stop().expect("failed run cleans up");
  }

  #[test]
  fn synthetic_five_minute_soak_has_exact_bounded_accounting() {
    const FIVE_MINUTE_FRAMES: u64 = 5 * 60 * 100;
    let source = FakeAudioInputFactory::new(descriptor());
    source.push_run([SourceStep::RepeatedPacket {
      samples: vec![0.125; FRAME_SAMPLES],
      metadata: PacketMetadata {
        frames: FRAME_SAMPLES,
        ..PacketMetadata::default()
      },
      remaining: FIVE_MINUTE_FRAMES,
    }]);
    let sink = FakeSinkFactory::default();
    let engine = engine(&source, &sink);
    engine
      .start(EngineConfig::new("physical-id"))
      .expect("soak starts");
    wait_for(|| engine.snapshot().output_frames == FIVE_MINUTE_FRAMES);
    engine.stop().expect("soak stops");

    let snapshot = engine.snapshot();
    assert_eq!(snapshot.captured_packets, FIVE_MINUTE_FRAMES);
    assert_eq!(
      snapshot.captured_samples,
      FIVE_MINUTE_FRAMES * FRAME_SAMPLES as u64
    );
    assert_eq!(snapshot.output_frames, FIVE_MINUTE_FRAMES);
    assert_eq!(
      snapshot.sink_accepted_frames + snapshot.discarded_frames,
      FIVE_MINUTE_FRAMES
    );
    assert_eq!(snapshot.sanitized_samples, 0);
    assert_eq!(snapshot.queue_depth, 0);
    assert!(snapshot.queue_high_water <= 4);
    assert_eq!(
      snapshot
        .sink_diagnostics_latest
        .as_ref()
        .map(|diagnostics| diagnostics.accepted_frames),
      Some(snapshot.sink_accepted_frames)
    );
  }
}
