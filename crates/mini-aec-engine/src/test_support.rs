//! Deterministic synthetic adapters for engine tests. They never access Windows devices or files.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use mini_aec_transport::{
  DiagnosticCounters, PcmFrame, SessionConfig, SessionId, SessionState, SinkDiagnostics, SinkError,
  VirtualMicrophoneSink, WriteReceipt, DIAGNOSTICS_SCHEMA_VERSION,
};

use crate::{
  AudioInput, AudioInputFactory, EchoCanceller, EchoCancellerError, EchoCancellerFactory,
  InputRole, PacketMetadata, SourceDescriptor, SourceError, VirtualSinkFactory,
};

type SourceRun = Result<VecDeque<SourceStep>, SourceError>;
type SourceRuns = Arc<Mutex<VecDeque<SourceRun>>>;

#[derive(Clone, Debug)]
pub enum SourceStep {
  Packet {
    samples: Vec<f32>,
    metadata: PacketMetadata,
  },
  RepeatedPacket {
    samples: Vec<f32>,
    metadata: PacketMetadata,
    remaining: u64,
  },
  Timeout,
  Delay(Duration),
  Failure(SourceError),
}

#[derive(Clone)]
pub struct FakeAudioInputFactory {
  microphone_descriptor: SourceDescriptor,
  render_descriptor: Arc<Mutex<Option<SourceDescriptor>>>,
  microphone_runs: SourceRuns,
  render_runs: SourceRuns,
  stops: Arc<Mutex<u64>>,
}

impl FakeAudioInputFactory {
  #[must_use]
  pub fn new(descriptor: SourceDescriptor) -> Self {
    Self {
      microphone_descriptor: descriptor,
      render_descriptor: Arc::default(),
      microphone_runs: Arc::default(),
      render_runs: Arc::default(),
      stops: Arc::default(),
    }
  }

  #[must_use]
  ///
  /// # Panics
  ///
  /// Panics if a prior fake-adapter thread poisoned the descriptor lock.
  pub fn with_render(self, descriptor: SourceDescriptor) -> Self {
    *self
      .render_descriptor
      .lock()
      .expect("fake render descriptor lock") = Some(descriptor);
    self
  }

  /// Adds one scripted source run.
  ///
  /// # Panics
  ///
  /// Panics if a prior fake-adapter thread poisoned the script lock.
  pub fn push_run(&self, steps: impl IntoIterator<Item = SourceStep>) {
    self
      .microphone_runs
      .lock()
      .expect("fake source runs lock")
      .push_back(Ok(steps.into_iter().collect()));
  }

  /// Adds one scripted render-loopback run.
  ///
  /// # Panics
  ///
  /// Panics if a prior fake-adapter thread poisoned the script lock.
  pub fn push_render_run(&self, steps: impl IntoIterator<Item = SourceStep>) {
    self
      .render_runs
      .lock()
      .expect("fake render runs lock")
      .push_back(Ok(steps.into_iter().collect()));
  }

  /// Adds a source-open failure for the next run.
  ///
  /// # Panics
  ///
  /// Panics if a prior fake-adapter thread poisoned the script lock.
  pub fn push_open_error(&self, error: SourceError) {
    self
      .microphone_runs
      .lock()
      .expect("fake source runs lock")
      .push_back(Err(error));
  }

  #[must_use]
  /// Returns how many fake streams were stopped.
  ///
  /// # Panics
  ///
  /// Panics if a prior fake-adapter thread poisoned the counter lock.
  pub fn stop_count(&self) -> u64 {
    *self.stops.lock().expect("fake source stops lock")
  }
}

impl AudioInputFactory for FakeAudioInputFactory {
  fn resolve(&self, role: InputRole, endpoint_id: &str) -> Result<SourceDescriptor, SourceError> {
    let descriptor = match role {
      InputRole::Microphone => Some(self.microphone_descriptor.clone()),
      InputRole::RenderLoopback => self
        .render_descriptor
        .lock()
        .expect("fake render descriptor lock")
        .clone(),
    };
    descriptor
      .filter(|descriptor| endpoint_id == descriptor.endpoint_id)
      .ok_or_else(|| {
        SourceError::new(
          crate::SourceErrorKind::Unavailable,
          format!("fake {role:?} endpoint {endpoint_id:?} is unavailable"),
        )
      })
  }

  fn open(
    &self,
    role: InputRole,
    _source: &SourceDescriptor,
  ) -> Result<Box<dyn AudioInput>, SourceError> {
    let runs = match role {
      InputRole::Microphone => &self.microphone_runs,
      InputRole::RenderLoopback => &self.render_runs,
    };
    let script = runs
      .lock()
      .expect("fake source runs lock")
      .pop_front()
      .unwrap_or_else(|| Ok(VecDeque::new()))?;
    Ok(Box::new(FakeAudioInput {
      script,
      stops: Arc::clone(&self.stops),
      stopped: false,
    }))
  }
}

#[derive(Clone, Debug, Default)]
pub struct FakeEchoPlan {
  pub fail_at: Option<u64>,
  pub non_finite_at: Option<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FakeEchoRecord {
  pub calls: u64,
  pub render_first_samples: Vec<i16>,
  pub capture_first_samples: Vec<i16>,
}

#[derive(Clone, Default)]
pub struct FakeEchoCancellerFactory {
  plans: Arc<Mutex<VecDeque<FakeEchoPlan>>>,
  records: Arc<Mutex<Vec<Arc<Mutex<FakeEchoRecord>>>>>,
}

impl FakeEchoCancellerFactory {
  /// Adds one scripted processor construction.
  ///
  /// # Panics
  ///
  /// Panics if a prior fake processor poisoned the plan lock.
  pub fn push_plan(&self, plan: FakeEchoPlan) {
    self
      .plans
      .lock()
      .expect("fake echo plans lock")
      .push_back(plan);
  }

  #[must_use]
  ///
  /// # Panics
  ///
  /// Panics if a prior fake processor poisoned a record lock.
  pub fn records(&self) -> Vec<FakeEchoRecord> {
    self
      .records
      .lock()
      .expect("fake echo records lock")
      .iter()
      .map(|record| record.lock().expect("fake echo record lock").clone())
      .collect()
  }
}

impl EchoCancellerFactory for FakeEchoCancellerFactory {
  fn create(&self) -> Result<Box<dyn EchoCanceller>, EchoCancellerError> {
    let plan = self
      .plans
      .lock()
      .expect("fake echo plans lock")
      .pop_front()
      .unwrap_or_default();
    let record = Arc::new(Mutex::new(FakeEchoRecord::default()));
    self
      .records
      .lock()
      .expect("fake echo records lock")
      .push(Arc::clone(&record));
    Ok(Box::new(FakeEchoCanceller { plan, record }))
  }
}

struct FakeEchoCanceller {
  plan: FakeEchoPlan,
  record: Arc<Mutex<FakeEchoRecord>>,
}

impl EchoCanceller for FakeEchoCanceller {
  #[allow(
    clippy::cast_possible_truncation,
    reason = "bounded normalized test samples are scaled only for compact deterministic records"
  )]
  fn process(
    &mut self,
    render: &[f32; 480],
    capture: &[f32; 480],
    output: &mut [f32; 480],
  ) -> Result<(), EchoCancellerError> {
    let call = self.record.lock().expect("fake echo record lock").calls;
    if self.plan.fail_at == Some(call) {
      return Err(EchoCancellerError::new("scripted echo-canceller failure"));
    }
    output.copy_from_slice(capture);
    if self.plan.non_finite_at == Some(call) {
      output[0] = f32::NAN;
    }
    let mut record = self.record.lock().expect("fake echo record lock");
    record.calls += 1;
    record
      .render_first_samples
      .push((render[0] * 1_000.0).round() as i16);
    record
      .capture_first_samples
      .push((capture[0] * 1_000.0).round() as i16);
    Ok(())
  }
}

struct FakeAudioInput {
  script: VecDeque<SourceStep>,
  stops: Arc<Mutex<u64>>,
  stopped: bool,
}

impl AudioInput for FakeAudioInput {
  fn read_packet(
    &mut self,
    samples: &mut [f32],
    timeout: Duration,
  ) -> Result<Option<PacketMetadata>, SourceError> {
    let Some(mut step) = self.script.pop_front() else {
      thread::sleep(timeout.min(Duration::from_millis(1)));
      return Ok(None);
    };
    match &mut step {
      SourceStep::Packet {
        samples: packet,
        metadata,
      } => {
        samples[..packet.len()].copy_from_slice(packet);
        Ok(Some(*metadata))
      }
      SourceStep::RepeatedPacket {
        samples: packet,
        metadata,
        remaining,
      } => {
        samples[..packet.len()].copy_from_slice(packet);
        *remaining = remaining.saturating_sub(1);
        let metadata = *metadata;
        if *remaining > 0 {
          self.script.push_front(step);
        }
        thread::yield_now();
        Ok(Some(metadata))
      }
      SourceStep::Timeout => Ok(None),
      SourceStep::Delay(duration) => {
        thread::sleep(*duration);
        Ok(None)
      }
      SourceStep::Failure(error) => Err(error.clone()),
    }
  }

  fn stop(&mut self) -> Result<(), SourceError> {
    if !self.stopped {
      *self.stops.lock().expect("fake source stops lock") += 1;
      self.stopped = true;
    }
    Ok(())
  }
}

#[derive(Clone, Debug, Default)]
pub struct FakeSinkPlan {
  pub connect_error: Option<SinkError>,
  pub open_error: Option<SinkError>,
  pub write_error_at: Option<(u64, SinkError)>,
  pub write_delay: Duration,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FakeSessionRecord {
  pub session_id: Option<SessionId>,
  pub accepted: Vec<(u64, i16)>,
  pub close_calls: u64,
}

#[derive(Clone, Default)]
pub struct FakeSinkFactory {
  plans: Arc<Mutex<VecDeque<FakeSinkPlan>>>,
  sessions: Arc<Mutex<Vec<Arc<Mutex<FakeSessionRecord>>>>>,
}

impl FakeSinkFactory {
  /// Adds a scripted sink run.
  ///
  /// # Panics
  ///
  /// Panics if a prior fake-adapter thread poisoned the plan lock.
  pub fn push_plan(&self, plan: FakeSinkPlan) {
    self
      .plans
      .lock()
      .expect("fake sink plans lock")
      .push_back(plan);
  }

  #[must_use]
  /// Returns metadata-only records from all fake sink sessions.
  ///
  /// # Panics
  ///
  /// Panics if a prior fake-adapter thread poisoned a record lock.
  pub fn records(&self) -> Vec<FakeSessionRecord> {
    self
      .sessions
      .lock()
      .expect("fake sink sessions lock")
      .iter()
      .map(|record| record.lock().expect("fake session record lock").clone())
      .collect()
  }
}

impl VirtualSinkFactory for FakeSinkFactory {
  fn connect(&self) -> Result<Box<dyn VirtualMicrophoneSink>, SinkError> {
    let plan = self
      .plans
      .lock()
      .expect("fake sink plans lock")
      .pop_front()
      .unwrap_or_default();
    if let Some(error) = plan.connect_error {
      return Err(error);
    }
    let record = Arc::new(Mutex::new(FakeSessionRecord::default()));
    self
      .sessions
      .lock()
      .expect("fake sink sessions lock")
      .push(Arc::clone(&record));
    Ok(Box::new(FakeSink {
      plan,
      record,
      open: false,
    }))
  }
}

struct FakeSink {
  plan: FakeSinkPlan,
  record: Arc<Mutex<FakeSessionRecord>>,
  open: bool,
}

impl VirtualMicrophoneSink for FakeSink {
  fn open_session(&mut self, config: SessionConfig) -> Result<(), SinkError> {
    if let Some(error) = self.plan.open_error.clone() {
      return Err(error);
    }
    self
      .record
      .lock()
      .expect("fake session record lock")
      .session_id = Some(config.session_id());
    self.open = true;
    Ok(())
  }

  fn write_frame(&mut self, frame: PcmFrame<'_>) -> Result<WriteReceipt, SinkError> {
    if let Some((sequence, error)) = &self.plan.write_error_at {
      if frame.sequence() == *sequence {
        return Err(error.clone());
      }
    }
    if !self.plan.write_delay.is_zero() {
      thread::sleep(self.plan.write_delay);
    }
    self
      .record
      .lock()
      .expect("fake session record lock")
      .accepted
      .push((frame.sequence(), frame.samples()[0]));
    Ok(WriteReceipt::accepted(frame.sequence()))
  }

  fn diagnostics(&self) -> Result<SinkDiagnostics, SinkError> {
    let record = self.record.lock().expect("fake session record lock");
    let accepted_frames = record.accepted.len() as u64;
    Ok(SinkDiagnostics {
      schema_version: DIAGNOSTICS_SCHEMA_VERSION,
      state: if self.open {
        SessionState::Open
      } else {
        SessionState::Closed
      },
      active_session: record.session_id.filter(|_| self.open),
      last_accepted_sequence: record.accepted.last().map(|(sequence, _)| *sequence),
      current_depth: 0,
      high_water_mark: 0,
      counters: DiagnosticCounters {
        session_opens: u64::from(record.session_id.is_some()),
        session_closes: record.close_calls,
        accepted_frames,
        ..DiagnosticCounters::default()
      },
    })
  }

  fn close_session(&mut self) -> Result<(), SinkError> {
    if self.open {
      self
        .record
        .lock()
        .expect("fake session record lock")
        .close_calls += 1;
      self.open = false;
    }
    Ok(())
  }
}
