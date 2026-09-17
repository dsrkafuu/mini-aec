//! Production release contracts and non-mutating lifecycle checks for `MiniAEC`.
//!
//! This crate deliberately contains no Windows installation, signing, device, certificate,
//! boot-state, or default-role mutation. It validates a release package and models the state
//! transitions and inventory comparisons that an approved lifecycle boundary must enforce.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{self, Display, Formatter, Write as _};
use std::fs;
use std::path::{Component as PathComponent, Path, PathBuf};

use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod lifecycle;

pub use lifecycle::{
  ActivationOutcome, LifecycleBackend, LifecycleCoordinator, LifecycleError, LifecycleJournal,
  LifecycleOperation, LifecyclePendingAction, LifecycleReleaseIdentity,
};

pub const RELEASE_MANIFEST_SCHEMA_VERSION: u16 = 1;
pub const PACKAGE_EVIDENCE_SCHEMA_VERSION: u16 = 1;
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
pub const SYSVAD_REPOSITORY: &str = "https://github.com/microsoft/Windows-driver-samples";
pub const SYSVAD_PATH: &str = "audio/sysvad";
pub const SYSVAD_COMMIT: &str = "2ee527bfeb0aeb6be11f0a8b6dce4011b358ce89";
pub const SYSVAD_LICENSE: &str = "MS-PL";
pub const SYSVAD_RECORD_PATH: &str = "driver/windows/UPSTREAM.md";
pub const PRODUCTION_DRIVER_IDENTITY: &str = "mini-aec-windows-driver";
pub const PRODUCTION_DRIVER_INF_PATH: &str = "driver/MiniAECProduction.inf";
pub const PRODUCTION_DRIVER_BINARY_PATH: &str = "driver/MiniAECProduction.sys";
pub const PRODUCTION_DRIVER_CATALOG_PATH: &str = "driver/MiniAECProduction.cat";
pub const PRODUCTION_DRIVER_NOTICE_PATH: &str = "trust/SysVAD-MS-PL.txt";
pub const PRODUCTION_SERVICE_NAME: &str = "MiniAECProduction";
pub const PRODUCTION_ENDPOINT_IDENTITY: &str = "Root\\MiniAECProduction";

const PRIVATE_SIGNING_SUFFIXES: [&str; 5] = ["pfx", "p12", "pvk", "key", "snk"];
const PRIVATE_AUDIO_SUFFIXES: [&str; 5] = ["wav", "flac", "mp3", "m4a", "pcm"];
const LOCAL_PATCH_PATHS: [&str; 5] = [
  "adapter.cpp",
  "EndpointsCommon/minwavertstream.cpp",
  "TabletAudioSample/micinwavtable.h",
  "TabletAudioSample/minipairs.h",
  "TabletAudioSample/TabletAudioSample.vcxproj",
];

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

/// Public, machine-readable evidence for a reproducible production package.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageEvidence {
  pub schema_version: u16,
  pub package_identifier: String,
  pub package_version: String,
  pub source: SourceProvenance,
  pub build: BuildProvenance,
  pub files: Vec<PackageFileEvidence>,
  pub catalog: CatalogEvidence,
  pub reproducibility: ReproducibilityEvidence,
  pub signing: SigningEvidence,
  pub verification: VerificationEvidence,
  pub privacy: PrivacyEvidence,
}

/// Immutable upstream and local-source identity used to produce a driver package.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceProvenance {
  pub repository: String,
  pub path: String,
  pub commit: String,
  pub license: String,
  pub record_path: String,
  pub source_tree_sha256: String,
  pub imported_file_set_sha256: String,
  pub local_patches: Vec<LocalPatchEvidence>,
}

/// One project-owned difference from the pinned upstream source slice.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalPatchEvidence {
  pub path: String,
  pub sha256: String,
}

/// Toolchain and command provenance for a package build.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildProvenance {
  pub windows: String,
  pub visual_studio: String,
  pub msbuild_version: String,
  pub msvc_version: String,
  pub sdk_version: String,
  pub wdk_version: String,
  pub inf2cat_version: String,
  pub signtool_version: String,
  pub configuration: String,
  pub platform: String,
  pub commands: Vec<String>,
  pub canonicalization: String,
}

/// Hash and size evidence for one release input or payload file.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackageFileEvidence {
  pub path: String,
  pub kind: String,
  pub sha256: String,
  pub size_bytes: u64,
}

/// Catalog member coverage for the final INF and SYS files.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEvidence {
  pub path: String,
  pub members: BTreeMap<String, String>,
  pub coverage_verified: bool,
}

/// Reproducibility evidence that excludes variable detached-signature bytes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReproducibilityEvidence {
  pub canonical_payload_sha256: String,
  pub catalog_member_set_sha256: String,
  pub replay_verified: bool,
  pub signature_bytes_excluded: bool,
}

/// Public trust result for the catalog and its INF/SYS members.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[allow(
  clippy::struct_excessive_bools,
  reason = "each catalog, coverage, embedded-signature, and TESTSIGNING result is independent evidence"
)]
pub struct SigningEvidence {
  pub route: String,
  pub signer_subject: String,
  pub signer_thumbprint: String,
  pub public_chain: Vec<CertificateEvidence>,
  pub cat_signature_verified: bool,
  pub inf_catalog_coverage_verified: bool,
  pub sys_catalog_coverage_verified: bool,
  pub sys_embedded_signature_required: bool,
  pub sys_embedded_signature_verified: bool,
  pub test_signing_required: bool,
  pub signature_timestamp: String,
}

/// Public certificate identity metadata; no private key material is allowed.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CertificateEvidence {
  pub subject: String,
  pub issuer: String,
  pub thumbprint: String,
  pub sha256: String,
}

/// Verification commands and read-only result state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationEvidence {
  pub verified: bool,
  pub replay_verified: bool,
  pub read_only: bool,
  pub tool: String,
  pub commands: Vec<String>,
}

/// Privacy and secret-material assertions for a release package.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
#[allow(
  clippy::struct_excessive_bools,
  reason = "each prohibited-content class is reported independently for release review"
)]
pub struct PrivacyEvidence {
  pub private_signing_material_present: bool,
  pub audio_content_present: bool,
  pub artifacts_content_present: bool,
  pub machine_secret_material_present: bool,
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
  #[allow(
    clippy::too_many_lines,
    reason = "the release manifest is the single validation boundary for all fixed package contracts"
  )]
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
      PRODUCTION_DRIVER_IDENTITY,
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

    require_exact(
      "files.runtime_executable",
      &self.files.runtime_executable,
      "runtime/mini-aec.exe",
    )?;
    require_exact(
      "files.driver_inf",
      &self.files.driver_inf,
      PRODUCTION_DRIVER_INF_PATH,
    )?;
    require_exact(
      "files.driver_binary",
      &self.files.driver_binary,
      PRODUCTION_DRIVER_BINARY_PATH,
    )?;
    require_exact(
      "files.driver_catalog",
      &self.files.driver_catalog,
      PRODUCTION_DRIVER_CATALOG_PATH,
    )?;
    require_exact(
      "files.signing_evidence",
      &self.files.signing_evidence,
      "trust/signing-evidence.json",
    )?;

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
  pub evidence: PackageEvidence,
  pub verified_files: Vec<PathBuf>,
}

/// Validates a package directory without installing or modifying it.
///
/// # Errors
///
/// Returns an error when the package directory, manifest, evidence, required files, hashes,
/// signatures, relative paths, or signing-material boundary is invalid.
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
  let notice_path = root.join(safe_relative_path(PRODUCTION_DRIVER_NOTICE_PATH)?);
  if !notice_path.is_file() {
    return Err(ReleaseError::RequiredFileMissing(notice_path));
  }
  verified_files.push(notice_path);
  if let Some(private_path) = find_private_signing_material(root)? {
    return Err(ReleaseError::PrivateSigningMaterial(private_path));
  }
  if let Some(forbidden_path) = find_forbidden_package_content(root)? {
    return Err(ReleaseError::ForbiddenPackageContent(forbidden_path));
  }

  let evidence_path = root.join(safe_relative_path(&manifest.files.signing_evidence)?);
  let evidence_json = fs::read_to_string(&evidence_path).map_err(|error| ReleaseError::Io {
    path: evidence_path.clone(),
    message: error.to_string(),
  })?;
  let evidence: PackageEvidence = serde_json::from_str(&evidence_json).map_err(|error| {
    ReleaseError::Evidence(PackageEvidenceError::InvalidJson(error.to_string()))
  })?;
  validate_package_evidence(root, &manifest, &evidence)?;

  Ok(PackagePreflight {
    package_root: root.to_path_buf(),
    manifest,
    evidence,
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
  UninstallStaged,
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
  UserRestartObserved,
  ActivationCompleted,
  VerificationPassed,
  OperationFailed,
  UninstallRequested,
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
      (LifecycleState::AwaitingUserRestart, LifecycleEvent::UserRestartObserved) => {
        LifecycleState::Staged
      }
      (LifecycleState::Staged, LifecycleEvent::ActivationCompleted) => LifecycleState::Activated,
      (LifecycleState::Activated, LifecycleEvent::VerificationPassed) => LifecycleState::Verified,
      (LifecycleState::Authorized, LifecycleEvent::UninstallRequested) => {
        LifecycleState::UninstallStaged
      }
      (
        LifecycleState::Authorized
        | LifecycleState::UninstallStaged
        | LifecycleState::Staged
        | LifecycleState::AwaitingUserRestart
        | LifecycleState::Activated
        | LifecycleState::Verified,
        LifecycleEvent::OperationFailed,
      ) => LifecycleState::RecoveryRequired,
      (LifecycleState::RecoveryRequired, LifecycleEvent::RollbackVerified) => {
        LifecycleState::RolledBack
      }
      (
        LifecycleState::Verified | LifecycleState::UninstallStaged,
        LifecycleEvent::UninstallVerified,
      ) => LifecycleState::Uninstalled,
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
  #[serde(default)]
  pub runtime_version: Option<String>,
  #[serde(default)]
  pub driver_version: Option<String>,
  pub public_endpoint_name: Option<String>,
  pub public_endpoint_identity: Option<String>,
  pub producer_interface_present: bool,
  pub services: BTreeSet<String>,
  pub driver_packages: BTreeSet<String>,
  #[serde(default)]
  pub trust: LifecycleTrustInventory,
  pub default_input_roles: BTreeMap<String, String>,
  pub active_session: bool,
  #[serde(default)]
  pub active_session_id: Option<String>,
  pub unrelated_audio_endpoints: BTreeSet<String>,
}

/// Read-only trust and code-integrity state captured around a lifecycle operation.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleTrustInventory {
  pub production_signature_verified: bool,
  pub signer_thumbprint: Option<String>,
  pub test_signing_enabled: bool,
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
      "runtime_version",
      &expected.runtime_version,
      &self.runtime_version,
    );
    compare_field(
      &mut differences,
      "driver_version",
      &expected.driver_version,
      &self.driver_version,
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
    compare_field(&mut differences, "trust", &expected.trust, &self.trust);
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
      "active_session_id",
      &expected.active_session_id,
      &self.active_session_id,
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
  Evidence(PackageEvidenceError),
  PackageRootMissing(PathBuf),
  RequiredFileMissing(PathBuf),
  UnsafeRelativePath(String),
  PrivateSigningMaterial(PathBuf),
  ForbiddenPackageContent(PathBuf),
}

impl Display for ReleaseError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    match self {
      Self::Io { path, message } => write!(formatter, "I/O error at {}: {message}", path.display()),
      Self::Manifest(error) => write!(formatter, "manifest validation failed: {error}"),
      Self::Evidence(error) => write!(formatter, "package evidence validation failed: {error}"),
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
      Self::ForbiddenPackageContent(path) => write!(
        formatter,
        "private audio or artifacts content is present in the production package: {}",
        path.display()
      ),
    }
  }
}

impl Error for ReleaseError {}

impl From<PackageEvidenceError> for ReleaseError {
  fn from(error: PackageEvidenceError) -> Self {
    Self::Evidence(error)
  }
}

/// Evidence mismatch detected by the non-mutating package preflight.
#[derive(Debug, Eq, PartialEq)]
pub enum PackageEvidenceError {
  InvalidJson(String),
  InvalidValue {
    field: String,
    message: String,
  },
  FileHashMismatch {
    path: String,
    expected: String,
    actual: String,
  },
  FileSizeMismatch {
    path: String,
    expected: u64,
    actual: u64,
  },
  MissingFileEvidence(String),
  UnexpectedFileEvidence(String),
  CatalogMemberMismatch {
    path: String,
    expected: String,
    actual: String,
  },
  DigestMismatch {
    field: String,
    expected: String,
    actual: String,
  },
  ForbiddenCommand(String),
}

impl Display for PackageEvidenceError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    match self {
      Self::InvalidJson(message) => write!(formatter, "invalid package evidence JSON: {message}"),
      Self::InvalidValue { field, message } => write!(formatter, "{field}: {message}"),
      Self::FileHashMismatch {
        path,
        expected,
        actual,
      } => write!(
        formatter,
        "{path} SHA-256 mismatch: expected {expected}, got {actual}"
      ),
      Self::FileSizeMismatch {
        path,
        expected,
        actual,
      } => write!(
        formatter,
        "{path} size mismatch: expected {expected}, got {actual}"
      ),
      Self::MissingFileEvidence(path) => {
        write!(
          formatter,
          "package evidence is missing file record for {path}"
        )
      }
      Self::UnexpectedFileEvidence(path) => {
        write!(
          formatter,
          "package evidence contains unexpected file record {path}"
        )
      }
      Self::CatalogMemberMismatch {
        path,
        expected,
        actual,
      } => write!(
        formatter,
        "catalog member {path} mismatch: expected {expected}, got {actual}"
      ),
      Self::DigestMismatch {
        field,
        expected,
        actual,
      } => write!(
        formatter,
        "{field} mismatch: expected {expected}, got {actual}"
      ),
      Self::ForbiddenCommand(command) => write!(
        formatter,
        "package evidence contains a forbidden machine-changing command: {command}"
      ),
    }
  }
}

impl Error for PackageEvidenceError {}

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

fn find_forbidden_package_content(root: &Path) -> Result<Option<PathBuf>, ReleaseError> {
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
      if path.file_name().is_some_and(|name| name == "artifacts") {
        return Ok(Some(path));
      }
      if let Some(found) = find_forbidden_package_content(&path)? {
        return Ok(Some(found));
      }
    } else if file_type.is_file()
      && path.extension().is_some_and(|extension| {
        let extension = extension.to_string_lossy().to_ascii_lowercase();
        PRIVATE_AUDIO_SUFFIXES.contains(&extension.as_str())
      })
    {
      return Ok(Some(path));
    }
  }
  Ok(None)
}

#[allow(
  clippy::too_many_lines,
  reason = "the evidence validator deliberately fails closed across every release proof field"
)]
fn validate_package_evidence(
  root: &Path,
  manifest: &ReleaseManifest,
  evidence: &PackageEvidence,
) -> Result<(), ReleaseError> {
  if evidence.schema_version != PACKAGE_EVIDENCE_SCHEMA_VERSION {
    return Err(evidence_invalid(
      "schema_version",
      format!(
        "expected {}, got {}",
        PACKAGE_EVIDENCE_SCHEMA_VERSION, evidence.schema_version
      ),
    ));
  }
  require_evidence_exact(
    "package_identifier",
    &evidence.package_identifier,
    PRODUCT_IDENTIFIER,
  )?;
  require_evidence_exact(
    "package_version",
    &evidence.package_version,
    &manifest.release_version,
  )?;

  require_evidence_exact(
    "source.repository",
    &evidence.source.repository,
    SYSVAD_REPOSITORY,
  )?;
  require_evidence_exact("source.path", &evidence.source.path, SYSVAD_PATH)?;
  require_evidence_exact("source.commit", &evidence.source.commit, SYSVAD_COMMIT)?;
  require_evidence_exact("source.license", &evidence.source.license, SYSVAD_LICENSE)?;
  require_evidence_exact(
    "source.record_path",
    &evidence.source.record_path,
    SYSVAD_RECORD_PATH,
  )?;
  require_sha256(
    "source.source_tree_sha256",
    &evidence.source.source_tree_sha256,
  )?;
  require_sha256(
    "source.imported_file_set_sha256",
    &evidence.source.imported_file_set_sha256,
  )?;
  let mut patch_paths = BTreeSet::new();
  for patch in &evidence.source.local_patches {
    if !patch_paths.insert(patch.path.clone()) {
      return Err(evidence_invalid(
        "source.local_patches",
        format!("duplicate local patch path {}", patch.path),
      ));
    }
    require_sha256(
      &format!("source.local_patches[{}].sha256", patch.path),
      &patch.sha256,
    )?;
  }
  let expected_patch_paths = LOCAL_PATCH_PATHS
    .iter()
    .map(|path| (*path).to_owned())
    .collect::<BTreeSet<_>>();
  if patch_paths != expected_patch_paths {
    return Err(evidence_invalid(
      "source.local_patches",
      format!("expected {expected_patch_paths:?}, got {patch_paths:?}"),
    ));
  }

  for (field, value) in [
    ("build.windows", evidence.build.windows.as_str()),
    ("build.visual_studio", evidence.build.visual_studio.as_str()),
    (
      "build.msbuild_version",
      evidence.build.msbuild_version.as_str(),
    ),
    ("build.msvc_version", evidence.build.msvc_version.as_str()),
    ("build.sdk_version", evidence.build.sdk_version.as_str()),
    ("build.wdk_version", evidence.build.wdk_version.as_str()),
    (
      "build.inf2cat_version",
      evidence.build.inf2cat_version.as_str(),
    ),
    (
      "build.signtool_version",
      evidence.build.signtool_version.as_str(),
    ),
    (
      "build.canonicalization",
      evidence.build.canonicalization.as_str(),
    ),
  ] {
    require_evidence_non_empty(field, value)?;
  }
  require_evidence_exact(
    "build.configuration",
    &evidence.build.configuration,
    "Release",
  )?;
  require_evidence_exact("build.platform", &evidence.build.platform, "x64")?;
  if evidence.build.commands.is_empty() {
    return Err(evidence_invalid(
      "build.commands",
      "at least one reproducible build command is required",
    ));
  }
  validate_commands(&evidence.build.commands)?;
  if evidence.verification.commands.is_empty() {
    return Err(evidence_invalid(
      "verification.commands",
      "at least one read-only verification command is required",
    ));
  }
  validate_commands(&evidence.verification.commands)?;

  let mut expected_paths = BTreeSet::from([
    MANIFEST_FILE_NAME.to_owned(),
    manifest.files.runtime_executable.clone(),
    manifest.files.driver_inf.clone(),
    manifest.files.driver_binary.clone(),
    manifest.files.driver_catalog.clone(),
    PRODUCTION_DRIVER_NOTICE_PATH.to_owned(),
  ]);
  let mut file_records = BTreeMap::new();
  for file in &evidence.files {
    if !expected_paths.remove(&file.path) {
      return Err(PackageEvidenceError::UnexpectedFileEvidence(file.path.clone()).into());
    }
    if file_records.insert(file.path.clone(), file).is_some() {
      return Err(evidence_invalid(
        "files",
        format!("duplicate file record {}", file.path),
      ));
    }
    require_sha256(&format!("files[{}].sha256", file.path), &file.sha256)?;
    require_evidence_non_empty(format!("files[{}].kind", file.path), &file.kind)?;
  }
  if let Some(path) = expected_paths.into_iter().next() {
    return Err(PackageEvidenceError::MissingFileEvidence(path).into());
  }

  let expected_kinds = [
    (MANIFEST_FILE_NAME, "manifest"),
    (manifest.files.runtime_executable.as_str(), "runtime"),
    (manifest.files.driver_inf.as_str(), "driver-inf"),
    (manifest.files.driver_binary.as_str(), "driver-sys"),
    (manifest.files.driver_catalog.as_str(), "driver-cat"),
    (PRODUCTION_DRIVER_NOTICE_PATH, "license-notice"),
  ];
  for (path, kind) in expected_kinds {
    if file_records[path].kind != kind {
      return Err(evidence_invalid(
        format!("files[{path}].kind"),
        format!("expected {kind}, got {}", file_records[path].kind),
      ));
    }
    let full_path = root.join(safe_relative_path(path)?);
    let (actual_hash, actual_size) = file_digest(&full_path)?;
    let record = file_records[path];
    if record.sha256 != actual_hash {
      return Err(
        PackageEvidenceError::FileHashMismatch {
          path: path.to_owned(),
          expected: record.sha256.clone(),
          actual: actual_hash,
        }
        .into(),
      );
    }
    if record.size_bytes != actual_size {
      return Err(
        PackageEvidenceError::FileSizeMismatch {
          path: path.to_owned(),
          expected: record.size_bytes,
          actual: actual_size,
        }
        .into(),
      );
    }
  }

  require_evidence_exact(
    "catalog.path",
    &evidence.catalog.path,
    &manifest.files.driver_catalog,
  )?;
  if !evidence.catalog.coverage_verified {
    return Err(evidence_invalid(
      "catalog.coverage_verified",
      "catalog coverage must be verified",
    ));
  }
  let expected_members = BTreeSet::from([
    manifest.files.driver_inf.clone(),
    manifest.files.driver_binary.clone(),
  ]);
  let actual_members = evidence
    .catalog
    .members
    .keys()
    .cloned()
    .collect::<BTreeSet<_>>();
  if actual_members != expected_members {
    return Err(evidence_invalid(
      "catalog.members",
      format!("expected {expected_members:?}, got {actual_members:?}"),
    ));
  }
  for member in expected_members {
    let file_hash = &file_records[&member].sha256;
    let catalog_hash = evidence.catalog.members.get(&member).ok_or_else(|| {
      PackageEvidenceError::MissingFileEvidence(format!("catalog member {member}"))
    })?;
    if catalog_hash != file_hash {
      return Err(
        PackageEvidenceError::CatalogMemberMismatch {
          path: member,
          expected: file_hash.clone(),
          actual: catalog_hash.clone(),
        }
        .into(),
      );
    }
    require_sha256(&format!("catalog.members[{member}]"), catalog_hash)?;
  }

  let payload_records = [
    file_records[manifest.files.driver_inf.as_str()],
    file_records[manifest.files.driver_binary.as_str()],
  ];
  let actual_payload_digest = canonical_payload_digest(&payload_records);
  if evidence.reproducibility.canonical_payload_sha256 != actual_payload_digest {
    return Err(
      PackageEvidenceError::DigestMismatch {
        field: "reproducibility.canonical_payload_sha256".to_owned(),
        expected: evidence.reproducibility.canonical_payload_sha256.clone(),
        actual: actual_payload_digest,
      }
      .into(),
    );
  }
  let actual_catalog_digest = catalog_member_digest(&evidence.catalog.members);
  if evidence.reproducibility.catalog_member_set_sha256 != actual_catalog_digest {
    return Err(
      PackageEvidenceError::DigestMismatch {
        field: "reproducibility.catalog_member_set_sha256".to_owned(),
        expected: evidence.reproducibility.catalog_member_set_sha256.clone(),
        actual: actual_catalog_digest,
      }
      .into(),
    );
  }
  if !evidence.reproducibility.replay_verified {
    return Err(evidence_invalid(
      "reproducibility.replay_verified",
      "clean replay must be verified",
    ));
  }
  if !evidence.reproducibility.signature_bytes_excluded {
    return Err(evidence_invalid(
      "reproducibility.signature_bytes_excluded",
      "detached signature bytes must be excluded from canonical payload comparison",
    ));
  }

  for (field, value) in [
    ("signing.route", evidence.signing.route.as_str()),
    (
      "signing.signer_subject",
      evidence.signing.signer_subject.as_str(),
    ),
    (
      "signing.signer_thumbprint",
      evidence.signing.signer_thumbprint.as_str(),
    ),
    (
      "signing.signature_timestamp",
      evidence.signing.signature_timestamp.as_str(),
    ),
  ] {
    require_evidence_non_empty(field, value)?;
  }
  require_evidence_exact(
    "signing.route",
    &evidence.signing.route,
    "external-production-signing",
  )?;
  if evidence.signing.public_chain.is_empty() {
    return Err(evidence_invalid(
      "signing.public_chain",
      "public signer chain metadata is required",
    ));
  }
  for (index, certificate) in evidence.signing.public_chain.iter().enumerate() {
    for (field, value) in [
      (
        format!("signing.public_chain[{index}].subject"),
        certificate.subject.as_str(),
      ),
      (
        format!("signing.public_chain[{index}].issuer"),
        certificate.issuer.as_str(),
      ),
      (
        format!("signing.public_chain[{index}].thumbprint"),
        certificate.thumbprint.as_str(),
      ),
    ] {
      require_evidence_non_empty(&field, value)?;
    }
    require_sha256(
      &format!("signing.public_chain[{index}].sha256"),
      &certificate.sha256,
    )?;
  }
  for (field, value) in [
    (
      "signing.cat_signature_verified",
      evidence.signing.cat_signature_verified,
    ),
    (
      "signing.inf_catalog_coverage_verified",
      evidence.signing.inf_catalog_coverage_verified,
    ),
    (
      "signing.sys_catalog_coverage_verified",
      evidence.signing.sys_catalog_coverage_verified,
    ),
    (
      "signing.test_signing_required",
      evidence.signing.test_signing_required,
    ),
  ] {
    if field == "signing.test_signing_required" {
      if value {
        return Err(evidence_invalid(
          field,
          "production package cannot require TESTSIGNING",
        ));
      }
    } else if !value {
      return Err(evidence_invalid(field, "production trust check must pass"));
    }
  }
  if evidence.signing.sys_embedded_signature_required
    && !evidence.signing.sys_embedded_signature_verified
  {
    return Err(evidence_invalid(
      "signing.sys_embedded_signature_verified",
      "required embedded SYS signature must pass",
    ));
  }
  require_evidence_exact(
    "signing.signer_subject",
    &evidence.signing.signer_subject,
    &manifest.trust.signer_subject,
  )?;
  require_evidence_exact(
    "signing.signer_thumbprint",
    &evidence.signing.signer_thumbprint,
    &manifest.trust.signer_thumbprint,
  )?;
  if !evidence.verification.verified
    || !evidence.verification.replay_verified
    || !evidence.verification.read_only
  {
    return Err(evidence_invalid(
      "verification",
      "package verification must pass, include replay evidence, and remain read-only",
    ));
  }
  require_evidence_non_empty("verification.tool", &evidence.verification.tool)?;
  if evidence.privacy.private_signing_material_present
    || evidence.privacy.audio_content_present
    || evidence.privacy.artifacts_content_present
    || evidence.privacy.machine_secret_material_present
  {
    return Err(evidence_invalid(
      "privacy",
      "private signing, audio, artifacts, and machine-secret content must be absent",
    ));
  }
  Ok(())
}

fn evidence_invalid(field: impl Into<String>, message: impl Into<String>) -> ReleaseError {
  ReleaseError::Evidence(PackageEvidenceError::InvalidValue {
    field: field.into(),
    message: message.into(),
  })
}

fn require_evidence_exact(
  field: impl Into<String>,
  actual: &str,
  expected: &str,
) -> Result<(), ReleaseError> {
  if actual == expected {
    Ok(())
  } else {
    Err(evidence_invalid(
      field,
      format!("expected {expected}, got {actual}"),
    ))
  }
}

fn require_evidence_non_empty(field: impl Into<String>, value: &str) -> Result<(), ReleaseError> {
  if value.trim().is_empty() {
    Err(evidence_invalid(field, "value must not be empty"))
  } else {
    Ok(())
  }
}

fn require_sha256(field: &str, value: &str) -> Result<(), ReleaseError> {
  if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
    Ok(())
  } else {
    Err(evidence_invalid(
      field,
      "value must be a 64-character SHA-256 digest",
    ))
  }
}

fn validate_commands(commands: &[String]) -> Result<(), ReleaseError> {
  for command in commands {
    let normalized = command.to_ascii_lowercase();
    if [
      "pnputil",
      "devcon",
      "bcdedit",
      "certutil",
      "restart-computer",
      "stop-computer",
      "shutdown.exe",
      "logoff.exe",
      "-verb runas",
    ]
    .iter()
    .any(|forbidden| normalized.contains(forbidden))
    {
      return Err(PackageEvidenceError::ForbiddenCommand(command.clone()).into());
    }
  }
  Ok(())
}

fn file_digest(path: &Path) -> Result<(String, u64), ReleaseError> {
  let bytes = fs::read(path).map_err(|error| ReleaseError::Io {
    path: path.to_path_buf(),
    message: error.to_string(),
  })?;
  let size = bytes.len() as u64;
  Ok((sha256_bytes(&bytes), size))
}

fn sha256_bytes(bytes: &[u8]) -> String {
  let digest = Sha256::digest(bytes);
  let mut hexadecimal = String::with_capacity(digest.len() * 2);
  for byte in digest {
    write!(&mut hexadecimal, "{byte:02x}").expect("writing to a String cannot fail");
  }
  hexadecimal
}

fn canonical_payload_digest(records: &[&PackageFileEvidence]) -> String {
  let mut records = records.to_vec();
  records.sort_by(|left, right| left.path.cmp(&right.path));
  let mut canonical = String::new();
  for record in records {
    writeln!(
      &mut canonical,
      "{}\0{}\0{}",
      record.path,
      record.sha256.to_ascii_lowercase(),
      record.size_bytes
    )
    .expect("writing to a String cannot fail");
  }
  sha256_bytes(canonical.as_bytes())
}

fn catalog_member_digest(members: &BTreeMap<String, String>) -> String {
  let mut canonical = String::new();
  for (path, hash) in members {
    writeln!(&mut canonical, "{}\0{}", path, hash.to_ascii_lowercase())
      .expect("writing to a String cannot fail");
  }
  sha256_bytes(canonical.as_bytes())
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

  #[allow(
    clippy::too_many_lines,
    reason = "the synthetic evidence fixture mirrors the complete public schema for preflight tests"
  )]
  fn valid_evidence(manifest: &ReleaseManifest, root: &Path) -> PackageEvidence {
    let file_specs = [
      (MANIFEST_FILE_NAME, "manifest"),
      (manifest.files.runtime_executable.as_str(), "runtime"),
      (manifest.files.driver_inf.as_str(), "driver-inf"),
      (manifest.files.driver_binary.as_str(), "driver-sys"),
      (manifest.files.driver_catalog.as_str(), "driver-cat"),
      (PRODUCTION_DRIVER_NOTICE_PATH, "license-notice"),
    ];
    let files = file_specs
      .into_iter()
      .map(|(path, kind)| {
        let (sha256, size_bytes) = file_digest(&root.join(path)).expect("file digest");
        PackageFileEvidence {
          path: path.to_owned(),
          kind: kind.to_owned(),
          sha256,
          size_bytes,
        }
      })
      .collect::<Vec<_>>();
    let file_records = files
      .iter()
      .map(|file| (file.path.clone(), file))
      .collect::<BTreeMap<_, _>>();
    let catalog = BTreeMap::from([
      (
        manifest.files.driver_inf.clone(),
        file_records[&manifest.files.driver_inf].sha256.clone(),
      ),
      (
        manifest.files.driver_binary.clone(),
        file_records[&manifest.files.driver_binary].sha256.clone(),
      ),
    ]);
    let payload_records = [
      file_records[&manifest.files.driver_inf],
      file_records[&manifest.files.driver_binary],
    ];
    let canonical_payload_sha256 = canonical_payload_digest(&payload_records);
    let catalog_member_set_sha256 = catalog_member_digest(&catalog);

    PackageEvidence {
      schema_version: PACKAGE_EVIDENCE_SCHEMA_VERSION,
      package_identifier: PRODUCT_IDENTIFIER.to_owned(),
      package_version: manifest.release_version.clone(),
      source: SourceProvenance {
        repository: SYSVAD_REPOSITORY.to_owned(),
        path: SYSVAD_PATH.to_owned(),
        commit: SYSVAD_COMMIT.to_owned(),
        license: SYSVAD_LICENSE.to_owned(),
        record_path: SYSVAD_RECORD_PATH.to_owned(),
        source_tree_sha256: "11".repeat(32),
        imported_file_set_sha256: "22".repeat(32),
        local_patches: LOCAL_PATCH_PATHS
          .iter()
          .map(|path| LocalPatchEvidence {
            path: (*path).to_owned(),
            sha256: "33".repeat(32),
          })
          .collect(),
      },
      build: BuildProvenance {
        windows: "Windows 11 test build".to_owned(),
        visual_studio: "Visual Studio test".to_owned(),
        msbuild_version: "1.0.0".to_owned(),
        msvc_version: "1.0.0".to_owned(),
        sdk_version: "10.0.1".to_owned(),
        wdk_version: "10.0.1".to_owned(),
        inf2cat_version: "1.0.0".to_owned(),
        signtool_version: "1.0.0".to_owned(),
        configuration: "Release".to_owned(),
        platform: "x64".to_owned(),
        commands: vec!["build-production.ps1 --reproducible".to_owned()],
        canonicalization: "MiniAEC package canonical v1".to_owned(),
      },
      files,
      catalog: CatalogEvidence {
        path: manifest.files.driver_catalog.clone(),
        members: catalog.clone(),
        coverage_verified: true,
      },
      reproducibility: ReproducibilityEvidence {
        canonical_payload_sha256,
        catalog_member_set_sha256,
        replay_verified: true,
        signature_bytes_excluded: true,
      },
      signing: SigningEvidence {
        route: "external-production-signing".to_owned(),
        signer_subject: manifest.trust.signer_subject.clone(),
        signer_thumbprint: manifest.trust.signer_thumbprint.clone(),
        public_chain: vec![CertificateEvidence {
          subject: manifest.trust.signer_subject.clone(),
          issuer: "CN=Test Public Root".to_owned(),
          thumbprint: manifest.trust.signer_thumbprint.clone(),
          sha256: "44".repeat(32),
        }],
        cat_signature_verified: true,
        inf_catalog_coverage_verified: true,
        sys_catalog_coverage_verified: true,
        sys_embedded_signature_required: false,
        sys_embedded_signature_verified: false,
        test_signing_required: false,
        signature_timestamp: "2026-08-21T00:00:00Z".to_owned(),
      },
      verification: VerificationEvidence {
        verified: true,
        replay_verified: true,
        read_only: true,
        tool: "synthetic package verifier".to_owned(),
        commands: vec![
          "signtool verify /kp /c MiniAECProduction.cat MiniAECProduction.sys".to_owned(),
        ],
      },
      privacy: PrivacyEvidence {
        private_signing_material_present: false,
        audio_content_present: false,
        artifacts_content_present: false,
        machine_secret_material_present: false,
      },
    }
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
      if path != manifest.files.signing_evidence {
        fs::write(root.join(path), b"synthetic production artifact").expect("write package file");
      }
    }
    fs::write(
      root.join(PRODUCTION_DRIVER_NOTICE_PATH),
      b"synthetic MS-PL notice",
    )
    .expect("write license notice");
    let evidence = valid_evidence(manifest, &root);
    fs::write(
      root.join(&manifest.files.signing_evidence),
      serde_json::to_vec_pretty(&evidence).expect("serialize evidence"),
    )
    .expect("write evidence");
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
  fn preflight_rejects_tampered_driver_payload() {
    let manifest = valid_manifest();
    let root = write_package(&manifest);
    fs::write(
      root.join(&manifest.files.driver_binary),
      b"tampered payload",
    )
    .expect("tamper driver payload");
    assert!(matches!(
      preflight_package(&root),
      Err(ReleaseError::Evidence(PackageEvidenceError::FileHashMismatch { path, .. }))
        if path == manifest.files.driver_binary
    ));
    fs::remove_dir_all(root).expect("remove test package");
  }

  #[test]
  fn preflight_rejects_catalog_member_mismatch() {
    let manifest = valid_manifest();
    let root = write_package(&manifest);
    let evidence_path = root.join(&manifest.files.signing_evidence);
    let mut evidence: PackageEvidence =
      serde_json::from_str(&fs::read_to_string(&evidence_path).expect("read evidence"))
        .expect("parse evidence");
    evidence
      .catalog
      .members
      .insert(manifest.files.driver_inf.clone(), "00".repeat(32));
    fs::write(
      evidence_path,
      serde_json::to_vec_pretty(&evidence).expect("serialize evidence"),
    )
    .expect("write evidence");
    assert!(matches!(
      preflight_package(&root),
      Err(ReleaseError::Evidence(PackageEvidenceError::CatalogMemberMismatch { path, .. }))
        if path == manifest.files.driver_inf
    ));
    fs::remove_dir_all(root).expect("remove test package");
  }

  #[test]
  fn preflight_rejects_invalid_catalog_trust() {
    let manifest = valid_manifest();
    let root = write_package(&manifest);
    let evidence_path = root.join(&manifest.files.signing_evidence);
    let mut evidence: PackageEvidence =
      serde_json::from_str(&fs::read_to_string(&evidence_path).expect("read evidence"))
        .expect("parse evidence");
    evidence.signing.cat_signature_verified = false;
    fs::write(
      evidence_path,
      serde_json::to_vec_pretty(&evidence).expect("serialize evidence"),
    )
    .expect("write evidence");
    assert!(matches!(
      preflight_package(&root),
      Err(ReleaseError::Evidence(PackageEvidenceError::InvalidValue { field, .. }))
        if field == "signing.cat_signature_verified"
    ));
    fs::remove_dir_all(root).expect("remove test package");
  }

  #[test]
  fn preflight_rejects_source_and_build_input_drift() {
    let manifest = valid_manifest();
    let root = write_package(&manifest);
    let evidence_path = root.join(&manifest.files.signing_evidence);
    let mut evidence: PackageEvidence =
      serde_json::from_str(&fs::read_to_string(&evidence_path).expect("read evidence"))
        .expect("parse evidence");
    evidence.source.commit = "changed-source".to_owned();
    fs::write(
      &evidence_path,
      serde_json::to_vec_pretty(&evidence).expect("serialize source drift"),
    )
    .expect("write source drift");
    assert!(matches!(
      preflight_package(&root),
      Err(ReleaseError::Evidence(PackageEvidenceError::InvalidValue { field, .. }))
        if field == "source.commit"
    ));

    evidence.source.commit = SYSVAD_COMMIT.to_owned();
    evidence.build.configuration = "Debug".to_owned();
    fs::write(
      &evidence_path,
      serde_json::to_vec_pretty(&evidence).expect("serialize build drift"),
    )
    .expect("write build drift");
    assert!(matches!(
      preflight_package(&root),
      Err(ReleaseError::Evidence(PackageEvidenceError::InvalidValue { field, .. }))
        if field == "build.configuration"
    ));
    fs::remove_dir_all(root).expect("remove test package");
  }

  #[test]
  fn preflight_accepts_variable_signature_metadata() {
    let manifest = valid_manifest();
    let root = write_package(&manifest);
    let evidence_path = root.join(&manifest.files.signing_evidence);
    let mut evidence: PackageEvidence =
      serde_json::from_str(&fs::read_to_string(&evidence_path).expect("read evidence"))
        .expect("parse evidence");
    evidence.signing.signature_timestamp = "timestamp signer: CN=Other Public TSA".to_owned();
    fs::write(
      evidence_path,
      serde_json::to_vec_pretty(&evidence).expect("serialize evidence"),
    )
    .expect("write evidence");
    assert!(preflight_package(&root).is_ok());
    fs::remove_dir_all(root).expect("remove test package");
  }

  #[test]
  fn preflight_rejects_machine_changing_evidence_command() {
    let manifest = valid_manifest();
    let root = write_package(&manifest);
    let evidence_path = root.join(&manifest.files.signing_evidence);
    let mut evidence: PackageEvidence =
      serde_json::from_str(&fs::read_to_string(&evidence_path).expect("read evidence"))
        .expect("parse evidence");
    evidence.verification.commands = vec!["pnputil /add-driver package.inf".to_owned()];
    fs::write(
      evidence_path,
      serde_json::to_vec_pretty(&evidence).expect("serialize evidence"),
    )
    .expect("write evidence");
    assert!(matches!(
      preflight_package(&root),
      Err(ReleaseError::Evidence(PackageEvidenceError::ForbiddenCommand(command)))
        if command.contains("pnputil")
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
      .apply(LifecycleEvent::UserRestartObserved)
      .expect("user observed restart");
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

  fn empty_inventory() -> LifecycleInventory {
    LifecycleInventory {
      schema_version: INVENTORY_SCHEMA_VERSION,
      ..LifecycleInventory::default()
    }
  }

  fn installed_inventory(manifest: &ReleaseManifest) -> LifecycleInventory {
    let mut inventory = empty_inventory();
    inventory.product_release = Some(manifest.release_version.clone());
    inventory.runtime_version = Some(manifest.runtime.version.clone());
    inventory.driver_version = Some(manifest.driver.version.clone());
    inventory.public_endpoint_name = Some(PUBLIC_ENDPOINT_NAME.to_owned());
    inventory.public_endpoint_identity = Some(PRODUCTION_ENDPOINT_IDENTITY.to_owned());
    inventory.producer_interface_present = true;
    inventory
      .services
      .insert(PRODUCTION_SERVICE_NAME.to_owned());
    inventory
      .driver_packages
      .insert(PRODUCTION_DRIVER_IDENTITY.to_owned());
    inventory.trust = LifecycleTrustInventory {
      production_signature_verified: true,
      signer_thumbprint: Some(manifest.trust.signer_thumbprint.clone()),
      test_signing_enabled: false,
    };
    inventory
  }

  fn candidate_package(manifest: &ReleaseManifest) -> (PathBuf, PackagePreflight) {
    let root = write_package(manifest);
    let package = preflight_package(&root).expect("synthetic production package preflight");
    (root, package)
  }

  struct FakeLifecycleBackend {
    elevated: bool,
    inventory: LifecycleInventory,
    rollback_inventory: LifecycleInventory,
    activation_outcome: ActivationOutcome,
    activation_failure: bool,
    omit_endpoint: bool,
    stage_calls: usize,
    activation_calls: usize,
    uninstall_calls: usize,
    rollback_calls: usize,
  }

  impl FakeLifecycleBackend {
    fn new(inventory: LifecycleInventory) -> Self {
      Self {
        rollback_inventory: inventory.clone(),
        inventory,
        elevated: true,
        activation_outcome: ActivationOutcome::Activated,
        activation_failure: false,
        omit_endpoint: false,
        stage_calls: 0,
        activation_calls: 0,
        uninstall_calls: 0,
        rollback_calls: 0,
      }
    }
  }

  impl LifecycleBackend for FakeLifecycleBackend {
    type Error = String;

    fn has_elevated_authority(&self) -> bool {
      self.elevated
    }

    fn capture_inventory(&mut self) -> Result<LifecycleInventory, Self::Error> {
      Ok(self.inventory.clone())
    }

    fn stage_package(&mut self, _package: &PackagePreflight) -> Result<(), Self::Error> {
      self.stage_calls += 1;
      Ok(())
    }

    fn stage_uninstall(&mut self, _installed: &ReleaseManifest) -> Result<(), Self::Error> {
      self.stage_calls += 1;
      Ok(())
    }

    fn activate_package(
      &mut self,
      package: &PackagePreflight,
    ) -> Result<ActivationOutcome, Self::Error> {
      self.activation_calls += 1;
      if self.activation_failure {
        return Err(String::from("synthetic activation failure"));
      }
      let default_input_roles = self.inventory.default_input_roles.clone();
      let unrelated_audio_endpoints = self.inventory.unrelated_audio_endpoints.clone();
      self.inventory = installed_inventory(&package.manifest);
      self.inventory.default_input_roles = default_input_roles;
      self.inventory.unrelated_audio_endpoints = unrelated_audio_endpoints;
      if self.omit_endpoint {
        self.inventory.public_endpoint_name = None;
      }
      Ok(self.activation_outcome)
    }

    fn rollback(
      &mut self,
      _prior_release: Option<&LifecycleReleaseIdentity>,
    ) -> Result<(), Self::Error> {
      self.rollback_calls += 1;
      self.inventory = self.rollback_inventory.clone();
      Ok(())
    }

    fn uninstall(&mut self) -> Result<(), Self::Error> {
      self.uninstall_calls += 1;
      self.inventory.product_release = None;
      self.inventory.runtime_version = None;
      self.inventory.driver_version = None;
      self.inventory.public_endpoint_name = None;
      self.inventory.public_endpoint_identity = None;
      self.inventory.producer_interface_present = false;
      self.inventory.services.remove(PRODUCTION_SERVICE_NAME);
      self
        .inventory
        .driver_packages
        .remove(PRODUCTION_DRIVER_IDENTITY);
      self.inventory.trust = LifecycleTrustInventory::default();
      self.inventory.active_session = false;
      self.inventory.active_session_id = None;
      Ok(())
    }
  }

  #[test]
  fn lifecycle_coordinator_installs_and_verifies_outside_realtime_workers() {
    let manifest = valid_manifest();
    let (root, package) = candidate_package(&manifest);
    let before = empty_inventory();
    let mut backend = FakeLifecycleBackend::new(before.clone());
    let mut coordinator =
      LifecycleCoordinator::prepare_install(&package, before).expect("prepare install");

    coordinator.authorize().expect("authorize");
    coordinator
      .stage(&mut backend, Some(&package))
      .expect("stage package");
    assert_eq!(coordinator.state(), LifecycleState::Staged);
    assert_eq!(
      coordinator
        .activate(&mut backend, &package)
        .expect("activate package"),
      ActivationOutcome::Activated
    );
    coordinator
      .verify_activation(&mut backend)
      .expect("verify activation");
    assert_eq!(coordinator.state(), LifecycleState::Verified);
    assert_eq!(backend.stage_calls, 1);
    assert_eq!(backend.activation_calls, 1);
    assert!(coordinator.journal().last_observed.is_some());
    fs::remove_dir_all(root).expect("remove synthetic package");
  }

  #[test]
  fn lifecycle_requires_elevated_backend_before_staging() {
    let manifest = valid_manifest();
    let (root, package) = candidate_package(&manifest);
    let mut backend = FakeLifecycleBackend::new(empty_inventory());
    backend.elevated = false;
    let mut coordinator =
      LifecycleCoordinator::prepare_install(&package, empty_inventory()).expect("prepare install");
    coordinator.authorize().expect("authorize");
    assert_eq!(
      coordinator.stage(&mut backend, Some(&package)),
      Err(LifecycleError::ElevatedAuthorityRequired)
    );
    assert_eq!(coordinator.state(), LifecycleState::Authorized);
    assert_eq!(backend.stage_calls, 0);
    fs::remove_dir_all(root).expect("remove synthetic package");
  }

  #[test]
  fn lifecycle_rejects_incompatible_upgrade_before_backend_mutation() {
    let installed = valid_manifest();
    let mut candidate = valid_manifest();
    candidate.release_version = "0.2.0".to_owned();
    candidate.runtime.version = "0.2.0".to_owned();
    candidate.driver.version = "0.2.0".to_owned();
    candidate.compatibility.runtime_minimum = "0.2.0".to_owned();
    candidate.compatibility.runtime_maximum = Some("0.3.0".to_owned());
    candidate.compatibility.driver_minimum = "0.2.0".to_owned();
    candidate.compatibility.driver_maximum = Some("0.3.0".to_owned());
    let (root, package) = candidate_package(&candidate);
    let backend = FakeLifecycleBackend::new(installed_inventory(&installed));
    let error =
      LifecycleCoordinator::prepare_upgrade(&package, &installed, installed_inventory(&installed))
        .expect_err("incompatible upgrade must stop before staging");
    assert_eq!(
      error,
      LifecycleError::Compatibility(CompatibilityError::InstalledReleaseOutsideCandidateRange)
    );
    assert_eq!(backend.stage_calls, 0);
    fs::remove_dir_all(root).expect("remove synthetic package");
  }

  #[test]
  fn lifecycle_requires_explicit_user_restart_observation() {
    let manifest = valid_manifest();
    let (root, package) = candidate_package(&manifest);
    let mut backend = FakeLifecycleBackend::new(empty_inventory());
    backend.activation_outcome = ActivationOutcome::AwaitingUserRestart;
    let mut coordinator =
      LifecycleCoordinator::prepare_install(&package, empty_inventory()).expect("prepare install");
    coordinator.authorize().expect("authorize");
    coordinator
      .stage(&mut backend, Some(&package))
      .expect("stage package");
    assert_eq!(
      coordinator
        .activate(&mut backend, &package)
        .expect("activation boundary"),
      ActivationOutcome::AwaitingUserRestart
    );
    assert_eq!(coordinator.state(), LifecycleState::AwaitingUserRestart);
    coordinator
      .record_user_restart(backend.inventory.clone())
      .expect("record manually observed restart");
    assert_eq!(coordinator.state(), LifecycleState::Staged);
    backend.activation_outcome = ActivationOutcome::Activated;
    coordinator
      .activate(&mut backend, &package)
      .expect("continue activation");
    coordinator
      .verify_activation(&mut backend)
      .expect("verify activation");
    assert_eq!(coordinator.state(), LifecycleState::Verified);
    fs::remove_dir_all(root).expect("remove synthetic package");
  }

  #[test]
  fn lifecycle_records_failure_and_requires_verified_rollback() {
    let manifest = valid_manifest();
    let (root, package) = candidate_package(&manifest);
    let before = empty_inventory();
    let mut backend = FakeLifecycleBackend::new(before.clone());
    backend.activation_failure = true;
    let mut coordinator =
      LifecycleCoordinator::prepare_install(&package, before.clone()).expect("prepare install");
    coordinator.authorize().expect("authorize");
    coordinator
      .stage(&mut backend, Some(&package))
      .expect("stage package");
    assert!(matches!(
      coordinator.activate(&mut backend, &package),
      Err(LifecycleError::Backend {
        action: "activate package",
        ..
      })
    ));
    assert_eq!(coordinator.state(), LifecycleState::RecoveryRequired);
    backend.rollback_inventory = before.without_active_session();
    coordinator
      .rollback(&mut backend)
      .expect("verified rollback");
    assert_eq!(coordinator.state(), LifecycleState::RolledBack);
    assert_eq!(backend.rollback_calls, 1);
    fs::remove_dir_all(root).expect("remove synthetic package");
  }

  #[test]
  fn lifecycle_uninstall_requires_clean_baseline_restoration() {
    let manifest = valid_manifest();
    let baseline = LifecycleInventory {
      default_input_roles: BTreeMap::from([(
        String::from("console"),
        String::from("Physical Mic"),
      )]),
      unrelated_audio_endpoints: BTreeSet::from([String::from("Physical Mic")]),
      ..empty_inventory()
    };
    let current = LifecycleInventory {
      default_input_roles: baseline.default_input_roles.clone(),
      unrelated_audio_endpoints: baseline.unrelated_audio_endpoints.clone(),
      ..installed_inventory(&manifest)
    };
    let mut backend = FakeLifecycleBackend::new(current.clone());
    let mut coordinator = LifecycleCoordinator::prepare_uninstall(&manifest, current, &baseline)
      .expect("prepare uninstall");
    coordinator.authorize().expect("authorize");
    coordinator
      .stage(&mut backend, None)
      .expect("stage uninstall");
    assert_eq!(coordinator.state(), LifecycleState::UninstallStaged);
    coordinator
      .uninstall(&mut backend)
      .expect("verified uninstall");
    assert_eq!(coordinator.state(), LifecycleState::Uninstalled);
    assert_eq!(backend.uninstall_calls, 1);
  }

  #[test]
  fn lifecycle_postcondition_failure_enters_recovery() {
    let manifest = valid_manifest();
    let (root, package) = candidate_package(&manifest);
    let mut backend = FakeLifecycleBackend::new(empty_inventory());
    backend.omit_endpoint = true;
    let mut coordinator =
      LifecycleCoordinator::prepare_install(&package, empty_inventory()).expect("prepare install");
    coordinator.authorize().expect("authorize");
    coordinator
      .stage(&mut backend, Some(&package))
      .expect("stage package");
    coordinator
      .activate(&mut backend, &package)
      .expect("activate package");
    assert!(matches!(
      coordinator.verify_activation(&mut backend),
      Err(LifecycleError::PostconditionMismatch {
        operation: LifecycleOperation::Install,
        ..
      })
    ));
    assert_eq!(coordinator.state(), LifecycleState::RecoveryRequired);
    fs::remove_dir_all(root).expect("remove synthetic package");
  }
}
