use std::env;
use std::sync::{Arc, Mutex};

use mini_aec_engine::windows::WindowsAudioInputFactory;
use mini_aec_engine::{
  DefaultEchoCancellerFactory, Engine, EngineConfig, EngineError, EngineSnapshot, ProcessingMode,
  VirtualSinkFactory,
};
use mini_aec_transport::{SinkError, VirtualMicrophoneSink};
use mini_aec_windows_transport::WindowsVirtualMicrophoneSink;

pub(crate) const MICROPHONE_ENV: &str = "MINI_AEC_MICROPHONE_ID";
pub(crate) const RENDER_ENV: &str = "MINI_AEC_RENDER_ID";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TrayConfiguration {
  pub microphone_endpoint_id: String,
  pub render_endpoint_id: String,
}

impl TrayConfiguration {
  pub(crate) fn from_environment() -> Option<Self> {
    Self::from_values(env::var(MICROPHONE_ENV).ok(), env::var(RENDER_ENV).ok())
  }

  fn from_values(microphone: Option<String>, render: Option<String>) -> Option<Self> {
    let microphone_endpoint_id = microphone?.trim().to_owned();
    let render_endpoint_id = render?.trim().to_owned();
    if microphone_endpoint_id.is_empty() || render_endpoint_id.is_empty() {
      return None;
    }
    Some(Self {
      microphone_endpoint_id,
      render_endpoint_id,
    })
  }
}

struct WindowsSinkFactory;

impl VirtualSinkFactory for WindowsSinkFactory {
  fn connect(&self) -> Result<Box<dyn VirtualMicrophoneSink>, SinkError> {
    WindowsVirtualMicrophoneSink::connect()
      .map(|sink| Box::new(sink) as Box<dyn VirtualMicrophoneSink>)
  }
}

pub(crate) struct TrayEngine {
  engine: Arc<Engine>,
  config: TrayConfiguration,
  selected_mode: Mutex<ProcessingMode>,
}

impl TrayEngine {
  pub(crate) fn new(config: TrayConfiguration) -> Self {
    Self {
      engine: Arc::new(Engine::new_with_aec(
        Arc::new(WindowsAudioInputFactory),
        Arc::new(WindowsSinkFactory),
        Arc::new(DefaultEchoCancellerFactory),
      )),
      config,
      selected_mode: Mutex::new(ProcessingMode::Aec),
    }
  }

  pub(crate) fn select_and_start(&self, mode: ProcessingMode) -> Result<(), EngineError> {
    *self.selected_mode.lock().expect("tray mode lock") = mode;
    self.engine.stop()?;
    self.engine.start(self.engine_config(mode))
  }

  pub(crate) fn restart(&self) -> Result<(), EngineError> {
    let mode = *self.selected_mode.lock().expect("tray mode lock");
    self.engine.stop()?;
    self.engine.start(self.engine_config(mode))
  }

  pub(crate) fn stop(&self) -> Result<(), EngineError> {
    self.engine.stop()
  }

  pub(crate) fn snapshot(&self) -> EngineSnapshot {
    self.engine.snapshot()
  }

  fn engine_config(&self, mode: ProcessingMode) -> EngineConfig {
    match mode {
      ProcessingMode::Bypass => EngineConfig::bypass(self.config.microphone_endpoint_id.clone()),
      ProcessingMode::Aec => EngineConfig::aec(
        self.config.microphone_endpoint_id.clone(),
        self.config.render_endpoint_id.clone(),
      ),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::TrayConfiguration;

  #[test]
  fn configuration_requires_both_exact_nonempty_ids() {
    assert!(TrayConfiguration::from_values(None, None).is_none());
    assert!(TrayConfiguration::from_values(Some("mic".to_owned()), None).is_none());
    assert!(
      TrayConfiguration::from_values(Some(" ".to_owned()), Some("render".to_owned())).is_none()
    );
    assert_eq!(
      TrayConfiguration::from_values(Some(" mic ".to_owned()), Some(" render ".to_owned())),
      Some(TrayConfiguration {
        microphone_endpoint_id: "mic".to_owned(),
        render_endpoint_id: "render".to_owned(),
      })
    );
  }
}
