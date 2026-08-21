//! Production release contracts and non-mutating lifecycle checks for `MiniAEC`.
//!
//! This crate deliberately contains no Windows installation, signing, device, certificate,
//! boot-state, or default-role mutation. It validates a release package and models the state
//! transitions and inventory comparisons that an approved lifecycle boundary must enforce.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::fs;
use std::path::{Component as PathComponent, Path, PathBuf};

use semver::Version;
use serde::{Deserialize, Serialize};

pub const RELEASE_MANIFEST_SCHEMA_VERSION: u16 = 1;
pub const INVENTORY_SCHEMA_VERSION: u16 = 1;
pub const PRODUCT_NAME: &str = "MiniAEC";
pub const PRODUCT_IDENTIFIER: &str = "mini-aec";
pub const SUPPORTED_OS: &str = "windows-11";
pub const SUPPORTED_ARCHITECTURE: &str = "x86_64";
pub const PUBLIC_ENDPOINT_NAME: &str = "MiniAEC Microphone";
pub const PRODUCER_INTERFACE_NAME: &str = "MiniAECTransport";
pub const TRANSPORT_PROTOCOL_VERSION: u16 = 1;
pub const DIAGNOSTICS_SCHEMA_VERSION: u16 = 2;
pub const MANIFEST_FILE_NAME: &str = "manifest.json";

const PRIVATE_SIGNING_SUFFIXES: [&str; 5] = ["pfx", "p12", "pvk", "key", "snk"];

/// A complete production release identity and compatibility contract.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifest {
  pub schema_version: u16,
  pub product: ProductIdentity,
  pub release_version: String,
  pub target: TargetIdentity,
  pub runtime: ComponentIdentity,
  pub driver: ComponentIdentity,
  pub transport: TransportContract,
  pub compatibility: CompatibilityRange,
  pub trust: TrustEvidence,
  pub files: PackageFiles,
}

/// Product name and stable package identifier.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProductIdentity {
  pub name: String,
  pub identifier: String,
}

/// Supported operating-system and CPU target.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TargetIdentity {
  pub os: String,
  pub architecture: String,
}

/// Versioned component identity within one release unit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentIdentity {
  pub identifier: String,
  pub version: String,
}

/// Fixed public and private transport contract that production packages must preserve.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransportContract {
  pub public_endpoint_name: String,
  pub producer_interface_name: String,
  pub protocol_version: u16,
  pub diagnostics_schema_version: u16,
  pub sample_rate_hz: u32,
  pub channels: u16,
  pub bits_per_sample: u16,
  pub frame_samples_per_channel: u16,
}

/// Component versions accepted by a candidate release during upgrade.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibilityRange {
  pub runtime_minimum: String,
  pub runtime_maximum: Option<String>,
  pub driver_minimum: String,
  pub driver_maximum: Option<String>,
}

impl CompatibilityRange {
  fn validate(&self) -> Result<(), ManifestError> {
    validate_version_range(
      "compatibility.runtime_minimum",
      &self.runtime_minimum,
      "compatibility.runtime_maximum",
      self.runtime_maximum.as_deref(),
    )?;
    validate_version_range(
      "compatibility.driver_minimum",
      &self.driver_minimum,
      "compatibility.driver_maximum",
      self.driver_maximum.as_deref(),
    )?;
    Ok(())
  }

  fn accepts(&self, runtime: &Version, driver: &Version) -> bool {
    accepts_version(
      &self.runtime_minimum,
      self.runtime_maximum.as_deref(),
      runtime,
    ) && accepts_version(&self.driver_minimum, self.driver_maximum.as_deref(), driver)
  }
}

/// Public evidence that a package uses the approved production trust path.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrustEvidence {
  pub channel: ReleaseChannel,
  pub signer_subject: String,
  pub signer_thumbprint: String,
  pub signature_verified: bool,
  pub test_signing_required: bool,
  pub private_signing_material_present: bool,
}

/// Release channel carried by a package manifest.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseChannel {
  Development,
  Production,
}

/// Relative files required by a production release package.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageFiles {
  pub runtime_executable: String,
  pub driver_inf: String,
  pub driver_binary: String,
  pub driver_catalog: String,
  pub signing_evidence: String,
}

impl PackageFiles {
  fn paths(&self) -> [&str; 5] {
    [
      &self.runtime_executable,
      &self.driver_inf,
      &self.driver_binary,
      &self.driver_catalog,
      &self.signing_evidence,
    ]
  }
}

impl ReleaseManifest {
  /// Parses a manifest without performing filesystem or Windows state access.
  ///
  /// # Errors
  ///
  /// Returns an error when the input is not a complete manifest JSON document.
  pub fn from_json(json: &str) -> Result<Self, ManifestError> {
    serde_json::from_str(json).map_err(|error| ManifestError::InvalidJson(error.to_string()))
  }

  /// Reads and parses a manifest file without changing any state.
  ///
  /// # Errors
  ///
  /// Returns an error when the file cannot be read or contains invalid manifest JSON.
  pub fn from_path(path: &Path) -> Result<Self, ReleaseError> {
    let json = fs::read_to_string(path).map_err(|error| ReleaseError::Io {
      path: path.to_path_buf(),
      message: error.to_string(),
    })?;
    Self::from_json(&json).map_err(ReleaseError::Manifest)
  }

  /// Validates the fixed `MiniAEC` product and transport contracts.
  ///
  /// # Errors
  ///
  /// Returns an error when the product identity, target, transport, compatibility range, trust
  /// evidence, or package paths do not satisfy the production contract.
  pub fn validate(&self) -> Result<(), ManifestError> {
    if self.schema_version != RELEASE_MANIFEST_SCHEMA_VERSION {
      return Err(ManifestError::UnsupportedValue {
        field: "schema_version",
        expected: RELEASE_MANIFEST_SCHEMA_VERSION.to_string(),
        actual: self.schema_version.to_string(),
      });
    }
    require_exact("product.name", &self.product.name, PRODUCT_NAME)?;
    require_exact(
      "product.identifier",
      &self.product.identifier,
      PRODUCT_IDENTIFIER,
    )?;
    require_version("release_version", &self.release_version)?;
    require_exact("target.os", &self.target.os, SUPPORTED_OS)?;
    require_exact(
      "target.architecture",
      &self.target.architecture,
      SUPPORTED_ARCHITECTURE,
    )?;
    require_exact(
      "runtime.identifier",
      &self.runtime.identifier,
      PRODUCT_IDENTIFIER,
    )?;
    require_version("runtime.version", &self.runtime.version)?;
    require_exact(
      "driver.identifier",
      &self.driver.identifier,
      "mini-aec-windows-driver",
    )?;
    require_version("driver.version", &self.driver.version)?;

    require_exact(
      "transport.public_endpoint_name",
      &self.transport.public_endpoint_name,
      PUBLIC_ENDPOINT_NAME,
    )?;
    require_exact(
      "transport.producer_interface_name",
      &self.transport.producer_interface_name,
      PRODUCER_INTERFACE_NAME,
    )?;
    require_number(
      "transport.protocol_version",
      &self.transport.protocol_version,
      &TRANSPORT_PROTOCOL_VERSION,
    )?;
    require_number(
      "transport.diagnostics_schema_version",
      &self.transport.diagnostics_schema_version,
      &DIAGNOSTICS_SCHEMA_VERSION,
    )?;
    require_number(
      "transport.sample_rate_hz",
      &self.transport.sample_rate_hz,
      &48_000,
    )?;
    require_number("transport.channels", &self.transport.channels, &1)?;
    require_number(
      "transport.bits_per_sample",
      &self.transport.bits_per_sample,
      &16,
    )?;
    require_number(
      "transport.frame_samples_per_channel",
      &self.transport.frame_samples_per_channel,
      &480,
    )?;
    self.compatibility.validate()?;

    if self.trust.channel != ReleaseChannel::Production {
      return Err(ManifestError::UnsupportedValue {
        field: "trust.channel",
        expected: "production".to_owned(),
        actual: format!("{:?}", self.trust.channel).to_ascii_lowercase(),
      });
    }
    require_non_empty("trust.signer_subject", &self.trust.signer_subject)?;
    require_non_empty("trust.signer_thumbprint", &self.trust.signer_thumbprint)?;
    require_true(
      "trust.signature_verified",
      self.trust.signature_verified,
      "signature verification must be recorded as successful",
    )?;
    require_false(
      "trust.test_signing_required",
      self.trust.test_signing_required,
      "production packages cannot require TESTSIGNING",
    )?;
    require_false(
      "trust.private_signing_material_present",
      self.trust.private_signing_material_present,
      "private signing material must remain outside the package",
    )?;

    for (index, path) in self.files.paths().into_iter().enumerate() {
      validate_relative_path("files", index, path)?;
    }
    Ok(())
  }

  /// Checks whether this candidate manifest can replace the installed manifest.
  ///
  /// # Errors
  ///
  /// Returns an error when the product identity, target, transport contract, or installed
  /// component versions are not accepted by the candidate.
  pub fn is_compatible_with(&self, installed: &Self) -> Result<(), CompatibilityError> {
    if self.product != installed.product {
      return Err(CompatibilityError::ProductIdentity);
    }
    if self.target != installed.target {
      return Err(CompatibilityError::Target);
    }
    if self.transport != installed.transport {
      return Err(CompatibilityError::TransportContract);
    }
    let runtime = parse_version_for_compatibility("runtime.version", &installed.runtime.version)?;
    let driver = parse_version_for_compatibility("driver.version", &installed.driver.version)?;
    if !self.compatibility.accepts(&runtime, &driver) {
      return Err(CompatibilityError::InstalledReleaseOutsideCandidateRange);
    }
    Ok(())
  }
}

/// Result of a read-only production package preflight.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PackagePreflight {
  pub package_root: PathBuf,
  pub manifest: ReleaseManifest,
  pub verified_files: Vec<PathBuf>,
}

/// Validates a package directory without installing or modifying it.
///
/// # Errors
///
/// Returns an error when the package directory, manifest, required files, relative paths, or
/// signing-material boundary is invalid.
pub fn preflight_package(root: &Path) -> Result<PackagePreflight, ReleaseError> {
  if !root.is_dir() {
    return Err(ReleaseError::PackageRootMissing(root.to_path_buf()));
  }
  let manifest_path = root.join(MANIFEST_FILE_NAME);
  let manifest = ReleaseManifest::from_path(&manifest_path)?;
  manifest.validate().map_err(ReleaseError::Manifest)?;

  let mut verified_files = vec![manifest_path];
  for path in manifest.files.paths() {
    let relative = safe_relative_path(path)?;
    let full_path = root.join(relative);
    if !full_path.is_file() {
      return Err(ReleaseError::RequiredFileMissing(full_path));
    }
    verified_files.push(full_path);
  }
  if let Some(private_path) = find_private_signing_material(root)? {
    return Err(ReleaseError::PrivateSigningMaterial(private_path));
  }

  Ok(PackagePreflight {
    package_root: root.to_path_buf(),
    manifest,
    verified_files,
  })
}

/// Installation state modeled independently from Windows APIs or package tooling.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
  New,
  Preflighted,
  Authorized,
  Staged,
  AwaitingUserRestart,
  Activated,
  Verified,
  RecoveryRequired,
  RolledBack,
  Uninstalled,
}

/// Events accepted by the lifecycle state model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleEvent {
  PreflightPassed,
  UserAuthorized,
  PackageStaged,
  RestartRequired,
  ActivationCompleted,
  VerificationPassed,
  OperationFailed,
  RollbackVerified,
  UninstallVerified,
}

/// Pure state machine for the production lifecycle boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LifecycleStateMachine {
  state: LifecycleState,
}

impl Default for LifecycleStateMachine {
  fn default() -> Self {
    Self::new()
  }
}

impl LifecycleStateMachine {
  #[must_use]
  pub const fn new() -> Self {
    Self {
      state: LifecycleState::New,
    }
  }

  #[must_use]
  pub const fn state(self) -> LifecycleState {
    self.state
  }

  /// Applies one lifecycle event and refuses invalid transitions.
  ///
  /// # Errors
  ///
  /// Returns an error when the event is not valid for the current lifecycle state.
  pub fn apply(self, event: LifecycleEvent) -> Result<Self, TransitionError> {
    let next = match (self.state, event) {
      (LifecycleState::New, LifecycleEvent::PreflightPassed) => LifecycleState::Preflighted,
      (LifecycleState::Preflighted, LifecycleEvent::UserAuthorized) => LifecycleState::Authorized,
      (LifecycleState::Authorized, LifecycleEvent::PackageStaged) => LifecycleState::Staged,
      (LifecycleState::Staged, LifecycleEvent::RestartRequired) => {
        LifecycleState::AwaitingUserRestart
      }
      (
        LifecycleState::Staged | LifecycleState::AwaitingUserRestart,
        LifecycleEvent::ActivationCompleted,
      ) => LifecycleState::Activated,
      (LifecycleState::Activated, LifecycleEvent::VerificationPassed) => LifecycleState::Verified,
      (
        LifecycleState::Staged
        | LifecycleState::AwaitingUserRestart
        | LifecycleState::Activated
        | LifecycleState::Verified,
        LifecycleEvent::OperationFailed,
      ) => LifecycleState::RecoveryRequired,
      (LifecycleState::RecoveryRequired, LifecycleEvent::RollbackVerified) => {
        LifecycleState::RolledBack
      }
      (LifecycleState::Verified, LifecycleEvent::UninstallVerified) => LifecycleState::Uninstalled,
      _ => {
        return Err(TransitionError {
          state: self.state,
          event,
        })
      }
    };
    Ok(Self { state: next })
  }
}

/// Read-only inventory captured before or after a lifecycle operation.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleInventory {
  pub schema_version: u16,
  pub product_release: Option<String>,
  pub public_endpoint_name: Option<String>,
  pub public_endpoint_identity: Option<String>,
  pub producer_interface_present: bool,
  pub services: BTreeSet<String>,
  pub driver_packages: BTreeSet<String>,
  pub default_input_roles: BTreeMap<String, String>,
  pub active_session: bool,
  pub unrelated_audio_endpoints: BTreeSet<String>,
}

/// One observable mismatch between expected and observed lifecycle inventory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InventoryDifference {
  pub field: String,
  pub expected: String,
  pub observed: String,
}

/// Result of comparing an observed inventory with a saved baseline.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InventoryComparison {
  pub differences: Vec<InventoryDifference>,
}

impl InventoryComparison {
  #[must_use]
  pub const fn is_clean(&self) -> bool {
    self.differences.is_empty()
  }
}

impl LifecycleInventory {
  /// Compares all lifecycle-relevant fields without reading Windows state.
  #[must_use]
  pub fn compare_to(&self, expected: &Self) -> InventoryComparison {
    let mut differences = Vec::new();
    compare_field(
      &mut differences,
      "schema_version",
      &expected.schema_version,
      &self.schema_version,
    );
    compare_field(
      &mut differences,
      "product_release",
      &expected.product_release,
      &self.product_release,
    );
    compare_field(
      &mut differences,
      "public_endpoint_name",
      &expected.public_endpoint_name,
      &self.public_endpoint_name,
    );
    compare_field(
      &mut differences,
      "public_endpoint_identity",
      &expected.public_endpoint_identity,
      &self.public_endpoint_identity,
    );
    compare_field(
      &mut differences,
      "producer_interface_present",
      &expected.producer_interface_present,
      &self.producer_interface_present,
    );
    compare_field(
      &mut differences,
      "services",
      &expected.services,
      &self.services,
    );
    compare_field(
      &mut differences,
      "driver_packages",
      &expected.driver_packages,
      &self.driver_packages,
    );
    compare_field(
      &mut differences,
      "default_input_roles",
      &expected.default_input_roles,
      &self.default_input_roles,
    );
    compare_field(
      &mut differences,
      "active_session",
      &expected.active_session,
      &self.active_session,
    );
    compare_field(
      &mut differences,
      "unrelated_audio_endpoints",
      &expected.unrelated_audio_endpoints,
      &self.unrelated_audio_endpoints,
    );
    InventoryComparison { differences }
  }
}

/// Manifest validation and package errors.
#[derive(Debug, Eq, PartialEq)]
pub enum ReleaseError {
  Io { path: PathBuf, message: String },
  Manifest(ManifestError),
  PackageRootMissing(PathBuf),
  RequiredFileMissing(PathBuf),
  UnsafeRelativePath(String),
  PrivateSigningMaterial(PathBuf),
}

impl Display for ReleaseError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    match self {
      Self::Io { path, message } => write!(formatter, "I/O error at {}: {message}", path.display()),
      Self::Manifest(error) => write!(formatter, "manifest validation failed: {error}"),
      Self::PackageRootMissing(path) => {
        write!(
          formatter,
          "production package directory is missing: {}",
          path.display()
        )
      }
      Self::RequiredFileMissing(path) => {
        write!(
          formatter,
          "required production package file is missing: {}",
          path.display()
        )
      }
      Self::UnsafeRelativePath(path) => {
        write!(
          formatter,
          "package manifest contains an unsafe relative path: {path}"
        )
      }
      Self::PrivateSigningMaterial(path) => write!(
        formatter,
        "private signing material is present in the production package: {}",
        path.display()
      ),
    }
  }
}

impl Error for ReleaseError {}

/// Manifest parsing and contract validation errors.
#[derive(Debug, Eq, PartialEq)]
pub enum ManifestError {
  InvalidJson(String),
  EmptyField(&'static str),
  InvalidVersion {
    field: &'static str,
    value: String,
  },
  InvalidVersionRange {
    minimum: &'static str,
    maximum: &'static str,
  },
  UnsupportedValue {
    field: &'static str,
    expected: String,
    actual: String,
  },
  InvalidBoolean {
    field: &'static str,
    message: &'static str,
  },
  UnsafePath {
    field: &'static str,
    value: String,
  },
}

impl Display for ManifestError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    match self {
      Self::InvalidJson(message) => write!(formatter, "invalid JSON: {message}"),
      Self::EmptyField(field) => write!(formatter, "{field} must not be empty"),
      Self::InvalidVersion { field, value } => {
        write!(formatter, "{field} is not a semantic version: {value}")
      }
      Self::InvalidVersionRange { minimum, maximum } => {
        write!(
          formatter,
          "{maximum} must be greater than or equal to {minimum}"
        )
      }
      Self::UnsupportedValue {
        field,
        expected,
        actual,
      } => write!(formatter, "{field} must be {expected}, got {actual}"),
      Self::InvalidBoolean { field, message } => write!(formatter, "{field}: {message}"),
      Self::UnsafePath { field, value } => write!(formatter, "{field} is unsafe: {value}"),
    }
  }
}

impl Error for ManifestError {}

/// Incompatibility detected before an upgrade or activation.
#[derive(Debug, Eq, PartialEq)]
pub enum CompatibilityError {
  ProductIdentity,
  Target,
  TransportContract,
  InvalidVersion { field: &'static str, value: String },
  InstalledReleaseOutsideCandidateRange,
}

impl Display for CompatibilityError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    match self {
      Self::ProductIdentity => write!(formatter, "product identities do not match"),
      Self::Target => write!(formatter, "release targets do not match"),
      Self::TransportContract => write!(formatter, "transport contracts do not match"),
      Self::InvalidVersion { field, value } => {
        write!(formatter, "{field} is not a semantic version: {value}")
      }
      Self::InstalledReleaseOutsideCandidateRange => {
        write!(
          formatter,
          "installed component versions are outside the candidate compatibility range"
        )
      }
    }
  }
}

impl Error for CompatibilityError {}

/// Invalid state-machine transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionError {
  pub state: LifecycleState,
  pub event: LifecycleEvent,
}

impl Display for TransitionError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    write!(
      formatter,
      "event {:?} is not valid from state {:?}",
      self.event, self.state
    )
  }
}

impl Error for TransitionError {}

fn require_exact(field: &'static str, actual: &str, expected: &str) -> Result<(), ManifestError> {
  if actual == expected {
    Ok(())
  } else {
    Err(ManifestError::UnsupportedValue {
      field,
      expected: expected.to_owned(),
      actual: actual.to_owned(),
    })
  }
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), ManifestError> {
  if value.trim().is_empty() {
    Err(ManifestError::EmptyField(field))
  } else {
    Ok(())
  }
}

fn require_version(field: &'static str, value: &str) -> Result<Version, ManifestError> {
  require_non_empty(field, value)?;
  Version::parse(value).map_err(|_| ManifestError::InvalidVersion {
    field,
    value: value.to_owned(),
  })
}

fn require_number<T>(field: &'static str, actual: &T, expected: &T) -> Result<(), ManifestError>
where
  T: Display + Eq,
{
  if actual == expected {
    Ok(())
  } else {
    Err(ManifestError::UnsupportedValue {
      field,
      expected: expected.to_string(),
      actual: actual.to_string(),
    })
  }
}

fn require_true(
  field: &'static str,
  value: bool,
  message: &'static str,
) -> Result<(), ManifestError> {
  if value {
    Ok(())
  } else {
    Err(ManifestError::InvalidBoolean { field, message })
  }
}

fn require_false(
  field: &'static str,
  value: bool,
  message: &'static str,
) -> Result<(), ManifestError> {
  if value {
    Err(ManifestError::InvalidBoolean { field, message })
  } else {
    Ok(())
  }
}

fn validate_version_range(
  minimum_field: &'static str,
  minimum: &str,
  maximum_field: &'static str,
  maximum: Option<&str>,
) -> Result<(), ManifestError> {
  let minimum_version = require_version(minimum_field, minimum)?;
  if let Some(maximum) = maximum {
    let maximum_version = require_version(maximum_field, maximum)?;
    if maximum_version < minimum_version {
      return Err(ManifestError::InvalidVersionRange {
        minimum: minimum_field,
        maximum: maximum_field,
      });
    }
  }
  Ok(())
}

fn accepts_version(minimum: &str, maximum: Option<&str>, version: &Version) -> bool {
  let Ok(minimum) = Version::parse(minimum) else {
    return false;
  };
  let maximum_is_valid =
    maximum.is_none_or(|maximum| Version::parse(maximum).is_ok_and(|maximum| version <= &maximum));
  version >= &minimum && maximum_is_valid
}

fn validate_relative_path(
  group: &'static str,
  index: usize,
  value: &str,
) -> Result<(), ManifestError> {
  require_non_empty(group, value)?;
  let path = Path::new(value);
  let safe = !path.is_absolute()
    && path
      .components()
      .all(|component| !matches!(component, PathComponent::ParentDir | PathComponent::RootDir));
  if safe {
    Ok(())
  } else {
    Err(ManifestError::UnsafePath {
      field: "files",
      value: format!("index {index}: {value}"),
    })
  }
}

fn safe_relative_path(value: &str) -> Result<&Path, ReleaseError> {
  let path = Path::new(value);
  let safe = !path.is_absolute()
    && path
      .components()
      .all(|component| !matches!(component, PathComponent::ParentDir | PathComponent::RootDir));
  if safe {
    Ok(path)
  } else {
    Err(ReleaseError::UnsafeRelativePath(value.to_owned()))
  }
}

fn find_private_signing_material(root: &Path) -> Result<Option<PathBuf>, ReleaseError> {
  let entries = fs::read_dir(root).map_err(|error| ReleaseError::Io {
    path: root.to_path_buf(),
    message: error.to_string(),
  })?;
  for entry in entries {
    let entry = entry.map_err(|error| ReleaseError::Io {
      path: root.to_path_buf(),
      message: error.to_string(),
    })?;
    let path = entry.path();
    let file_type = entry.file_type().map_err(|error| ReleaseError::Io {
      path: path.clone(),
      message: error.to_string(),
    })?;
    if file_type.is_dir() {
      if let Some(found) = find_private_signing_material(&path)? {
        return Ok(Some(found));
      }
    } else if file_type.is_file()
      && path.extension().is_some_and(|extension| {
        let extension = extension.to_string_lossy().to_ascii_lowercase();
        PRIVATE_SIGNING_SUFFIXES.contains(&extension.as_str())
      })
    {
      return Ok(Some(path));
    }
  }
  Ok(None)
}

fn parse_version_for_compatibility(
  field: &'static str,
  value: &str,
) -> Result<Version, CompatibilityError> {
  Version::parse(value).map_err(|_| CompatibilityError::InvalidVersion {
    field,
    value: value.to_owned(),
  })
}

fn compare_field<T>(
  differences: &mut Vec<InventoryDifference>,
  field: &str,
  expected: &T,
  observed: &T,
) where
  T: fmt::Debug + PartialEq,
{
  if expected != observed {
    differences.push(InventoryDifference {
      field: field.to_owned(),
      expected: format!("{expected:?}"),
      observed: format!("{observed:?}"),
    });
  }
}

#[cfg(test)]
mod tests {
  use std::fs;
  use std::sync::atomic::{AtomicU64, Ordering};

  use super::*;

  static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

  fn valid_manifest() -> ReleaseManifest {
    ReleaseManifest {
      schema_version: RELEASE_MANIFEST_SCHEMA_VERSION,
      product: ProductIdentity {
        name: PRODUCT_NAME.to_owned(),
        identifier: PRODUCT_IDENTIFIER.to_owned(),
      },
      release_version: "0.1.0".to_owned(),
      target: TargetIdentity {
        os: SUPPORTED_OS.to_owned(),
        architecture: SUPPORTED_ARCHITECTURE.to_owned(),
      },
      runtime: ComponentIdentity {
        identifier: PRODUCT_IDENTIFIER.to_owned(),
        version: "0.1.0".to_owned(),
      },
      driver: ComponentIdentity {
        identifier: "mini-aec-windows-driver".to_owned(),
        version: "0.1.0".to_owned(),
      },
      transport: TransportContract {
        public_endpoint_name: PUBLIC_ENDPOINT_NAME.to_owned(),
        producer_interface_name: PRODUCER_INTERFACE_NAME.to_owned(),
        protocol_version: TRANSPORT_PROTOCOL_VERSION,
        diagnostics_schema_version: DIAGNOSTICS_SCHEMA_VERSION,
        sample_rate_hz: 48_000,
        channels: 1,
        bits_per_sample: 16,
        frame_samples_per_channel: 480,
      },
      compatibility: CompatibilityRange {
        runtime_minimum: "0.1.0".to_owned(),
        runtime_maximum: Some("0.2.0".to_owned()),
        driver_minimum: "0.1.0".to_owned(),
        driver_maximum: Some("0.2.0".to_owned()),
      },
      trust: TrustEvidence {
        channel: ReleaseChannel::Production,
        signer_subject: "CN=MiniAEC Production".to_owned(),
        signer_thumbprint: "0123456789ABCDEF0123456789ABCDEF01234567".to_owned(),
        signature_verified: true,
        test_signing_required: false,
        private_signing_material_present: false,
      },
      files: PackageFiles {
        runtime_executable: "runtime/mini-aec.exe".to_owned(),
        driver_inf: "driver/MiniAECProduction.inf".to_owned(),
        driver_binary: "driver/MiniAECProduction.sys".to_owned(),
        driver_catalog: "driver/MiniAECProduction.cat".to_owned(),
        signing_evidence: "trust/signing-evidence.json".to_owned(),
      },
    }
  }

  fn package_directory() -> PathBuf {
    let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
      "mini-aec-release-{}-{sequence}",
      std::process::id()
    ))
  }

  fn write_package(manifest: &ReleaseManifest) -> PathBuf {
    let root = package_directory();
    fs::create_dir_all(root.join("runtime")).expect("create runtime directory");
    fs::create_dir_all(root.join("driver")).expect("create driver directory");
    fs::create_dir_all(root.join("trust")).expect("create trust directory");
    fs::write(
      root.join(MANIFEST_FILE_NAME),
      serde_json::to_vec_pretty(manifest).expect("serialize manifest"),
    )
    .expect("write manifest");
    for path in manifest.files.paths() {
      fs::write(root.join(path), b"synthetic production artifact").expect("write package file");
    }
    root
  }

  #[test]
  fn production_manifest_validates() {
    assert!(valid_manifest().validate().is_ok());
  }

  #[test]
  fn development_channel_is_rejected() {
    let mut manifest = valid_manifest();
    manifest.trust.channel = ReleaseChannel::Development;
    assert!(matches!(
      manifest.validate(),
      Err(ManifestError::UnsupportedValue {
        field: "trust.channel",
        ..
      })
    ));
  }

  #[test]
  fn wrong_transport_contract_is_rejected() {
    let mut manifest = valid_manifest();
    manifest.transport.public_endpoint_name = "Other Microphone".to_owned();
    assert!(matches!(
      manifest.validate(),
      Err(ManifestError::UnsupportedValue {
        field: "transport.public_endpoint_name",
        ..
      })
    ));
  }

  #[test]
  fn preflight_requires_all_files_and_rejects_private_material() {
    let manifest = valid_manifest();
    let root = write_package(&manifest);
    assert!(preflight_package(&root).is_ok());

    fs::write(root.join("trust/private.key"), b"not a real key").expect("write private marker");
    assert!(matches!(
      preflight_package(&root),
      Err(ReleaseError::PrivateSigningMaterial(_))
    ));
    fs::remove_dir_all(root).expect("remove test package");
  }

  #[test]
  fn candidate_accepts_compatible_installed_release() {
    let installed = valid_manifest();
    let mut candidate = valid_manifest();
    candidate.release_version = "0.2.0".to_owned();
    candidate.runtime.version = "0.2.0".to_owned();
    candidate.driver.version = "0.2.0".to_owned();
    assert!(candidate.is_compatible_with(&installed).is_ok());
  }

  #[test]
  fn candidate_rejects_changed_transport_contract() {
    let installed = valid_manifest();
    let mut candidate = valid_manifest();
    candidate.transport.protocol_version = 2;
    assert_eq!(
      candidate.is_compatible_with(&installed),
      Err(CompatibilityError::TransportContract)
    );
  }

  #[test]
  fn lifecycle_requires_verification_before_uninstall() {
    let machine = LifecycleStateMachine::new()
      .apply(LifecycleEvent::PreflightPassed)
      .expect("preflight")
      .apply(LifecycleEvent::UserAuthorized)
      .expect("authorization")
      .apply(LifecycleEvent::PackageStaged)
      .expect("staging")
      .apply(LifecycleEvent::ActivationCompleted)
      .expect("activation");
    assert_eq!(
      machine.apply(LifecycleEvent::UninstallVerified),
      Err(TransitionError {
        state: LifecycleState::Activated,
        event: LifecycleEvent::UninstallVerified,
      })
    );
  }

  #[test]
  fn lifecycle_allows_manual_restart_boundary_and_rollback() {
    let machine = LifecycleStateMachine::new()
      .apply(LifecycleEvent::PreflightPassed)
      .expect("preflight")
      .apply(LifecycleEvent::UserAuthorized)
      .expect("authorization")
      .apply(LifecycleEvent::PackageStaged)
      .expect("staging")
      .apply(LifecycleEvent::RestartRequired)
      .expect("restart boundary");
    assert_eq!(machine.state(), LifecycleState::AwaitingUserRestart);
    let machine = machine
      .apply(LifecycleEvent::ActivationCompleted)
      .expect("activation")
      .apply(LifecycleEvent::OperationFailed)
      .expect("failure")
      .apply(LifecycleEvent::RollbackVerified)
      .expect("rollback");
    assert_eq!(machine.state(), LifecycleState::RolledBack);
  }

  #[test]
  fn inventory_comparison_reports_default_role_and_endpoint_differences() {
    let expected = LifecycleInventory {
      schema_version: INVENTORY_SCHEMA_VERSION,
      default_input_roles: BTreeMap::from([(
        String::from("console"),
        String::from("Physical Mic"),
      )]),
      unrelated_audio_endpoints: BTreeSet::from([String::from("Physical Mic")]),
      ..LifecycleInventory::default()
    };
    let observed = LifecycleInventory {
      schema_version: INVENTORY_SCHEMA_VERSION,
      public_endpoint_name: Some(PUBLIC_ENDPOINT_NAME.to_owned()),
      default_input_roles: BTreeMap::from([(
        String::from("console"),
        String::from(PUBLIC_ENDPOINT_NAME),
      )]),
      unrelated_audio_endpoints: BTreeSet::from([String::from("Physical Mic")]),
      ..LifecycleInventory::default()
    };
    let comparison = observed.compare_to(&expected);
    assert!(!comparison.is_clean());
    assert!(comparison
      .differences
      .iter()
      .any(|difference| difference.field == "public_endpoint_name"));
    assert!(comparison
      .differences
      .iter()
      .any(|difference| difference.field == "default_input_roles"));
  }
}
