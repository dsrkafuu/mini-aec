use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

use mini_aec_engine::ProcessingMode;
use serde::{Deserialize, Serialize};

pub(crate) const CONFIG_VERSION: u16 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ConfigError {
  Io(String),
  Invalid(String),
  Parse(String),
  UnsupportedVersion(u16),
}

impl Display for ConfigError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    match self {
      Self::Io(message) => write!(formatter, "configuration I/O failed: {message}"),
      Self::Invalid(message) => write!(formatter, "invalid configuration: {message}"),
      Self::Parse(message) => write!(formatter, "configuration JSON is invalid: {message}"),
      Self::UnsupportedVersion(version) => {
        write!(formatter, "unsupported configuration version: {version}")
      }
    }
  }
}

impl Error for ConfigError {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PersistedConfiguration {
  pub version: u16,
  pub mode: ProcessingMode,
  pub microphone_endpoint_id: String,
  #[serde(default)]
  pub microphone_uses_default: bool,
  pub render_endpoint_id: Option<String>,
  #[serde(default)]
  pub render_uses_default: bool,
  pub cable_input_endpoint_id: String,
  pub cable_output_endpoint_id: String,
}

impl PersistedConfiguration {
  pub(crate) fn validate(&self) -> Result<(), ConfigError> {
    if self.version != CONFIG_VERSION {
      return Err(ConfigError::UnsupportedVersion(self.version));
    }
    validate_id("microphone", &self.microphone_endpoint_id)?;
    validate_id("CABLE Input", &self.cable_input_endpoint_id)?;
    validate_id("CABLE Output", &self.cable_output_endpoint_id)?;
    if let Some(render) = &self.render_endpoint_id {
      validate_id("render", render)?;
    }
    if self.mode == ProcessingMode::Aec && self.render_endpoint_id.is_none() {
      return Err(ConfigError::Invalid(
        "AEC mode requires a physical render endpoint ID".to_owned(),
      ));
    }
    Ok(())
  }
}

fn validate_id(role: &str, id: &str) -> Result<(), ConfigError> {
  if id.trim().is_empty() {
    return Err(ConfigError::Invalid(format!(
      "{role} endpoint ID must not be empty"
    )));
  }
  Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConfigStore {
  path: PathBuf,
}

impl ConfigStore {
  #[must_use]
  pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
    Self { path: path.into() }
  }

  #[must_use]
  #[cfg(test)]
  pub(crate) fn path(&self) -> &Path {
    &self.path
  }

  pub(crate) fn load(&self) -> Result<Option<PersistedConfiguration>, ConfigError> {
    let bytes = match fs::read(&self.path) {
      Ok(bytes) => bytes,
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
      Err(error) => return Err(ConfigError::Io(error.to_string())),
    };
    let configuration = serde_json::from_slice::<PersistedConfiguration>(&bytes)
      .map_err(|error| ConfigError::Parse(error.to_string()))?;
    configuration.validate()?;
    Ok(Some(configuration))
  }

  pub(crate) fn save(&self, configuration: &PersistedConfiguration) -> Result<(), ConfigError> {
    configuration.validate()?;
    let parent = self
      .path
      .parent()
      .ok_or_else(|| ConfigError::Io("configuration path has no parent directory".to_owned()))?;
    fs::create_dir_all(parent).map_err(|error| ConfigError::Io(error.to_string()))?;
    let temporary_path = self.path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(configuration)
      .map_err(|error| ConfigError::Parse(error.to_string()))?;
    fs::write(&temporary_path, bytes).map_err(|error| ConfigError::Io(error.to_string()))?;
    replace_file(&temporary_path, &self.path)
  }
}

fn replace_file(temporary_path: &Path, destination: &Path) -> Result<(), ConfigError> {
  let backup_path = destination.with_extension("json.bak");
  if destination.exists() {
    let _ = fs::remove_file(&backup_path);
    fs::rename(destination, &backup_path).map_err(|error| ConfigError::Io(error.to_string()))?;
  }
  match fs::rename(temporary_path, destination) {
    Ok(()) => {
      let _ = fs::remove_file(backup_path);
      Ok(())
    }
    Err(error) => {
      if destination.exists() {
        let _ = fs::remove_file(destination);
      }
      if backup_path.exists() {
        let _ = fs::rename(&backup_path, destination);
      }
      Err(ConfigError::Io(error.to_string()))
    }
  }
}

#[cfg(test)]
mod tests {
  use std::fs;
  use std::time::{SystemTime, UNIX_EPOCH};

  use super::*;

  fn configuration(mode: ProcessingMode) -> PersistedConfiguration {
    PersistedConfiguration {
      version: CONFIG_VERSION,
      mode,
      microphone_endpoint_id: "mic-id".to_owned(),
      microphone_uses_default: false,
      render_endpoint_id: (mode == ProcessingMode::Aec).then(|| "render-id".to_owned()),
      render_uses_default: false,
      cable_input_endpoint_id: "cable-input-id".to_owned(),
      cable_output_endpoint_id: "cable-output-id".to_owned(),
    }
  }

  fn temporary_store() -> (ConfigStore, std::path::PathBuf) {
    let suffix = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .expect("system time is valid")
      .as_nanos();
    let root = std::env::temp_dir().join(format!("mini-aec-config-{suffix}"));
    (ConfigStore::new(root.join("config.json")), root)
  }

  #[test]
  fn configuration_validation_requires_expected_ids_and_version() {
    let mut valid = configuration(ProcessingMode::Aec);
    assert!(valid.validate().is_ok());
    valid.microphone_endpoint_id = " ".to_owned();
    assert!(matches!(valid.validate(), Err(ConfigError::Invalid(_))));

    let mut missing_render = configuration(ProcessingMode::Aec);
    missing_render.render_endpoint_id = None;
    assert!(matches!(
      missing_render.validate(),
      Err(ConfigError::Invalid(_))
    ));

    let mut unknown_version = configuration(ProcessingMode::Bypass);
    unknown_version.version = CONFIG_VERSION + 1;
    assert_eq!(
      unknown_version.validate(),
      Err(ConfigError::UnsupportedVersion(CONFIG_VERSION + 1))
    );
  }

  #[test]
  fn config_store_round_trips_and_ignores_incomplete_temporary_files() {
    let (store, root) = temporary_store();
    let expected = configuration(ProcessingMode::Aec);
    store.save(&expected).expect("configuration saves");
    let loaded = store.load().expect("configuration loads");
    assert_eq!(loaded, Some(expected.clone()));
    fs::write(store.path().with_extension("json.tmp"), b"incomplete").expect("temp file writes");
    assert_eq!(
      store.load().expect("existing configuration remains"),
      Some(expected)
    );
    fs::remove_dir_all(root).expect("temporary configuration root removes");
  }

  #[test]
  fn config_store_rejects_unknown_fields_and_never_serializes_audio_content() {
    let (store, root) = temporary_store();
    let mut value = serde_json::to_value(configuration(ProcessingMode::Bypass))
      .expect("configuration serializes");
    value["pcm"] = serde_json::json!("must-not-be-present");
    fs::create_dir_all(store.path().parent().expect("configuration parent")).expect("parent");
    fs::write(
      store.path(),
      serde_json::to_vec(&value).expect("test configuration serializes"),
    )
    .expect("test configuration writes");
    assert!(matches!(store.load(), Err(ConfigError::Parse(_))));

    store
      .save(&configuration(ProcessingMode::Bypass))
      .expect("valid configuration saves");
    let text = fs::read_to_string(store.path()).expect("saved configuration reads");
    assert!(!text.contains("pcm"));
    assert!(!text.contains("meeting"));
    fs::remove_dir_all(root).expect("temporary configuration root removes");
  }

  #[test]
  fn legacy_configuration_without_default_intent_loads_as_direct_selection() {
    let mut value = serde_json::to_value(configuration(ProcessingMode::Bypass))
      .expect("configuration serializes");
    let object = value.as_object_mut().expect("configuration is an object");
    object.remove("microphone_uses_default");
    object.remove("render_uses_default");
    let parsed = serde_json::from_value::<PersistedConfiguration>(value)
      .expect("legacy configuration remains readable");
    assert!(!parsed.microphone_uses_default);
    assert!(!parsed.render_uses_default);
  }
}
