use std::env;
use std::fmt::Display;
use std::sync::{Arc, Mutex};

use crate::config::{ConfigError, ConfigStore, PersistedConfiguration};
use mini_aec_engine::windows::{
  enumerate_capture_endpoints, enumerate_default_capture_endpoint,
  enumerate_default_render_endpoint, enumerate_render_endpoints, WindowsAudioInputFactory,
};
use mini_aec_engine::{
  AudioOutputFactory, DefaultEchoCancellerFactory, Engine, EngineConfig, EngineError,
  EngineErrorKind, EngineSnapshot, InputRole, ProcessingMode, SourceDescriptor,
};
use mini_aec_output::{AudioOutput, OutputError, OutputPair};
use mini_aec_windows_output::{
  enumerate_vb_cable_pairs, resolve_vb_cable_pair, WindowsVbCableOutput,
};

pub(crate) const MICROPHONE_ENV: &str = "MINI_AEC_MICROPHONE_ID";
pub(crate) const RENDER_ENV: &str = "MINI_AEC_RENDER_ID";
pub(crate) const CABLE_INPUT_ENV: &str = "MINI_AEC_CABLE_INPUT_ID";
pub(crate) const CABLE_OUTPUT_ENV: &str = "MINI_AEC_CABLE_OUTPUT_ID";

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
  clippy::struct_field_names,
  reason = "the explicit suffix distinguishes persisted Windows endpoint identities from labels"
)]
pub(crate) struct TrayConfiguration {
  pub mode: ProcessingMode,
  pub microphone_endpoint_id: String,
  pub microphone_uses_default: bool,
  pub render_endpoint_id: Option<String>,
  pub render_uses_default: bool,
  pub cable_input_endpoint_id: String,
  pub cable_output_endpoint_id: String,
}

impl TrayConfiguration {
  fn from_persisted(configuration: PersistedConfiguration) -> Result<Self, ConfigError> {
    configuration.validate()?;
    Ok(Self {
      mode: configuration.mode,
      microphone_endpoint_id: configuration.microphone_endpoint_id,
      microphone_uses_default: configuration.microphone_uses_default,
      render_endpoint_id: configuration.render_endpoint_id,
      render_uses_default: configuration.render_uses_default,
      cable_input_endpoint_id: configuration.cable_input_endpoint_id,
      cable_output_endpoint_id: configuration.cable_output_endpoint_id,
    })
  }

  fn to_persisted(&self) -> PersistedConfiguration {
    PersistedConfiguration {
      version: crate::config::CONFIG_VERSION,
      mode: self.mode,
      microphone_endpoint_id: self.microphone_endpoint_id.clone(),
      microphone_uses_default: self.microphone_uses_default,
      render_endpoint_id: self.render_endpoint_id.clone(),
      render_uses_default: self.render_uses_default,
      cable_input_endpoint_id: self.cable_input_endpoint_id.clone(),
      cable_output_endpoint_id: self.cable_output_endpoint_id.clone(),
    }
  }

  pub(crate) fn from_environment() -> Result<Option<Self>, ConfigError> {
    Self::from_values(
      env::var(MICROPHONE_ENV).ok(),
      env::var(RENDER_ENV).ok(),
      env::var(CABLE_INPUT_ENV).ok(),
      env::var(CABLE_OUTPUT_ENV).ok(),
    )
  }

  fn from_values(
    microphone: Option<String>,
    render: Option<String>,
    cable_input: Option<String>,
    cable_output: Option<String>,
  ) -> Result<Option<Self>, ConfigError> {
    let values = [&microphone, &render, &cable_input, &cable_output];
    if values.iter().all(|value| value.is_none()) {
      return Ok(None);
    }
    let Some(microphone_endpoint_id) = normalize_optional(microphone) else {
      return Err(ConfigError::Invalid(format!(
        "{MICROPHONE_ENV} must be set with the complete override group"
      )));
    };
    let Some(render_endpoint_id) = normalize_optional(render) else {
      return Err(ConfigError::Invalid(format!(
        "{RENDER_ENV} must be set with the complete override group"
      )));
    };
    let Some(cable_input_endpoint_id) = normalize_optional(cable_input) else {
      return Err(ConfigError::Invalid(format!(
        "{CABLE_INPUT_ENV} must be set with the complete override group"
      )));
    };
    let Some(cable_output_endpoint_id) = normalize_optional(cable_output) else {
      return Err(ConfigError::Invalid(format!(
        "{CABLE_OUTPUT_ENV} must be set with the complete override group"
      )));
    };
    Ok(Some(Self {
      mode: ProcessingMode::Aec,
      microphone_endpoint_id,
      microphone_uses_default: false,
      render_endpoint_id: Some(render_endpoint_id),
      render_uses_default: false,
      cable_input_endpoint_id,
      cable_output_endpoint_id,
    }))
  }

  fn engine_config(&self, mode: ProcessingMode) -> Result<EngineConfig, EngineError> {
    match mode {
      ProcessingMode::Bypass => Ok(EngineConfig::bypass(
        self.microphone_endpoint_id.clone(),
        self.cable_input_endpoint_id.clone(),
        self.cable_output_endpoint_id.clone(),
      )),
      ProcessingMode::Aec => self
        .render_endpoint_id
        .as_ref()
        .map(|render| {
          EngineConfig::aec(
            self.microphone_endpoint_id.clone(),
            render.clone(),
            self.cable_input_endpoint_id.clone(),
            self.cable_output_endpoint_id.clone(),
          )
        })
        .ok_or_else(|| {
          EngineError::new(
            EngineErrorKind::InvalidConfiguration,
            "AEC mode requires an exact physical render endpoint ID",
          )
        }),
    }
  }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
  value.and_then(|value| {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
  })
}

fn normalize_value(value: String) -> Option<String> {
  normalize_optional(Some(value))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConfigurationSource {
  None,
  Persisted,
  Environment,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConfigurationDraft {
  pub mode: ProcessingMode,
  pub microphone_endpoint_id: Option<String>,
  pub render_endpoint_id: Option<String>,
  pub cable_input_endpoint_id: Option<String>,
  pub cable_output_endpoint_id: Option<String>,
}

impl Default for ConfigurationDraft {
  fn default() -> Self {
    Self {
      mode: ProcessingMode::Bypass,
      microphone_endpoint_id: None,
      render_endpoint_id: None,
      cable_input_endpoint_id: None,
      cable_output_endpoint_id: None,
    }
  }
}

impl From<&TrayConfiguration> for ConfigurationDraft {
  fn from(configuration: &TrayConfiguration) -> Self {
    Self {
      mode: configuration.mode,
      microphone_endpoint_id: Some(configuration.microphone_endpoint_id.clone()),
      render_endpoint_id: configuration.render_endpoint_id.clone(),
      cable_input_endpoint_id: Some(configuration.cable_input_endpoint_id.clone()),
      cable_output_endpoint_id: Some(configuration.cable_output_endpoint_id.clone()),
    }
  }
}

impl ConfigurationDraft {
  fn into_configuration(self, mode: ProcessingMode) -> Result<TrayConfiguration, ConfigError> {
    let microphone_endpoint_id = self
      .microphone_endpoint_id
      .and_then(normalize_value)
      .ok_or_else(|| {
        ConfigError::Invalid("a physical microphone endpoint is required".to_owned())
      })?;
    let cable_input_endpoint_id = self
      .cable_input_endpoint_id
      .and_then(normalize_value)
      .ok_or_else(|| ConfigError::Invalid("a CABLE Input endpoint is required".to_owned()))?;
    let cable_output_endpoint_id = self
      .cable_output_endpoint_id
      .and_then(normalize_value)
      .ok_or_else(|| ConfigError::Invalid("a CABLE Output endpoint is required".to_owned()))?;
    let render_endpoint_id = self.render_endpoint_id.and_then(normalize_value);
    let configuration = TrayConfiguration {
      mode,
      microphone_endpoint_id,
      microphone_uses_default: false,
      render_endpoint_id,
      render_uses_default: false,
      cable_input_endpoint_id,
      cable_output_endpoint_id,
    };
    if mode == ProcessingMode::Aec && configuration.render_endpoint_id.is_none() {
      return Err(ConfigError::Invalid(
        "AEC mode requires a physical render endpoint".to_owned(),
      ));
    }
    Ok(configuration)
  }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct DeviceInventory {
  pub default_microphone: Option<SourceDescriptor>,
  pub default_render: Option<SourceDescriptor>,
  pub microphones: Vec<SourceDescriptor>,
  pub renders: Vec<SourceDescriptor>,
  pub cable_pairs: Vec<OutputPair>,
}

impl DeviceInventory {
  pub(crate) fn discover() -> Result<Self, String> {
    let mut microphones = enumerate_capture_endpoints()
      .map_err(|error| format!("failed to enumerate physical microphones: {error}"))?
      .into_iter()
      .filter(|source| source.active && source.role == InputRole::Microphone)
      .collect::<Vec<_>>();
    let mut renders = enumerate_render_endpoints()
      .map_err(|error| format!("failed to enumerate physical render endpoints: {error}"))?
      .into_iter()
      .filter(|source| source.active && source.role == InputRole::RenderLoopback)
      .collect::<Vec<_>>();
    let cable_pairs = enumerate_vb_cable_pairs()
      .map_err(|error| format!("failed to enumerate VB-CABLE pairs: {error}"))?;
    let cable_output_ids: Vec<_> = cable_pairs
      .iter()
      .map(|pair| pair.recording.endpoint_id.as_str())
      .collect();
    let cable_input_ids: Vec<_> = cable_pairs
      .iter()
      .map(|pair| pair.playback.endpoint_id.as_str())
      .collect();
    microphones.retain(|source| {
      !cable_output_ids
        .iter()
        .any(|endpoint_id| *endpoint_id == source.endpoint_id)
    });
    renders.retain(|source| {
      !cable_input_ids
        .iter()
        .any(|endpoint_id| *endpoint_id == source.endpoint_id)
    });

    let default_microphone = enumerate_default_capture_endpoint()
      .ok()
      .filter(|source| source.active && source.role == InputRole::Microphone)
      .and_then(|default| {
        microphones
          .iter()
          .find(|source| source.endpoint_id == default.endpoint_id)
          .cloned()
      });
    let default_render = enumerate_default_render_endpoint()
      .ok()
      .filter(|source| source.active && source.role == InputRole::RenderLoopback)
      .and_then(|default| {
        renders
          .iter()
          .find(|source| source.endpoint_id == default.endpoint_id)
          .cloned()
      });

    Ok(Self {
      default_microphone,
      default_render,
      microphones,
      renders,
      cable_pairs,
    })
  }
}

struct WindowsOutputFactory;

impl AudioOutputFactory for WindowsOutputFactory {
  fn resolve_pair(
    &self,
    cable_input_endpoint_id: &str,
    cable_output_endpoint_id: &str,
  ) -> Result<OutputPair, OutputError> {
    resolve_vb_cable_pair(cable_input_endpoint_id, cable_output_endpoint_id)
  }

  fn connect(&self, pair: &OutputPair) -> Result<Box<dyn AudioOutput>, OutputError> {
    Ok(Box::new(WindowsVbCableOutput::new(pair.clone())))
  }
}

pub(crate) struct TrayEngine {
  engine: Arc<Engine>,
  config_store: ConfigStore,
  configuration: Mutex<Option<TrayConfiguration>>,
  draft: Mutex<ConfigurationDraft>,
  source: Mutex<ConfigurationSource>,
  configuration_error: Mutex<Option<String>>,
  inventory_error: Mutex<Option<String>>,
  inventory: Mutex<DeviceInventory>,
  selected_mode: Mutex<ProcessingMode>,
  defaults_pending: Mutex<bool>,
  microphone_default_selected: Mutex<bool>,
  render_default_selected: Mutex<bool>,
}

impl TrayEngine {
  pub(crate) fn new(config_store: ConfigStore) -> Self {
    let (configuration, source, configuration_error) = load_configuration(&config_store);
    let draft = configuration
      .as_ref()
      .map(ConfigurationDraft::from)
      .unwrap_or_default();
    let selected_mode = configuration
      .as_ref()
      .map_or(ProcessingMode::Bypass, |configuration| configuration.mode);
    let microphone_default_selected = configuration
      .as_ref()
      .is_some_and(|configuration| configuration.microphone_uses_default);
    let render_default_selected = configuration
      .as_ref()
      .is_some_and(|configuration| configuration.render_uses_default);
    let defaults_pending = configuration_error.is_none()
      && source == ConfigurationSource::None
      && configuration.is_none();
    Self {
      engine: Arc::new(Engine::new_with_aec(
        Arc::new(WindowsAudioInputFactory),
        Arc::new(WindowsOutputFactory),
        Arc::new(DefaultEchoCancellerFactory),
      )),
      config_store,
      configuration: Mutex::new(configuration),
      draft: Mutex::new(draft),
      source: Mutex::new(source),
      configuration_error: Mutex::new(configuration_error),
      inventory_error: Mutex::new(None),
      inventory: Mutex::new(DeviceInventory::default()),
      selected_mode: Mutex::new(selected_mode),
      defaults_pending: Mutex::new(defaults_pending),
      microphone_default_selected: Mutex::new(microphone_default_selected),
      render_default_selected: Mutex::new(render_default_selected),
    }
  }

  pub(crate) fn refresh_inventory(&self) -> Result<bool, String> {
    match DeviceInventory::discover() {
      Ok(inventory) => {
        let changed = self.inventory() != inventory;
        self.initialize_defaults(&inventory);
        *self.inventory.lock().expect("tray inventory lock") = inventory;
        *self
          .inventory_error
          .lock()
          .expect("tray inventory error lock") = None;
        Ok(changed)
      }
      Err(error) => {
        *self
          .inventory_error
          .lock()
          .expect("tray inventory error lock") = Some(error.clone());
        Err(error)
      }
    }
  }

  pub(crate) fn inventory(&self) -> DeviceInventory {
    self.inventory.lock().expect("tray inventory lock").clone()
  }

  pub(crate) fn configuration_source(&self) -> ConfigurationSource {
    *self.source.lock().expect("tray configuration source lock")
  }

  pub(crate) fn configuration_error(&self) -> Option<String> {
    if let Some(error) = self
      .configuration_error
      .lock()
      .expect("tray configuration error lock")
      .clone()
    {
      return Some(error);
    }
    if let Some(error) = self
      .inventory_error
      .lock()
      .expect("tray inventory error lock")
      .clone()
    {
      return Some(error);
    }
    let configuration = self
      .configuration
      .lock()
      .expect("tray configuration lock")
      .clone()?;
    validate_configuration(&configuration, &self.inventory())
      .err()
      .map(|error| error.to_string())
  }

  pub(crate) fn draft(&self) -> ConfigurationDraft {
    self
      .draft
      .lock()
      .expect("tray configuration draft lock")
      .clone()
  }

  pub(crate) fn snapshot(&self) -> EngineSnapshot {
    self.engine.snapshot()
  }

  pub(crate) fn aec_enabled(&self) -> bool {
    *self.selected_mode.lock().expect("tray mode lock") == ProcessingMode::Aec
  }

  pub(crate) fn microphone_default_selected(&self) -> bool {
    *self
      .microphone_default_selected
      .lock()
      .expect("tray microphone default selection lock")
  }

  pub(crate) fn render_default_selected(&self) -> bool {
    *self
      .render_default_selected
      .lock()
      .expect("tray render default selection lock")
  }

  pub(crate) fn apply_current(&self) -> Result<(), EngineError> {
    if let Err(error) = self.engine.stop() {
      *self
        .configuration_error
        .lock()
        .expect("tray configuration error lock") = Some(error.to_string());
      return Err(error);
    }
    let mode = *self.selected_mode.lock().expect("tray mode lock");
    let result = (|| {
      let mut candidate = self
        .draft()
        .into_configuration(mode)
        .map_err(engine_configuration_error)?;
      candidate.microphone_uses_default = self.microphone_default_selected();
      candidate.render_uses_default = self.render_default_selected();
      validate_configuration(&candidate, &self.inventory()).map_err(engine_configuration_error)?;
      let engine_config = candidate.engine_config(mode)?;
      if should_persist_configuration(self.configuration_source()) {
        self
          .config_store
          .save(&candidate.to_persisted())
          .map_err(engine_configuration_error)?;
        *self.source.lock().expect("tray configuration source lock") =
          ConfigurationSource::Persisted;
      }
      *self.configuration.lock().expect("tray configuration lock") = Some(candidate.clone());
      *self.draft.lock().expect("tray configuration draft lock") =
        ConfigurationDraft::from(&candidate);
      *self.selected_mode.lock().expect("tray mode lock") = mode;
      *self.defaults_pending.lock().expect("tray defaults lock") = false;
      self.engine.start(engine_config)
    })();
    if let Err(error) = &result {
      *self
        .configuration_error
        .lock()
        .expect("tray configuration error lock") = Some(error.to_string());
    }
    result
  }

  pub(crate) fn stop(&self) -> Result<(), EngineError> {
    self.engine.stop()
  }

  pub(crate) fn set_aec_enabled(&self, enabled: bool) -> Result<(), String> {
    self.ensure_configuration_editable()?;
    let mode = if enabled {
      ProcessingMode::Aec
    } else {
      ProcessingMode::Bypass
    };
    *self.selected_mode.lock().expect("tray mode lock") = mode;
    self
      .draft
      .lock()
      .expect("tray configuration draft lock")
      .mode = mode;
    self.apply_current().map_err(|error| error.to_string())
  }

  pub(crate) fn select_microphone(&self, index: usize) -> Result<(), String> {
    let endpoint_id = self
      .inventory()
      .microphones
      .get(index)
      .map(|source| source.endpoint_id.clone())
      .ok_or_else(|| "selected microphone is no longer available".to_owned())?;
    self.select_microphone_id(endpoint_id, false)
  }

  pub(crate) fn select_default_microphone(&self) -> Result<(), String> {
    let endpoint_id = self
      .inventory()
      .default_microphone
      .map(|source| source.endpoint_id)
      .ok_or_else(|| "the Windows default microphone is not available".to_owned())?;
    self.select_microphone_id(endpoint_id, true)
  }

  pub(crate) fn select_render(&self, index: usize) -> Result<(), String> {
    let endpoint_id = self
      .inventory()
      .renders
      .get(index)
      .map(|source| source.endpoint_id.clone())
      .ok_or_else(|| "selected render endpoint is no longer available".to_owned())?;
    self.select_render_id(endpoint_id, false)
  }

  pub(crate) fn select_default_render(&self) -> Result<(), String> {
    let endpoint_id = self
      .inventory()
      .default_render
      .map(|source| source.endpoint_id)
      .ok_or_else(|| "the Windows default render endpoint is not available".to_owned())?;
    self.select_render_id(endpoint_id, true)
  }

  pub(crate) fn select_cable_pair(&self, index: usize) -> Result<(), String> {
    let (cable_input_endpoint_id, cable_output_endpoint_id) = self
      .inventory()
      .cable_pairs
      .get(index)
      .cloned()
      .map(|pair| (pair.playback.endpoint_id, pair.recording.endpoint_id))
      .ok_or_else(|| "selected VB-CABLE pair is no longer available".to_owned())?;
    self.update_draft_and_apply(|draft| {
      draft.cable_input_endpoint_id = Some(cable_input_endpoint_id);
      draft.cable_output_endpoint_id = Some(cable_output_endpoint_id);
    })
  }

  fn select_microphone_id(&self, endpoint_id: String, is_default: bool) -> Result<(), String> {
    self.ensure_configuration_editable()?;
    *self
      .microphone_default_selected
      .lock()
      .expect("tray microphone default selection lock") = is_default;
    self.update_draft_and_apply(|draft| {
      draft.microphone_endpoint_id = Some(endpoint_id);
    })
  }

  fn select_render_id(&self, endpoint_id: String, is_default: bool) -> Result<(), String> {
    self.ensure_configuration_editable()?;
    *self
      .render_default_selected
      .lock()
      .expect("tray render default selection lock") = is_default;
    self.update_draft_and_apply(|draft| {
      draft.render_endpoint_id = Some(endpoint_id);
    })
  }

  fn update_draft_and_apply<F>(&self, update: F) -> Result<(), String>
  where
    F: FnOnce(&mut ConfigurationDraft),
  {
    self.ensure_configuration_editable()?;
    update(&mut self.draft.lock().expect("tray configuration draft lock"));
    self.apply_current().map_err(|error| error.to_string())
  }

  fn ensure_configuration_editable(&self) -> Result<(), String> {
    if self.configuration_source() == ConfigurationSource::Environment {
      return Err(
        "environment overrides are temporary and cannot be edited from the tray".to_owned(),
      );
    }
    Ok(())
  }

  fn initialize_defaults(&self, inventory: &DeviceInventory) {
    let defaults_pending = *self.defaults_pending.lock().expect("tray defaults lock");
    let microphone_uses_default = self.microphone_default_selected();
    let render_uses_default = self.render_default_selected();
    let mut draft = self.draft.lock().expect("tray configuration draft lock");
    let microphone_was_missing = defaults_pending && draft.microphone_endpoint_id.is_none();
    let render_was_missing = defaults_pending && draft.render_endpoint_id.is_none();
    if defaults_pending {
      populate_default_draft(&mut draft, inventory);
    }
    if microphone_uses_default {
      draft.microphone_endpoint_id = inventory
        .default_microphone
        .as_ref()
        .map(|source| source.endpoint_id.clone());
    }
    if render_uses_default {
      draft.render_endpoint_id = inventory
        .default_render
        .as_ref()
        .map(|source| source.endpoint_id.clone());
    }
    let microphone_selected = microphone_was_missing
      && !microphone_uses_default
      && draft.microphone_endpoint_id.as_deref()
        == inventory
          .default_microphone
          .as_ref()
          .map(|source| source.endpoint_id.as_str());
    let render_selected = render_was_missing
      && !render_uses_default
      && draft.render_endpoint_id.as_deref()
        == inventory
          .default_render
          .as_ref()
          .map(|source| source.endpoint_id.as_str());
    drop(draft);
    if microphone_selected {
      *self
        .microphone_default_selected
        .lock()
        .expect("tray microphone default selection lock") = true;
    }
    if render_selected {
      *self
        .render_default_selected
        .lock()
        .expect("tray render default selection lock") = true;
    }
  }
}

fn populate_default_draft(draft: &mut ConfigurationDraft, inventory: &DeviceInventory) {
  if draft.microphone_endpoint_id.is_none() {
    draft.microphone_endpoint_id = inventory
      .default_microphone
      .as_ref()
      .map(|source| source.endpoint_id.clone());
  }
  if draft.render_endpoint_id.is_none() {
    draft.render_endpoint_id = inventory
      .default_render
      .as_ref()
      .map(|source| source.endpoint_id.clone());
  }
  if draft.cable_input_endpoint_id.is_none() && draft.cable_output_endpoint_id.is_none() {
    if let Some(pair) = inventory.cable_pairs.first() {
      draft.cable_input_endpoint_id = Some(pair.playback.endpoint_id.clone());
      draft.cable_output_endpoint_id = Some(pair.recording.endpoint_id.clone());
    }
  }
}

fn should_persist_configuration(source: ConfigurationSource) -> bool {
  source != ConfigurationSource::Environment
}

fn load_configuration(
  config_store: &ConfigStore,
) -> (
  Option<TrayConfiguration>,
  ConfigurationSource,
  Option<String>,
) {
  load_configuration_from(TrayConfiguration::from_environment(), config_store)
}

fn load_configuration_from(
  environment: Result<Option<TrayConfiguration>, ConfigError>,
  config_store: &ConfigStore,
) -> (
  Option<TrayConfiguration>,
  ConfigurationSource,
  Option<String>,
) {
  match environment {
    Ok(Some(configuration)) => (Some(configuration), ConfigurationSource::Environment, None),
    Err(error) => (None, ConfigurationSource::None, Some(error.to_string())),
    Ok(None) => match config_store.load() {
      Ok(Some(configuration)) => match TrayConfiguration::from_persisted(configuration) {
        Ok(configuration) => (Some(configuration), ConfigurationSource::Persisted, None),
        Err(error) => (None, ConfigurationSource::None, Some(error.to_string())),
      },
      Ok(None) => (None, ConfigurationSource::None, None),
      Err(error) => (None, ConfigurationSource::None, Some(error.to_string())),
    },
  }
}

fn engine_configuration_error(error: impl Display) -> EngineError {
  EngineError::new(EngineErrorKind::InvalidConfiguration, error.to_string())
}

fn validate_configuration(
  configuration: &TrayConfiguration,
  inventory: &DeviceInventory,
) -> Result<(), ConfigError> {
  if configuration.microphone_endpoint_id == configuration.cable_output_endpoint_id {
    return Err(ConfigError::Invalid(
      "CABLE Output cannot be used as the physical microphone".to_owned(),
    ));
  }
  if configuration
    .render_endpoint_id
    .as_deref()
    .is_some_and(|render| render == configuration.cable_input_endpoint_id)
  {
    return Err(ConfigError::Invalid(
      "CABLE Input cannot be used as the physical render-loopback reference".to_owned(),
    ));
  }
  if !inventory
    .microphones
    .iter()
    .any(|source| source.endpoint_id == configuration.microphone_endpoint_id && source.active)
  {
    return Err(ConfigError::Invalid(
      "the configured physical microphone is unavailable or inactive".to_owned(),
    ));
  }
  if configuration.mode == ProcessingMode::Aec {
    let render = configuration.render_endpoint_id.as_deref().ok_or_else(|| {
      ConfigError::Invalid("AEC mode requires a physical render endpoint".to_owned())
    })?;
    if !inventory
      .renders
      .iter()
      .any(|source| source.endpoint_id == render && source.active)
    {
      return Err(ConfigError::Invalid(
        "the configured physical render endpoint is unavailable or inactive".to_owned(),
      ));
    }
  }
  let matching_pairs = inventory
    .cable_pairs
    .iter()
    .filter(|pair| {
      pair.playback.endpoint_id == configuration.cable_input_endpoint_id
        && pair.recording.endpoint_id == configuration.cable_output_endpoint_id
    })
    .count();
  match matching_pairs {
    1 => {}
    0 => {
      return Err(ConfigError::Invalid(
        "the configured VB-CABLE pair is unavailable or inactive".to_owned(),
      ));
    }
    _ => {
      return Err(ConfigError::Invalid(
        "the configured VB-CABLE pair is ambiguous".to_owned(),
      ));
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use std::fs;
  use std::time::{SystemTime, UNIX_EPOCH};

  use super::*;
  use mini_aec_output::{EndpointRole, OutputEndpointDescriptor};

  fn source(role: InputRole, id: &str) -> SourceDescriptor {
    SourceDescriptor {
      role,
      endpoint_id: id.to_owned(),
      friendly_name: format!("device {id}"),
      active: true,
      native_format: None,
    }
  }

  fn pair() -> OutputPair {
    OutputPair {
      playback: OutputEndpointDescriptor {
        endpoint_id: "cable-input".to_owned(),
        friendly_name: "CABLE Input".to_owned(),
        role: EndpointRole::Playback,
        active: true,
        device_family: "VB-Audio Virtual Cable".to_owned(),
      },
      recording: OutputEndpointDescriptor {
        endpoint_id: "cable-output".to_owned(),
        friendly_name: "CABLE Output".to_owned(),
        role: EndpointRole::Recording,
        active: true,
        device_family: "VB-Audio Virtual Cable".to_owned(),
      },
    }
  }

  fn inventory() -> DeviceInventory {
    DeviceInventory {
      default_microphone: Some(source(InputRole::Microphone, "mic")),
      default_render: Some(source(InputRole::RenderLoopback, "render")),
      microphones: vec![source(InputRole::Microphone, "mic")],
      renders: vec![source(InputRole::RenderLoopback, "render")],
      cable_pairs: vec![pair()],
    }
  }

  fn temporary_store() -> (ConfigStore, std::path::PathBuf) {
    let suffix = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .expect("system time is valid")
      .as_nanos();
    let root = std::env::temp_dir().join(format!("mini-aec-tray-config-{suffix}"));
    (ConfigStore::new(root.join("config.json")), root)
  }

  fn persisted_configuration(mode: ProcessingMode) -> PersistedConfiguration {
    PersistedConfiguration {
      version: crate::config::CONFIG_VERSION,
      mode,
      microphone_endpoint_id: "persisted-mic".to_owned(),
      microphone_uses_default: false,
      render_endpoint_id: (mode == ProcessingMode::Aec).then(|| "persisted-render".to_owned()),
      render_uses_default: false,
      cable_input_endpoint_id: "persisted-input".to_owned(),
      cable_output_endpoint_id: "persisted-output".to_owned(),
    }
  }

  #[test]
  fn environment_override_requires_all_exact_ids() {
    assert_eq!(
      TrayConfiguration::from_values(None, None, None, None).expect("empty override is absent"),
      None
    );
    assert!(TrayConfiguration::from_values(
      Some("mic".to_owned()),
      Some("render".to_owned()),
      None,
      Some("output".to_owned()),
    )
    .is_err());
    let configuration = TrayConfiguration::from_values(
      Some(" mic ".to_owned()),
      Some(" render ".to_owned()),
      Some(" input ".to_owned()),
      Some(" output ".to_owned()),
    )
    .expect("complete override parses")
    .expect("complete override is present");
    assert_eq!(configuration.microphone_endpoint_id, "mic");
    assert_eq!(configuration.mode, ProcessingMode::Aec);
  }

  #[test]
  fn persisted_default_intent_round_trips_without_changing_endpoint_identity() {
    let persisted = PersistedConfiguration {
      version: crate::config::CONFIG_VERSION,
      mode: ProcessingMode::Aec,
      microphone_endpoint_id: "mic".to_owned(),
      microphone_uses_default: true,
      render_endpoint_id: Some("render".to_owned()),
      render_uses_default: true,
      cable_input_endpoint_id: "cable-input".to_owned(),
      cable_output_endpoint_id: "cable-output".to_owned(),
    };
    let configuration =
      TrayConfiguration::from_persisted(persisted.clone()).expect("persisted configuration parses");
    assert!(configuration.microphone_uses_default);
    assert!(configuration.render_uses_default);
    assert_eq!(configuration.to_persisted(), persisted);
  }

  #[test]
  fn persisted_default_intent_re_resolves_current_system_candidate() {
    let (store, root) = temporary_store();
    store
      .save(&PersistedConfiguration {
        version: crate::config::CONFIG_VERSION,
        mode: ProcessingMode::Bypass,
        microphone_endpoint_id: "old-mic".to_owned(),
        microphone_uses_default: true,
        render_endpoint_id: None,
        render_uses_default: false,
        cable_input_endpoint_id: "persisted-input".to_owned(),
        cable_output_endpoint_id: "persisted-output".to_owned(),
      })
      .expect("persisted configuration saves");
    let tray = TrayEngine::new(store);
    let mut changed_inventory = inventory();
    changed_inventory.default_microphone = Some(source(InputRole::Microphone, "new-mic"));
    tray.initialize_defaults(&changed_inventory);
    assert_eq!(
      tray.draft().microphone_endpoint_id.as_deref(),
      Some("new-mic")
    );
    assert!(tray.microphone_default_selected());
    fs::remove_dir_all(root).expect("temporary configuration root removes");
  }

  #[test]
  fn direct_persisted_selection_is_not_replaced_by_current_default() {
    let (store, root) = temporary_store();
    store
      .save(&persisted_configuration(ProcessingMode::Bypass))
      .expect("persisted configuration saves");
    let tray = TrayEngine::new(store);
    tray.initialize_defaults(&inventory());
    assert_eq!(
      tray.draft().microphone_endpoint_id.as_deref(),
      Some("persisted-mic")
    );
    assert!(!tray.microphone_default_selected());
    fs::remove_dir_all(root).expect("temporary configuration root removes");
  }

  #[test]
  fn environment_override_has_priority_and_never_writes_back() {
    let (store, root) = temporary_store();
    let persisted = persisted_configuration(ProcessingMode::Bypass);
    store
      .save(&persisted)
      .expect("persisted configuration saves");
    let environment = TrayConfiguration {
      mode: ProcessingMode::Aec,
      microphone_endpoint_id: "environment-mic".to_owned(),
      microphone_uses_default: false,
      render_endpoint_id: Some("environment-render".to_owned()),
      render_uses_default: false,
      cable_input_endpoint_id: "environment-input".to_owned(),
      cable_output_endpoint_id: "environment-output".to_owned(),
    };
    let (selected, source, error) = load_configuration_from(Ok(Some(environment.clone())), &store);
    assert_eq!(selected, Some(environment));
    assert_eq!(source, ConfigurationSource::Environment);
    assert_eq!(error, None);
    assert!(!should_persist_configuration(source));
    assert_eq!(
      store.load().expect("persisted configuration loads"),
      Some(persisted)
    );
    fs::remove_dir_all(root).expect("temporary configuration root removes");
  }

  #[test]
  fn invalid_environment_override_does_not_mix_with_persisted_values() {
    let (store, root) = temporary_store();
    let persisted = persisted_configuration(ProcessingMode::Bypass);
    store
      .save(&persisted)
      .expect("persisted configuration saves");
    let (selected, source, error) = load_configuration_from(
      Err(ConfigError::Invalid(
        "MINI_AEC_RENDER_ID is missing".to_owned(),
      )),
      &store,
    );
    assert_eq!(selected, None);
    assert_eq!(source, ConfigurationSource::None);
    assert!(error.is_some());
    fs::remove_dir_all(root).expect("temporary configuration root removes");
  }

  #[test]
  fn fresh_configuration_defaults_to_bypass_and_exact_system_candidates() {
    let mut draft = ConfigurationDraft::default();
    assert_eq!(draft.mode, ProcessingMode::Bypass);
    populate_default_draft(&mut draft, &inventory());
    assert_eq!(draft.microphone_endpoint_id.as_deref(), Some("mic"));
    assert_eq!(draft.render_endpoint_id.as_deref(), Some("render"));
    assert_eq!(
      draft.cable_input_endpoint_id.as_deref(),
      Some("cable-input")
    );
    assert_eq!(
      draft.cable_output_endpoint_id.as_deref(),
      Some("cable-output")
    );
  }

  #[test]
  fn missing_default_candidates_keep_the_draft_unrunnable() {
    let mut draft = ConfigurationDraft::default();
    let empty_inventory = DeviceInventory::default();
    populate_default_draft(&mut draft, &empty_inventory);
    assert!(draft
      .clone()
      .into_configuration(ProcessingMode::Bypass)
      .is_err());
  }

  #[test]
  fn configuration_validation_rejects_feedback_and_accepts_bypass_without_render() {
    let mut configuration = TrayConfiguration {
      mode: ProcessingMode::Aec,
      microphone_endpoint_id: "mic".to_owned(),
      microphone_uses_default: false,
      render_endpoint_id: Some("render".to_owned()),
      render_uses_default: false,
      cable_input_endpoint_id: "cable-input".to_owned(),
      cable_output_endpoint_id: "cable-output".to_owned(),
    };
    assert!(validate_configuration(&configuration, &inventory()).is_ok());
    configuration.microphone_endpoint_id = "cable-output".to_owned();
    assert!(validate_configuration(&configuration, &inventory()).is_err());
    configuration.microphone_endpoint_id = "mic".to_owned();
    configuration.mode = ProcessingMode::Bypass;
    configuration.render_endpoint_id = None;
    assert!(validate_configuration(&configuration, &inventory()).is_ok());
  }

  #[test]
  fn configuration_validation_uses_exact_ids_not_friendly_names() {
    let mut fake_inventory = inventory();
    fake_inventory.microphones[0].friendly_name = "CABLE Output".to_owned();
    let configuration = TrayConfiguration {
      mode: ProcessingMode::Bypass,
      microphone_endpoint_id: "mic".to_owned(),
      microphone_uses_default: false,
      render_endpoint_id: None,
      render_uses_default: false,
      cable_input_endpoint_id: "cable-input".to_owned(),
      cable_output_endpoint_id: "cable-output".to_owned(),
    };
    assert!(validate_configuration(&configuration, &fake_inventory).is_ok());
    let mut wrong_identity = configuration;
    wrong_identity.microphone_endpoint_id = "CABLE Output".to_owned();
    assert!(validate_configuration(&wrong_identity, &fake_inventory).is_err());
  }

  #[test]
  fn configuration_validation_rejects_ambiguous_vb_cable_pairs() {
    let mut fake_inventory = inventory();
    fake_inventory.cable_pairs.push(pair());
    let configuration = TrayConfiguration {
      mode: ProcessingMode::Bypass,
      microphone_endpoint_id: "mic".to_owned(),
      microphone_uses_default: false,
      render_endpoint_id: None,
      render_uses_default: false,
      cable_input_endpoint_id: "cable-input".to_owned(),
      cable_output_endpoint_id: "cable-output".to_owned(),
    };
    assert!(validate_configuration(&configuration, &fake_inventory).is_err());
  }

  #[test]
  fn configuration_draft_requires_roles_for_selected_mode() {
    let draft = ConfigurationDraft {
      mode: ProcessingMode::Bypass,
      microphone_endpoint_id: Some("mic".to_owned()),
      render_endpoint_id: None,
      cable_input_endpoint_id: Some("input".to_owned()),
      cable_output_endpoint_id: Some("output".to_owned()),
    };
    assert!(draft
      .clone()
      .into_configuration(ProcessingMode::Bypass)
      .is_ok());
    assert!(draft.into_configuration(ProcessingMode::Aec).is_err());
  }
}
