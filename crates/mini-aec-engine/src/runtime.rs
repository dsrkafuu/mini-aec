use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mini_aec_transport::{PcmFrame, SessionConfig, SessionId, VirtualMicrophoneSink};

use crate::framing::FrameAccumulator;
use crate::queue::{FrameQueue, PopResult, QueueMetrics};
use crate::synchronization::{
  TimedFrame, TimedFrameAccumulator, TimedFrameQueue, TimedPopResult, ALIGNMENT_TOLERANCE_100NS,
  HEALTHY_RECOVERY_FRAMES, MAXIMUM_SKEW_100NS, MAX_CONSECUTIVE_RENDER_MISSES,
};
use crate::{
  source_is_public_endpoint, AudioInputFactory, DegradationReason, EchoCancellerFactory,
  EngineCommand, EngineConfig, EngineError, EngineErrorKind, EngineSnapshot, EngineState,
  InputRole, ProcessingMode, ProcessingTimeSnapshot, SourceDescriptor, VirtualSinkFactory,
  INPUT_WAIT,
};

const START_TIMEOUT: Duration = Duration::from_secs(5);
const JOIN_TIMEOUT: Duration = Duration::from_secs(3);
const SINK_WAIT: Duration = Duration::from_millis(100);
const RENDER_WAIT: Duration = Duration::from_millis(5);
const PROCESSING_BUDGET: Duration = Duration::from_millis(10);
const MAX_CONSECUTIVE_AEC_FAILURES: u32 = 3;
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

  fn record_aec_failure(
    &self,
    error: EngineError,
    microphone_queue: &TimedFrameQueue,
    render_queue: &TimedFrameQueue,
  ) {
    if self
      .terminal_failure
      .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
      .is_ok()
    {
      self.stop_requested.store(true, Ordering::Release);
      microphone_queue.fail();
      render_queue.fail();
      let mut snapshot = self.snapshot.lock().expect("engine snapshot lock");
      snapshot.state = EngineState::Failed;
      snapshot.last_error = Some(error);
      snapshot.queue_depth = 0;
      snapshot.render_queue_depth = 0;
      snapshot.degradation_reason = None;
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
  queue: Option<Arc<FrameQueue>>,
  microphone_queue: Option<Arc<TimedFrameQueue>>,
  render_queue: Option<Arc<TimedFrameQueue>>,
  capture: Option<JoinHandle<()>>,
  render: Option<JoinHandle<()>>,
  sink: Option<JoinHandle<()>>,
  processing: Option<JoinHandle<()>>,
}

/// Controller for one reusable physical-microphone bypass engine.
pub struct Engine {
  source_factory: Arc<dyn AudioInputFactory>,
  sink_factory: Arc<dyn VirtualSinkFactory>,
  echo_factory: Option<Arc<dyn EchoCancellerFactory>>,
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
      echo_factory: None,
      shared: Arc::new(Shared {
        snapshot: Mutex::new(EngineSnapshot::default()),
        stop_requested: AtomicBool::new(false),
        terminal_failure: AtomicBool::new(false),
      }),
      run: Mutex::new(None),
    }
  }

  #[must_use]
  pub fn new_with_aec(
    source_factory: Arc<dyn AudioInputFactory>,
    sink_factory: Arc<dyn VirtualSinkFactory>,
    echo_factory: Arc<dyn EchoCancellerFactory>,
  ) -> Self {
    let mut engine = Self::new(source_factory, sink_factory);
    engine.echo_factory = Some(echo_factory);
    engine
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
  #[allow(
    clippy::needless_pass_by_value,
    reason = "the public controller consumes one explicit run configuration as a command value"
  )]
  pub fn start(&self, config: EngineConfig) -> Result<(), EngineError> {
    match config.mode {
      ProcessingMode::Bypass => self.start_bypass(&config.microphone_endpoint_id),
      ProcessingMode::Aec => {
        let render_endpoint_id = config.render_endpoint_id.as_deref().ok_or_else(|| {
          EngineError::new(
            EngineErrorKind::InvalidConfiguration,
            "an exact physical render endpoint ID is required for AEC mode",
          )
        })?;
        self.start_aec(&config.microphone_endpoint_id, render_endpoint_id)
      }
    }
  }

  #[allow(
    clippy::too_many_lines,
    reason = "bypass startup keeps source and sink readiness plus paired cleanup visible"
  )]
  fn start_bypass(&self, source_endpoint_id: &str) -> Result<(), EngineError> {
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
      EngineState::RunningBypass
      | EngineState::RunningAec
      | EngineState::Degraded
      | EngineState::Starting
      | EngineState::Stopping => {
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
      .resolve(InputRole::Microphone, source_endpoint_id.trim())
      .map_err(|error| EngineError::from_source(InputRole::Microphone, error))?;
    validate_source(&source, InputRole::Microphone, source_endpoint_id.trim())?;

    let run_id = new_identity();
    let session_value = new_identity();
    let session_id = SessionId::new(session_value).expect("generated identity is nonzero");
    {
      let mut snapshot = self.shared.snapshot.lock().expect("engine snapshot lock");
      *snapshot = EngineSnapshot {
        state: EngineState::Starting,
        mode: Some(ProcessingMode::Bypass),
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
      queue: Some(Arc::clone(&queue)),
      microphone_queue: None,
      render_queue: None,
      capture: None,
      render: None,
      sink: Some(sink),
      processing: None,
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
      InputRole::Microphone,
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

  #[allow(
    clippy::too_many_lines,
    reason = "AEC startup keeps two source roles, processor readiness and paired cleanup visible"
  )]
  fn start_aec(
    &self,
    microphone_endpoint_id: &str,
    render_endpoint_id: &str,
  ) -> Result<(), EngineError> {
    if microphone_endpoint_id.trim().is_empty() || render_endpoint_id.trim().is_empty() {
      return Err(EngineError::new(
        EngineErrorKind::InvalidConfiguration,
        "non-empty physical microphone and render endpoint IDs are required",
      ));
    }
    let echo_factory = self.echo_factory.clone().ok_or_else(|| {
      EngineError::new(
        EngineErrorKind::InvalidConfiguration,
        "AEC mode requires a configured project-owned echo-canceller factory",
      )
    })?;

    let state = self
      .shared
      .snapshot
      .lock()
      .expect("engine snapshot lock")
      .state;
    match state {
      EngineState::RunningBypass
      | EngineState::RunningAec
      | EngineState::Degraded
      | EngineState::Starting
      | EngineState::Stopping => {
        return Err(EngineError::new(
          EngineErrorKind::InvalidConfiguration,
          format!("cannot start the engine while it is {state:?}"),
        ));
      }
      EngineState::Failed => self.cleanup_previous_run()?,
      EngineState::Stopped => {}
    }

    let microphone = self
      .source_factory
      .resolve(InputRole::Microphone, microphone_endpoint_id.trim())
      .map_err(|error| EngineError::from_source(InputRole::Microphone, error))?;
    validate_source(
      &microphone,
      InputRole::Microphone,
      microphone_endpoint_id.trim(),
    )?;
    let render = self
      .source_factory
      .resolve(InputRole::RenderLoopback, render_endpoint_id.trim())
      .map_err(|error| EngineError::from_source(InputRole::RenderLoopback, error))?;
    validate_source(
      &render,
      InputRole::RenderLoopback,
      render_endpoint_id.trim(),
    )?;

    let run_id = new_identity();
    let session_value = new_identity();
    let aec_instance_id = new_identity();
    let session_id = SessionId::new(session_value).expect("generated identity is nonzero");
    {
      let mut snapshot = self.shared.snapshot.lock().expect("engine snapshot lock");
      *snapshot = EngineSnapshot {
        state: EngineState::Starting,
        mode: Some(ProcessingMode::Aec),
        source: Some(microphone.clone()),
        render_source: Some(render.clone()),
        run_id: Some(run_id),
        session_id: Some(session_value),
        aec_instance_id: Some(aec_instance_id),
        synchronization_epoch: 1,
        ..EngineSnapshot::default()
      };
    }
    self.shared.stop_requested.store(false, Ordering::Release);
    self.shared.terminal_failure.store(false, Ordering::Release);

    let microphone_queue = Arc::new(TimedFrameQueue::new());
    let render_queue = Arc::new(TimedFrameQueue::new());
    let mut handles = RunHandles {
      queue: None,
      microphone_queue: Some(Arc::clone(&microphone_queue)),
      render_queue: Some(Arc::clone(&render_queue)),
      capture: None,
      render: None,
      sink: None,
      processing: None,
    };

    let (microphone_ready_tx, microphone_ready_rx) = mpsc::sync_channel(1);
    handles.capture = Some(spawn_aec_input_worker(
      Arc::clone(&self.source_factory),
      InputRole::Microphone,
      microphone,
      Arc::clone(&microphone_queue),
      Arc::clone(&render_queue),
      Arc::clone(&self.shared),
      microphone_ready_tx,
    )?);

    let (render_ready_tx, render_ready_rx) = mpsc::sync_channel(1);
    handles.render = Some(spawn_aec_input_worker(
      Arc::clone(&self.source_factory),
      InputRole::RenderLoopback,
      render,
      Arc::clone(&render_queue),
      Arc::clone(&microphone_queue),
      Arc::clone(&self.shared),
      render_ready_tx,
    )?);

    let (processing_ready_tx, processing_ready_rx) = mpsc::sync_channel(1);
    handles.processing = Some(spawn_aec_processing_worker(
      Arc::clone(&self.sink_factory),
      echo_factory,
      session_id,
      Arc::clone(&microphone_queue),
      Arc::clone(&render_queue),
      Arc::clone(&self.shared),
      processing_ready_tx,
    )?);
    *self.run.lock().expect("engine run lock") = Some(handles);

    for (receiver, name) in [
      (&microphone_ready_rx, "physical microphone"),
      (&render_ready_rx, "physical render loopback"),
      (&processing_ready_rx, "AEC processing timeline"),
    ] {
      if let Err(error) = wait_ready(receiver, name) {
        self.shared.record_aec_failure(
          error.clone(),
          microphone_queue.as_ref(),
          render_queue.as_ref(),
        );
        self.cleanup_previous_run()?;
        return Err(error);
      }
    }

    let mut snapshot = self.shared.snapshot.lock().expect("engine snapshot lock");
    if self.shared.terminal_failure.load(Ordering::Acquire) {
      return Err(snapshot.last_error.clone().unwrap_or_else(|| {
        EngineError::new(EngineErrorKind::WorkerFailure, "AEC startup worker failed")
      }));
    }
    if snapshot.state == EngineState::Starting {
      snapshot.state = EngineState::RunningAec;
    }
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
      if let Some(queue) = &run.queue {
        let metrics = queue.metrics();
        snapshot.queue_depth = metrics.depth;
        snapshot.queue_high_water = metrics.high_water;
        snapshot.queue_overflows = metrics.overflows;
        snapshot.discarded_frames = metrics.discarded;
      }
      if let Some(queue) = &run.microphone_queue {
        let metrics = queue.metrics();
        snapshot.queue_depth = metrics.depth;
        snapshot.queue_high_water = metrics.high_water;
        snapshot.queue_overflows = metrics.overflows;
        snapshot.discarded_frames = metrics.discarded;
      }
      if let Some(queue) = &run.render_queue {
        let metrics = queue.metrics();
        snapshot.render_queue_depth = metrics.depth;
        snapshot.render_queue_high_water = metrics.high_water;
        snapshot.render_queue_overflows = metrics.overflows;
        snapshot.render_discarded_frames = metrics.discarded;
      }
    }
    snapshot
  }

  fn cleanup_previous_run(&self) -> Result<(), EngineError> {
    self.shared.stop_requested.store(true, Ordering::Release);
    let Some(mut handles) = self.run.lock().expect("engine run lock").take() else {
      return Ok(());
    };
    join_run(&mut handles)?;
    if let Some(queue) = &handles.queue {
      let metrics = queue.clear();
      self.shared.update_queue(metrics);
    }
    if let Some(queue) = &handles.microphone_queue {
      queue.clear();
    }
    if let Some(queue) = &handles.render_queue {
      queue.clear();
    }
    Ok(())
  }
}

impl Drop for Engine {
  fn drop(&mut self) {
    let _ = self.stop();
  }
}

fn validate_source(
  source: &SourceDescriptor,
  role: InputRole,
  requested_id: &str,
) -> Result<(), EngineError> {
  if source.endpoint_id != requested_id {
    return Err(EngineError::new(
      EngineErrorKind::InvalidSource,
      "the capture adapter did not resolve the exact requested endpoint ID",
    ));
  }
  if source.role != role {
    return Err(EngineError::new(
      EngineErrorKind::InvalidSource,
      format!(
        "the endpoint adapter resolved {role:?} as {:?}",
        source.role
      ),
    ));
  }
  if !source.active {
    return Err(EngineError::new(
      EngineErrorKind::SourceUnavailable,
      format!("capture endpoint {:?} is not active", source.friendly_name),
    ));
  }
  if role == InputRole::Microphone && source_is_public_endpoint(source) {
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
  role: InputRole,
  source: SourceDescriptor,
  queue: Arc<FrameQueue>,
  shared: Arc<Shared>,
  ready: mpsc::SyncSender<Result<(), EngineError>>,
) -> Result<JoinHandle<()>, EngineError> {
  thread::Builder::new()
    .name("mini-aec-capture".to_owned())
    .spawn(move || capture_worker(factory.as_ref(), role, &source, &queue, &shared, &ready))
    .map_err(|error| {
      EngineError::new(
        EngineErrorKind::WorkerFailure,
        format!("failed to spawn physical capture worker: {error}"),
      )
    })
}

fn capture_worker(
  factory: &dyn AudioInputFactory,
  role: InputRole,
  source: &SourceDescriptor,
  queue: &FrameQueue,
  shared: &Shared,
  ready: &mpsc::SyncSender<Result<(), EngineError>>,
) {
  let mut input = match factory.open(role, source) {
    Ok(input) => input,
    Err(error) => {
      let error = EngineError::from_source(role, error);
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
        shared.record_failure(EngineError::from_source(role, error), queue);
        break;
      }
    }
  }
  accumulator.clear();
  if let Err(error) = input.stop() {
    if !shared.terminal_failure.load(Ordering::Acquire) {
      shared.record_failure(EngineError::from_source(role, error), queue);
    }
  }
  queue.finish_producer();
}

fn spawn_aec_input_worker(
  factory: Arc<dyn AudioInputFactory>,
  role: InputRole,
  source: SourceDescriptor,
  queue: Arc<TimedFrameQueue>,
  peer_queue: Arc<TimedFrameQueue>,
  shared: Arc<Shared>,
  ready: mpsc::SyncSender<Result<(), EngineError>>,
) -> Result<JoinHandle<()>, EngineError> {
  let name = match role {
    InputRole::Microphone => "mini-aec-microphone",
    InputRole::RenderLoopback => "mini-aec-render",
  };
  thread::Builder::new()
    .name(name.to_owned())
    .spawn(move || {
      aec_input_worker(
        factory.as_ref(),
        role,
        &source,
        &queue,
        &peer_queue,
        &shared,
        &ready,
      );
    })
    .map_err(|error| {
      EngineError::new(
        EngineErrorKind::WorkerFailure,
        format!("failed to spawn {role:?} worker: {error}"),
      )
    })
}

fn aec_input_worker(
  factory: &dyn AudioInputFactory,
  role: InputRole,
  source: &SourceDescriptor,
  queue: &TimedFrameQueue,
  peer_queue: &TimedFrameQueue,
  shared: &Shared,
  ready: &mpsc::SyncSender<Result<(), EngineError>>,
) {
  let mut input = match factory.open(role, source) {
    Ok(input) => input,
    Err(error) => {
      let error = EngineError::from_source(role, error);
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
  let mut accumulator = TimedFrameAccumulator::default();
  while !shared.stop_requested.load(Ordering::Acquire) {
    match input.read_packet(&mut packet, INPUT_WAIT) {
      Ok(Some(metadata)) => {
        if metadata.frames > packet.len() {
          shared.record_aec_failure(
            EngineError::new(
              match role {
                InputRole::Microphone => EngineErrorKind::SourceFailure,
                InputRole::RenderLoopback => EngineErrorKind::RenderFailure,
              },
              format!(
                "{role:?} packet contains {} frames; preallocated capacity is {}",
                metadata.frames,
                packet.len()
              ),
            ),
            queue,
            peer_queue,
          );
          break;
        }
        update_input_packet_snapshot(shared, role, metadata);
        let sanitized = accumulator.push_packet(&packet, metadata, |frame| {
          let metrics = queue.push(frame);
          update_input_frame_snapshot(shared, role, metrics);
        });
        let mut snapshot = shared.snapshot.lock().expect("engine snapshot lock");
        match role {
          InputRole::Microphone => snapshot.sanitized_samples += sanitized,
          InputRole::RenderLoopback => snapshot.render_sanitized_samples += sanitized,
        }
      }
      Ok(None) => {}
      Err(error) => {
        shared.record_aec_failure(EngineError::from_source(role, error), queue, peer_queue);
        break;
      }
    }
  }
  accumulator.clear();
  if let Err(error) = input.stop() {
    if !shared.terminal_failure.load(Ordering::Acquire) {
      shared.record_aec_failure(EngineError::from_source(role, error), queue, peer_queue);
    }
  }
  queue.finish_producer();
}

fn update_input_packet_snapshot(shared: &Shared, role: InputRole, metadata: crate::PacketMetadata) {
  let mut snapshot = shared.snapshot.lock().expect("engine snapshot lock");
  match role {
    InputRole::Microphone => {
      snapshot.captured_packets += 1;
      snapshot.captured_samples += metadata.frames as u64;
      snapshot.silent_packets += u64::from(metadata.silent);
      snapshot.discontinuities += u64::from(metadata.data_discontinuity);
      snapshot.timestamp_errors += u64::from(metadata.timestamp_error);
      snapshot.last_device_position = Some(metadata.device_position);
      snapshot.last_qpc_timestamp_100ns = Some(metadata.qpc_timestamp_100ns);
    }
    InputRole::RenderLoopback => {
      snapshot.render_packets += 1;
      snapshot.render_samples += metadata.frames as u64;
      snapshot.render_silent_packets += u64::from(metadata.silent);
      snapshot.render_discontinuities += u64::from(metadata.data_discontinuity);
      snapshot.render_timestamp_errors += u64::from(metadata.timestamp_error);
      snapshot.last_render_device_position = Some(metadata.device_position);
      snapshot.last_render_qpc_timestamp_100ns = Some(metadata.qpc_timestamp_100ns);
    }
  }
}

fn update_input_frame_snapshot(
  shared: &Shared,
  role: InputRole,
  metrics: crate::synchronization::TimedQueueMetrics,
) {
  let mut snapshot = shared.snapshot.lock().expect("engine snapshot lock");
  match role {
    InputRole::Microphone => {
      snapshot.output_frames += 1;
      snapshot.queue_depth = metrics.depth;
      snapshot.queue_high_water = metrics.high_water;
      snapshot.queue_overflows = metrics.overflows;
      snapshot.discarded_frames = metrics.discarded;
    }
    InputRole::RenderLoopback => {
      snapshot.render_frames += 1;
      snapshot.render_queue_depth = metrics.depth;
      snapshot.render_queue_high_water = metrics.high_water;
      snapshot.render_queue_overflows = metrics.overflows;
      snapshot.render_discarded_frames = metrics.discarded;
    }
  }
}

fn spawn_aec_processing_worker(
  sink_factory: Arc<dyn VirtualSinkFactory>,
  echo_factory: Arc<dyn EchoCancellerFactory>,
  session_id: SessionId,
  microphone_queue: Arc<TimedFrameQueue>,
  render_queue: Arc<TimedFrameQueue>,
  shared: Arc<Shared>,
  ready: mpsc::SyncSender<Result<(), EngineError>>,
) -> Result<JoinHandle<()>, EngineError> {
  thread::Builder::new()
    .name("mini-aec-processing".to_owned())
    .spawn(move || {
      aec_processing_worker(
        sink_factory.as_ref(),
        echo_factory.as_ref(),
        session_id,
        &microphone_queue,
        &render_queue,
        &shared,
        &ready,
      );
    })
    .map_err(|error| {
      EngineError::new(
        EngineErrorKind::WorkerFailure,
        format!("failed to spawn AEC processing worker: {error}"),
      )
    })
}

#[allow(
  clippy::too_many_lines,
  reason = "the real-time processing loop keeps pairing, recovery, sink sequencing and cleanup together"
)]
fn aec_processing_worker(
  sink_factory: &dyn VirtualSinkFactory,
  echo_factory: &dyn EchoCancellerFactory,
  session_id: SessionId,
  microphone_queue: &TimedFrameQueue,
  render_queue: &TimedFrameQueue,
  shared: &Shared,
  ready: &mpsc::SyncSender<Result<(), EngineError>>,
) {
  let mut sink = match sink_factory.connect() {
    Ok(sink) => sink,
    Err(error) => {
      let _ = ready.send(Err(EngineError::from_sink(&error)));
      return;
    }
  };
  if let Err(error) = sink.open_session(SessionConfig::new(session_id)) {
    let _ = ready.send(Err(EngineError::from_sink(&error)));
    return;
  }
  if let Err(error) = refresh_sink_diagnostics(sink.as_ref(), shared) {
    let _ = sink.close_session();
    let _ = ready.send(Err(error));
    return;
  }
  let mut echo = match echo_factory.create() {
    Ok(echo) => echo,
    Err(error) => {
      let _ = sink.close_session();
      let _ = ready.send(Err(EngineError::new(
        EngineErrorKind::EchoCancellerFailure,
        error.to_string(),
      )));
      return;
    }
  };

  let first_microphone = match microphone_queue.pop(START_TIMEOUT) {
    TimedPopResult::Frame(frame) => frame,
    TimedPopResult::Timeout | TimedPopResult::Finished => {
      let _ = sink.close_session();
      let _ = ready.send(Err(EngineError::new(
        EngineErrorKind::SynchronizationFailure,
        "microphone did not establish the AEC timeline",
      )));
      return;
    }
  };
  let first_render = match render_queue.pop(RENDER_WAIT) {
    TimedPopResult::Frame(frame) => Some(frame),
    TimedPopResult::Finished if shared.terminal_failure.load(Ordering::Acquire) => {
      let error = shared
        .snapshot
        .lock()
        .expect("engine snapshot lock")
        .last_error
        .clone()
        .unwrap_or_else(|| {
          EngineError::new(
            EngineErrorKind::RenderFailure,
            "render loopback failed before the AEC timeline started",
          )
        });
      let _ = sink.close_session();
      let _ = ready.send(Err(error));
      return;
    }
    TimedPopResult::Timeout | TimedPopResult::Finished => None,
  };
  {
    let mut snapshot = shared.snapshot.lock().expect("engine snapshot lock");
    snapshot.synchronization_origin_qpc_100ns = Some(first_render.map_or(
      first_microphone.qpc_timestamp_100ns,
      |render| {
        first_microphone
          .qpc_timestamp_100ns
          .max(render.qpc_timestamp_100ns)
      },
    ));
  }
  if ready.send(Ok(())).is_err() {
    let _ = sink.close_session();
    return;
  }

  let mut microphone = Some(first_microphone);
  let mut render = first_render;
  let mut sequence = 0_u64;
  let mut consecutive_skew_misses = 0_u32;
  let mut consecutive_aec_failures = 0_u32;
  let mut healthy_frames = 0_u32;
  let mut processing_histogram = ProcessingHistogram::default();
  while !shared.stop_requested.load(Ordering::Acquire) {
    let microphone_frame = if let Some(frame) = microphone.take() {
      frame
    } else {
      match microphone_queue.pop(INPUT_WAIT) {
        TimedPopResult::Frame(frame) => frame,
        TimedPopResult::Timeout => continue,
        TimedPopResult::Finished => break,
      }
    };

    if microphone_frame.reset_epoch {
      if let Err(error) = rebuild_echo(
        echo_factory,
        &mut echo,
        shared,
        DegradationReason::AecReset,
        true,
      ) {
        shared.record_aec_failure(error, microphone_queue, render_queue);
        break;
      }
    }

    let (render_samples, paired, delta) = match select_render_frame(
      &microphone_frame,
      &mut render,
      render_queue,
      shared,
      echo_factory,
      &mut echo,
    ) {
      Ok(selection) => selection,
      Err(error) => {
        shared.record_aec_failure(error, microphone_queue, render_queue);
        break;
      }
    };
    {
      let mut snapshot = shared.snapshot.lock().expect("engine snapshot lock");
      snapshot.last_device_position = Some(microphone_frame.device_position);
      snapshot.current_delta_100ns = delta;
      if let Some(delta) = delta {
        snapshot.maximum_absolute_skew_100ns = snapshot
          .maximum_absolute_skew_100ns
          .max(delta.unsigned_abs());
      }
      if paired {
        snapshot.paired_frames += 1;
      } else {
        snapshot.silent_render_references += 1;
      }
    }

    if paired {
      consecutive_skew_misses = 0;
      healthy_frames = healthy_frames.saturating_add(1);
      if healthy_frames >= HEALTHY_RECOVERY_FRAMES {
        let mut snapshot = shared.snapshot.lock().expect("engine snapshot lock");
        snapshot.state = EngineState::RunningAec;
        snapshot.degradation_reason = None;
      }
    } else {
      if delta.is_some() {
        consecutive_skew_misses = consecutive_skew_misses.saturating_add(1);
      } else {
        consecutive_skew_misses = 0;
      }
      healthy_frames = 0;
      let mut snapshot = shared.snapshot.lock().expect("engine snapshot lock");
      snapshot.state = EngineState::Degraded;
      snapshot.degradation_reason = Some(DegradationReason::RenderReferenceMissing);
    }
    if consecutive_skew_misses >= MAX_CONSECUTIVE_RENDER_MISSES {
      shared.record_aec_failure(
        EngineError::new(
          EngineErrorKind::SynchronizationFailure,
          "render and microphone timestamps remained unpairable beyond the bounded M3 skew window",
        ),
        microphone_queue,
        render_queue,
      );
      break;
    }

    let started = Instant::now();
    let mut output = [0.0_f32; mini_aec_transport::FRAME_SAMPLES];
    let process_result = echo.process(&render_samples, &microphone_frame.samples, &mut output);
    let invalid_output = output.iter().any(|sample| !sample.is_finite());
    if process_result.is_err() || invalid_output {
      consecutive_aec_failures = consecutive_aec_failures.saturating_add(1);
      healthy_frames = 0;
      output.fill(0.0);
      {
        let mut snapshot = shared.snapshot.lock().expect("engine snapshot lock");
        snapshot.aec_invalid_outputs += u64::from(invalid_output);
        snapshot.state = EngineState::Degraded;
        snapshot.degradation_reason = Some(DegradationReason::AecReset);
      }
      if consecutive_aec_failures >= MAX_CONSECUTIVE_AEC_FAILURES {
        shared.record_aec_failure(
          EngineError::new(
            EngineErrorKind::EchoCancellerFailure,
            "echo canceller repeatedly failed after bounded reconstruction attempts",
          ),
          microphone_queue,
          render_queue,
        );
        break;
      }
      if let Err(error) = rebuild_echo(
        echo_factory,
        &mut echo,
        shared,
        DegradationReason::AecReset,
        false,
      ) {
        shared.record_aec_failure(error, microphone_queue, render_queue);
        break;
      }
    } else {
      consecutive_aec_failures = 0;
      shared
        .snapshot
        .lock()
        .expect("engine snapshot lock")
        .aec_processed_frames += 1;
    }

    let elapsed = started.elapsed();
    processing_histogram.record(elapsed);
    {
      let mut snapshot = shared.snapshot.lock().expect("engine snapshot lock");
      snapshot.processing_time = processing_histogram.snapshot();
      if elapsed > PROCESSING_BUDGET {
        snapshot.processing_deadline_misses += 1;
        snapshot.state = EngineState::Degraded;
        snapshot.degradation_reason = Some(DegradationReason::ProcessingDeadline);
      }
    }

    let mut pcm = [0_i16; mini_aec_transport::FRAME_SAMPLES];
    for (destination, sample) in pcm.iter_mut().zip(output) {
      *destination = crate::framing::pcm16(sample).0;
    }
    if let Err(error) = sink.write_frame(PcmFrame::new(sequence, &pcm)) {
      shared
        .snapshot
        .lock()
        .expect("engine snapshot lock")
        .sink_failures += 1;
      shared.record_aec_failure(
        EngineError::from_sink(&error),
        microphone_queue,
        render_queue,
      );
      break;
    }
    sequence = sequence.saturating_add(1);
    shared
      .snapshot
      .lock()
      .expect("engine snapshot lock")
      .sink_accepted_frames += 1;
    if sequence.is_multiple_of(100) {
      if let Err(error) = refresh_sink_diagnostics(sink.as_ref(), shared) {
        shared.record_aec_failure(error, microphone_queue, render_queue);
        break;
      }
    }
  }

  let _ = refresh_sink_diagnostics(sink.as_ref(), shared);
  if let Err(error) = sink.close_session() {
    if !shared.terminal_failure.load(Ordering::Acquire) {
      shared.record_aec_failure(
        EngineError::from_sink(&error),
        microphone_queue,
        render_queue,
      );
    }
  }
}

fn select_render_frame(
  microphone: &TimedFrame,
  render: &mut Option<TimedFrame>,
  render_queue: &TimedFrameQueue,
  shared: &Shared,
  echo_factory: &dyn EchoCancellerFactory,
  echo: &mut Box<dyn crate::EchoCanceller>,
) -> Result<([f32; mini_aec_transport::FRAME_SAMPLES], bool, Option<i64>), EngineError> {
  if render.is_none() {
    *render = match render_queue.pop(RENDER_WAIT) {
      TimedPopResult::Frame(frame) => Some(frame),
      TimedPopResult::Timeout | TimedPopResult::Finished => None,
    };
  }
  while let Some(candidate) = *render {
    if candidate.reset_epoch {
      rebuild_echo(
        echo_factory,
        echo,
        shared,
        DegradationReason::AecReset,
        true,
      )?;
    }
    let delta = signed_delta(
      candidate.qpc_timestamp_100ns,
      microphone.qpc_timestamp_100ns,
    );
    if delta < -ALIGNMENT_TOLERANCE_100NS.cast_signed() {
      shared
        .snapshot
        .lock()
        .expect("engine snapshot lock")
        .stale_render_frames += 1;
      *render = render_queue.try_pop();
      continue;
    }
    if delta.unsigned_abs() > MAXIMUM_SKEW_100NS {
      return Ok(([0.0; mini_aec_transport::FRAME_SAMPLES], false, Some(delta)));
    }
    if delta <= ALIGNMENT_TOLERANCE_100NS.cast_signed() {
      *render = render_queue.try_pop();
      return Ok((candidate.samples, true, Some(delta)));
    }
    return Ok(([0.0; mini_aec_transport::FRAME_SAMPLES], false, Some(delta)));
  }
  Ok(([0.0; mini_aec_transport::FRAME_SAMPLES], false, None))
}

fn rebuild_echo(
  factory: &dyn EchoCancellerFactory,
  echo: &mut Box<dyn crate::EchoCanceller>,
  shared: &Shared,
  reason: DegradationReason,
  alignment_reset: bool,
) -> Result<(), EngineError> {
  let rebuilt = factory.create().map_err(|error| {
    EngineError::new(
      EngineErrorKind::EchoCancellerFailure,
      format!("failed to reconstruct the echo canceller: {error}"),
    )
  })?;
  *echo = rebuilt;
  let mut snapshot = shared.snapshot.lock().expect("engine snapshot lock");
  snapshot.aec_resets += 1;
  snapshot.aec_rebuilds += 1;
  snapshot.aec_instance_id = Some(new_identity());
  if alignment_reset {
    snapshot.alignment_resets += 1;
    snapshot.synchronization_epoch = snapshot.synchronization_epoch.saturating_add(1);
  }
  snapshot.state = EngineState::Degraded;
  snapshot.degradation_reason = Some(reason);
  Ok(())
}

fn signed_delta(left: u64, right: u64) -> i64 {
  let delta = i128::from(left) - i128::from(right);
  i64::try_from(delta).unwrap_or_else(|_| {
    if delta.is_negative() {
      i64::MIN
    } else {
      i64::MAX
    }
  })
}

#[derive(Default)]
struct ProcessingHistogram {
  buckets: [u64; 8],
  samples: u64,
  maximum_us: u64,
}

impl ProcessingHistogram {
  const LIMITS_US: [u64; 8] = [100, 250, 500, 1_000, 2_000, 5_000, 10_000, u64::MAX];

  fn record(&mut self, duration: Duration) {
    let micros = u64::try_from(duration.as_micros()).unwrap_or(u64::MAX);
    let index = Self::LIMITS_US
      .iter()
      .position(|limit| micros <= *limit)
      .unwrap_or(Self::LIMITS_US.len() - 1);
    self.buckets[index] += 1;
    self.samples += 1;
    self.maximum_us = self.maximum_us.max(micros);
  }

  fn snapshot(&self) -> ProcessingTimeSnapshot {
    ProcessingTimeSnapshot {
      samples: self.samples,
      p50_us: self.percentile(50),
      p95_us: self.percentile(95),
      p99_us: self.percentile(99),
      maximum_us: self.maximum_us,
    }
  }

  fn percentile(&self, percentile: u64) -> u64 {
    if self.samples == 0 {
      return 0;
    }
    let target = self.samples.saturating_mul(percentile).div_ceil(100);
    let mut cumulative = 0_u64;
    for (count, limit) in self.buckets.iter().zip(Self::LIMITS_US) {
      cumulative += count;
      if cumulative >= target {
        return limit.min(self.maximum_us.max(1));
      }
    }
    self.maximum_us
  }
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
      if let Some(queue) = &handles.queue {
        queue.fail();
      }
      if let Some(queue) = &handles.microphone_queue {
        queue.fail();
      }
      first_error = Some(error);
    }
  } else if let Some(queue) = &handles.queue {
    queue.finish_producer();
  }
  if let Some(render) = handles.render.take() {
    if let Err(error) = join_finite(render, "render") {
      if let Some(queue) = &handles.render_queue {
        queue.fail();
      }
      first_error.get_or_insert(error);
    }
  }
  if let Some(sink) = handles.sink.take() {
    if let Err(error) = join_finite(sink, "sink") {
      first_error.get_or_insert(error);
    }
  }
  if let Some(processing) = handles.processing.take() {
    if let Err(error) = join_finite(processing, "processing") {
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
  use crate::test_support::{
    FakeAudioInputFactory, FakeEchoCancellerFactory, FakeEchoPlan, FakeSinkFactory, FakeSinkPlan,
    SourceStep,
  };
  use crate::{
    EngineConfig, EngineErrorKind, EngineState, InputRole, PacketMetadata, SourceDescriptor,
    SourceError, SourceErrorKind,
  };

  fn descriptor() -> SourceDescriptor {
    SourceDescriptor {
      role: InputRole::Microphone,
      endpoint_id: "physical-id".to_owned(),
      friendly_name: "Synthetic Microphone".to_owned(),
      active: true,
      native_format: None,
    }
  }

  fn render_descriptor() -> SourceDescriptor {
    SourceDescriptor {
      role: InputRole::RenderLoopback,
      endpoint_id: "render-id".to_owned(),
      friendly_name: "Synthetic Speakers".to_owned(),
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

  fn aec_engine(
    source: &FakeAudioInputFactory,
    sink: &FakeSinkFactory,
    echo: &FakeEchoCancellerFactory,
  ) -> Engine {
    Engine::new_with_aec(
      Arc::new(source.clone()),
      Arc::new(sink.clone()),
      Arc::new(echo.clone()),
    )
  }

  fn packet_at(value: f32, qpc_timestamp_100ns: u64) -> SourceStep {
    SourceStep::Packet {
      samples: vec![value; FRAME_SAMPLES],
      metadata: PacketMetadata {
        frames: FRAME_SAMPLES,
        qpc_timestamp_100ns,
        ..PacketMetadata::default()
      },
    }
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

  #[test]
  fn healthy_aec_uses_explicit_roles_and_render_first_processing() {
    let source = FakeAudioInputFactory::new(descriptor()).with_render(render_descriptor());
    source.push_run([packet_at(0.25, 1_000_000), packet_at(0.5, 1_100_000)]);
    source.push_render_run([packet_at(0.125, 1_000_000), packet_at(0.25, 1_100_000)]);
    let sink = FakeSinkFactory::default();
    let echo = FakeEchoCancellerFactory::default();
    let engine = aec_engine(&source, &sink, &echo);

    engine
      .start(EngineConfig::aec("physical-id", "render-id"))
      .expect("AEC starts");
    assert_eq!(engine.snapshot().state, EngineState::RunningAec);
    wait_for(|| engine.snapshot().sink_accepted_frames == 2);
    engine.stop().expect("AEC stops");

    let snapshot = engine.snapshot();
    assert_eq!(snapshot.paired_frames, 2);
    assert_eq!(snapshot.aec_processed_frames, 2);
    assert_eq!(snapshot.silent_render_references, 0);
    let records = echo.records();
    assert_eq!(records[0].render_first_samples, vec![125, 250]);
    assert_eq!(records[0].capture_first_samples, vec![250, 500]);
  }

  #[test]
  fn missing_render_is_visible_degradation_not_bypass() {
    let source = FakeAudioInputFactory::new(descriptor()).with_render(render_descriptor());
    source.push_run((0..4).map(|index| packet_at(0.25, 1_000_000 + index * 100_000)));
    source.push_render_run([packet_at(0.125, 1_000_000)]);
    let sink = FakeSinkFactory::default();
    let echo = FakeEchoCancellerFactory::default();
    let engine = aec_engine(&source, &sink, &echo);

    engine
      .start(EngineConfig::aec("physical-id", "render-id"))
      .expect("AEC starts");
    wait_for(|| engine.snapshot().silent_render_references >= 1);
    let snapshot = engine.snapshot();
    assert_eq!(snapshot.state, EngineState::Degraded);
    assert!(snapshot.silent_render_references >= 1);
    engine.stop().expect("degraded AEC stops");
  }

  #[test]
  fn invalid_aec_output_is_silenced_and_reconstructed() {
    let source = FakeAudioInputFactory::new(descriptor()).with_render(render_descriptor());
    source.push_run([packet_at(0.5, 1_000_000), packet_at(0.5, 1_100_000)]);
    source.push_render_run([packet_at(0.25, 1_000_000), packet_at(0.25, 1_100_000)]);
    let sink = FakeSinkFactory::default();
    let echo = FakeEchoCancellerFactory::default();
    echo.push_plan(FakeEchoPlan {
      non_finite_at: Some(0),
      ..FakeEchoPlan::default()
    });
    let engine = aec_engine(&source, &sink, &echo);

    engine
      .start(EngineConfig::aec("physical-id", "render-id"))
      .expect("AEC starts");
    wait_for(|| engine.snapshot().sink_accepted_frames == 2);
    engine.stop().expect("AEC stops");
    let snapshot = engine.snapshot();
    assert_eq!(snapshot.aec_invalid_outputs, 1);
    assert_eq!(snapshot.aec_rebuilds, 1);
    assert_eq!(sink.records()[0].accepted[0].1, 0);
  }

  #[test]
  fn reconstructed_aec_requires_healthy_recovery_gate() {
    let source = FakeAudioInputFactory::new(descriptor()).with_render(render_descriptor());
    let mut microphone_steps = Vec::new();
    let mut render_steps = Vec::new();
    for index in 0..12 {
      microphone_steps.push(packet_at(0.5, 1_000_000 + index * 100_000));
      microphone_steps.push(SourceStep::Delay(Duration::from_millis(2)));
      render_steps.push(packet_at(0.25, 1_000_000 + index * 100_000));
      render_steps.push(SourceStep::Delay(Duration::from_millis(2)));
    }
    source.push_run(microphone_steps);
    source.push_render_run(render_steps);
    let sink = FakeSinkFactory::default();
    let echo = FakeEchoCancellerFactory::default();
    echo.push_plan(FakeEchoPlan {
      non_finite_at: Some(0),
      ..FakeEchoPlan::default()
    });
    let engine = aec_engine(&source, &sink, &echo);

    engine
      .start(EngineConfig::aec("physical-id", "render-id"))
      .expect("AEC starts");
    wait_for(|| engine.snapshot().sink_accepted_frames == 12);
    let snapshot = engine.snapshot();
    assert_eq!(snapshot.state, EngineState::RunningAec);
    assert_eq!(snapshot.aec_rebuilds, 1);
    assert_eq!(snapshot.aec_processed_frames, 11);
    engine.stop().expect("recovered AEC stops");
  }

  #[test]
  fn repeated_aec_failures_exhaust_bounded_reconstruction() {
    let source = FakeAudioInputFactory::new(descriptor()).with_render(render_descriptor());
    source.push_run((0..4).map(|index| packet_at(0.5, 1_000_000 + index * 100_000)));
    source.push_render_run((0..4).map(|index| packet_at(0.25, 1_000_000 + index * 100_000)));
    let sink = FakeSinkFactory::default();
    let echo = FakeEchoCancellerFactory::default();
    for _ in 0..3 {
      echo.push_plan(FakeEchoPlan {
        fail_at: Some(0),
        ..FakeEchoPlan::default()
      });
    }
    let engine = aec_engine(&source, &sink, &echo);

    engine
      .start(EngineConfig::aec("physical-id", "render-id"))
      .expect("AEC timeline starts");
    wait_for(|| engine.snapshot().state == EngineState::Failed);
    let snapshot = engine.snapshot();
    assert_eq!(
      snapshot.last_error.as_ref().map(|error| error.kind),
      Some(EngineErrorKind::EchoCancellerFailure)
    );
    assert_eq!(snapshot.aec_rebuilds, 2);
    assert_eq!(sink.records()[0].accepted.len(), 2);
    assert!(sink.records()[0].accepted.iter().all(|frame| frame.1 == 0));
    engine.stop().expect("failed AEC cleanup succeeds");
  }

  #[test]
  fn sustained_render_silence_remains_degraded_without_bypass() {
    let source = FakeAudioInputFactory::new(descriptor()).with_render(render_descriptor());
    let mut microphone_steps = Vec::new();
    for index in 0..55 {
      microphone_steps.push(packet_at(0.25, 1_000_000 + index * 100_000));
      microphone_steps.push(SourceStep::Delay(Duration::from_millis(20)));
    }
    source.push_run(microphone_steps);
    source.push_render_run([packet_at(0.125, 1_000_000)]);
    let sink = FakeSinkFactory::default();
    let echo = FakeEchoCancellerFactory::default();
    let engine = aec_engine(&source, &sink, &echo);

    engine
      .start(EngineConfig::aec("physical-id", "render-id"))
      .expect("AEC timeline starts");
    let deadline = Instant::now() + Duration::from_secs(10);
    while engine.snapshot().sink_accepted_frames < 55 && Instant::now() < deadline {
      thread::sleep(Duration::from_millis(1));
    }
    let snapshot = engine.snapshot();
    assert_eq!(
      snapshot.sink_accepted_frames, 55,
      "sustained render-silence snapshot: {snapshot:#?}"
    );
    assert_eq!(snapshot.state, EngineState::Degraded);
    assert_eq!(snapshot.silent_render_references, 54);
    assert!(snapshot.last_error.is_none());
    engine.stop().expect("silent-render AEC cleanup succeeds");
  }

  #[test]
  fn aec_can_start_with_an_active_but_silent_render_endpoint() {
    let source = FakeAudioInputFactory::new(descriptor()).with_render(render_descriptor());
    source.push_run([
      packet_at(0.25, 1_000_000),
      packet_at(0.25, 1_100_000),
      SourceStep::Delay(Duration::from_millis(20)),
    ]);
    source.push_render_run(Vec::<SourceStep>::new());
    let sink = FakeSinkFactory::default();
    let echo = FakeEchoCancellerFactory::default();
    let engine = aec_engine(&source, &sink, &echo);

    engine
      .start(EngineConfig::aec("physical-id", "render-id"))
      .expect("silent render endpoint still establishes an AEC run");
    wait_for(|| engine.snapshot().sink_accepted_frames == 2);
    let snapshot = engine.snapshot();
    assert_eq!(snapshot.state, EngineState::Degraded);
    assert_eq!(snapshot.silent_render_references, 2);
    assert!(snapshot.last_error.is_none());
    engine.stop().expect("silent-start AEC cleanup succeeds");
  }

  #[test]
  fn sustained_timestamp_skew_is_terminal() {
    let source = FakeAudioInputFactory::new(descriptor()).with_render(render_descriptor());
    let mut microphone_steps = Vec::new();
    for index in 0..55 {
      microphone_steps.push(packet_at(0.25, 1_000_000 + index * 100_000));
      microphone_steps.push(SourceStep::Delay(Duration::from_millis(10)));
    }
    source.push_run(microphone_steps);
    source.push_render_run([packet_at(0.125, 100_000_000)]);
    let sink = FakeSinkFactory::default();
    let echo = FakeEchoCancellerFactory::default();
    let engine = aec_engine(&source, &sink, &echo);

    engine
      .start(EngineConfig::aec("physical-id", "render-id"))
      .expect("AEC workers start before sustained skew is observed");
    wait_for(|| engine.snapshot().state == EngineState::Failed);
    let snapshot = engine.snapshot();
    assert_eq!(
      snapshot.last_error.as_ref().map(|error| error.kind),
      Some(EngineErrorKind::SynchronizationFailure)
    );
    assert_eq!(snapshot.silent_render_references, 50);
    assert_eq!(snapshot.queue_depth, 0);
    assert_eq!(snapshot.render_queue_depth, 0);
    engine.stop().expect("failed-skew AEC cleanup succeeds");
  }

  #[test]
  fn aec_restart_uses_new_session_sequence_and_processing_instance() {
    let source = FakeAudioInputFactory::new(descriptor()).with_render(render_descriptor());
    source.push_run([
      packet_at(0.25, 1_000_000),
      SourceStep::Delay(Duration::from_millis(20)),
    ]);
    source.push_render_run([
      packet_at(0.125, 1_000_000),
      SourceStep::Delay(Duration::from_millis(20)),
    ]);
    source.push_run([
      packet_at(0.5, 2_000_000),
      SourceStep::Delay(Duration::from_millis(20)),
    ]);
    source.push_render_run([
      packet_at(0.25, 2_000_000),
      SourceStep::Delay(Duration::from_millis(20)),
    ]);
    let sink = FakeSinkFactory::default();
    let echo = FakeEchoCancellerFactory::default();
    let engine = aec_engine(&source, &sink, &echo);

    engine
      .start(EngineConfig::aec("physical-id", "render-id"))
      .expect("first AEC run starts");
    wait_for(|| engine.snapshot().sink_accepted_frames == 1);
    let first = engine.snapshot();
    engine.stop().expect("first AEC run stops");

    engine
      .start(EngineConfig::aec("physical-id", "render-id"))
      .expect("second AEC run starts");
    wait_for(|| engine.snapshot().sink_accepted_frames == 1);
    let second = engine.snapshot();
    engine.stop().expect("second AEC run stops");

    assert_ne!(first.run_id, second.run_id);
    assert_ne!(first.session_id, second.session_id);
    assert_ne!(first.aec_instance_id, second.aec_instance_id);
    let records = sink.records();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].accepted[0].0, 0);
    assert_eq!(records[1].accepted[0].0, 0);
    assert_eq!(records[0].accepted[0].1, 8_192);
    assert_eq!(records[1].accepted[0].1, 16_384);
  }

  #[test]
  fn wrong_role_render_descriptor_is_rejected_without_fallback() {
    let source = FakeAudioInputFactory::new(descriptor()).with_render(descriptor());
    let sink = FakeSinkFactory::default();
    let echo = FakeEchoCancellerFactory::default();
    let engine = aec_engine(&source, &sink, &echo);

    let error = engine
      .start(EngineConfig::aec("physical-id", "physical-id"))
      .expect_err("capture-role descriptor must not be accepted as render loopback");
    assert_eq!(error.kind, EngineErrorKind::InvalidSource);
    assert!(sink.records().is_empty());
  }

  #[test]
  fn render_invalidation_is_terminal_and_clears_both_queues() {
    let source = FakeAudioInputFactory::new(descriptor()).with_render(render_descriptor());
    source.push_run([SourceStep::RepeatedPacket {
      samples: vec![0.25; FRAME_SAMPLES],
      metadata: PacketMetadata {
        frames: FRAME_SAMPLES,
        qpc_timestamp_100ns: 1_000_000,
        ..PacketMetadata::default()
      },
      remaining: 100,
    }]);
    source.push_render_run([
      packet_at(0.125, 1_000_000),
      SourceStep::Failure(SourceError::new(
        SourceErrorKind::DeviceInvalidated,
        "render removed",
      )),
    ]);
    let sink = FakeSinkFactory::default();
    let echo = FakeEchoCancellerFactory::default();
    let engine = aec_engine(&source, &sink, &echo);

    let _ = engine.start(EngineConfig::aec("physical-id", "render-id"));
    wait_for(|| engine.snapshot().state == EngineState::Failed);
    let snapshot = engine.snapshot();
    assert_eq!(
      snapshot.last_error.as_ref().map(|error| error.kind),
      Some(EngineErrorKind::RenderInvalidated)
    );
    assert_eq!(snapshot.queue_depth, 0);
    assert_eq!(snapshot.render_queue_depth, 0);
    engine.stop().expect("failed AEC cleanup succeeds");
  }
}
