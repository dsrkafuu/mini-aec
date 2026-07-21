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
  AudioInput, AudioInputFactory, PacketMetadata, SourceDescriptor, SourceError, VirtualSinkFactory,
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
  descriptor: SourceDescriptor,
  runs: SourceRuns,
  stops: Arc<Mutex<u64>>,
}

impl FakeAudioInputFactory {
  #[must_use]
  pub fn new(descriptor: SourceDescriptor) -> Self {
    Self {
      descriptor,
      runs: Arc::default(),
      stops: Arc::default(),
    }
  }

  /// Adds one scripted source run.
  ///
  /// # Panics
  ///
  /// Panics if a prior fake-adapter thread poisoned the script lock.
  pub fn push_run(&self, steps: impl IntoIterator<Item = SourceStep>) {
    self
      .runs
      .lock()
      .expect("fake source runs lock")
      .push_back(Ok(steps.into_iter().collect()));
  }

  /// Adds a source-open failure for the next run.
  ///
  /// # Panics
  ///
  /// Panics if a prior fake-adapter thread poisoned the script lock.
  pub fn push_open_error(&self, error: SourceError) {
    self
      .runs
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
  fn resolve(&self, endpoint_id: &str) -> Result<SourceDescriptor, SourceError> {
    if endpoint_id == self.descriptor.endpoint_id {
      Ok(self.descriptor.clone())
    } else {
      Err(SourceError::new(
        crate::SourceErrorKind::Unavailable,
        format!("fake endpoint {endpoint_id:?} is unavailable"),
      ))
    }
  }

  fn open(&self, _source: &SourceDescriptor) -> Result<Box<dyn AudioInput>, SourceError> {
    let script = self
      .runs
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
