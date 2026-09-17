#![allow(
  clippy::missing_errors_doc,
  reason = "LifecycleError and the backend boundary document the shared error contract."
)]

use std::error::Error;
use std::fmt::{self, Debug, Display, Formatter};

use serde::{Deserialize, Serialize};

use super::{
  preflight_package, CompatibilityError, InventoryDifference, LifecycleEvent, LifecycleInventory,
  LifecycleState, LifecycleStateMachine, ManifestError, PackagePreflight, ReleaseError,
  ReleaseManifest, INVENTORY_SCHEMA_VERSION, PRODUCTION_DRIVER_IDENTITY,
  PRODUCTION_ENDPOINT_IDENTITY, PRODUCTION_SERVICE_NAME, PUBLIC_ENDPOINT_NAME,
};

/// Schema version for the metadata-only lifecycle journal.
pub const LIFECYCLE_JOURNAL_SCHEMA_VERSION: u16 = 1;

/// The machine-changing lifecycle operation represented by a coordinator journal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleOperation {
  Install,
  Upgrade,
  Uninstall,
}

/// Result returned by an elevated backend when activation reaches a user restart boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationOutcome {
  Activated,
  AwaitingUserRestart,
}

/// A pending user action recorded by the lifecycle boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecyclePendingAction {
  RestartWindows,
}

/// The immutable component identity recorded in a lifecycle journal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleReleaseIdentity {
  pub product_identifier: String,
  pub release_version: String,
  pub runtime_identifier: String,
  pub runtime_version: String,
  pub driver_identifier: String,
  pub driver_version: String,
  pub transport_protocol_version: u16,
  pub signer_thumbprint: String,
}

impl LifecycleReleaseIdentity {
  #[must_use]
  pub fn from_manifest(manifest: &ReleaseManifest) -> Self {
    Self {
      product_identifier: manifest.product.identifier.clone(),
      release_version: manifest.release_version.clone(),
      runtime_identifier: manifest.runtime.identifier.clone(),
      runtime_version: manifest.runtime.version.clone(),
      driver_identifier: manifest.driver.identifier.clone(),
      driver_version: manifest.driver.version.clone(),
      transport_protocol_version: manifest.transport.protocol_version,
      signer_thumbprint: manifest.trust.signer_thumbprint.clone(),
    }
  }
}

/// Persisted metadata for an install, upgrade, or uninstall attempt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleJournal {
  pub schema_version: u16,
  pub operation: LifecycleOperation,
  pub state: LifecycleState,
  pub before: LifecycleInventory,
  pub restoration_target: LifecycleInventory,
  pub staged_release: Option<LifecycleReleaseIdentity>,
  pub prior_release: Option<LifecycleReleaseIdentity>,
  pub last_observed: Option<LifecycleInventory>,
  pub pending_user_action: Option<LifecyclePendingAction>,
  pub failure: Option<String>,
}

/// Backend boundary for low-frequency machine lifecycle work.
///
/// A Windows implementation must be an explicit elevated installer or maintenance boundary.
/// The release crate never invokes these methods from real-time audio workers, the tray UI, or
/// an operating-system restart path. `capture_inventory` is read-only; every other method may
/// change machine state and must be guarded by the backend's authorization boundary.
pub trait LifecycleBackend {
  type Error: Display;

  /// Returns whether this backend is executing inside the authorized elevated boundary.
  fn has_elevated_authority(&self) -> bool;

  /// Captures package, endpoint, service, trust, role, and session metadata without mutation.
  fn capture_inventory(&mut self) -> Result<LifecycleInventory, Self::Error>;

  /// Stages a production package without claiming endpoint activation.
  fn stage_package(&mut self, package: &PackagePreflight) -> Result<(), Self::Error>;

  /// Stages removal of the installed production package.
  fn stage_uninstall(&mut self, installed: &ReleaseManifest) -> Result<(), Self::Error>;

  /// Activates a staged runtime/driver pair and reports a manual restart boundary when needed.
  fn activate_package(
    &mut self,
    package: &PackagePreflight,
  ) -> Result<ActivationOutcome, Self::Error>;

  /// Restores the previous release or removes a partially installed candidate.
  fn rollback(
    &mut self,
    prior_release: Option<&LifecycleReleaseIdentity>,
  ) -> Result<(), Self::Error>;

  /// Removes product-owned machine state after uninstall staging.
  fn uninstall(&mut self) -> Result<(), Self::Error>;
}

/// Coordinates the low-frequency lifecycle contract while leaving Windows operations injectable.
#[derive(Debug)]
pub struct LifecycleCoordinator {
  operation: LifecycleOperation,
  machine: LifecycleStateMachine,
  journal: LifecycleJournal,
  candidate: Option<LifecycleReleaseIdentity>,
  installed_manifest: Option<ReleaseManifest>,
}

impl LifecycleCoordinator {
  /// Prepares a production install from a clean product baseline.
  ///
  /// This method is read-only. It does not request elevation or call a backend.
  pub fn prepare_install(
    package: &PackagePreflight,
    before: LifecycleInventory,
  ) -> Result<Self, LifecycleError> {
    package
      .manifest
      .validate()
      .map_err(LifecycleError::Manifest)?;
    before.validate()?;
    if !before.product_absent() {
      return Err(LifecycleError::ExistingProductState);
    }
    Self::new(LifecycleOperation::Install, before, &package.manifest, None)
  }

  /// Prepares an upgrade and rejects incompatible runtime/driver pairs before staging.
  ///
  /// This method is read-only. It does not request elevation or call a backend.
  pub fn prepare_upgrade(
    package: &PackagePreflight,
    installed: &ReleaseManifest,
    before: LifecycleInventory,
  ) -> Result<Self, LifecycleError> {
    package
      .manifest
      .validate()
      .map_err(LifecycleError::Manifest)?;
    installed.validate().map_err(LifecycleError::Manifest)?;
    before.validate()?;
    package
      .manifest
      .is_compatible_with(installed)
      .map_err(LifecycleError::Compatibility)?;
    before.require_installed_release(installed)?;
    Self::new(
      LifecycleOperation::Upgrade,
      before,
      &package.manifest,
      Some(installed),
    )
  }

  /// Prepares an uninstall against the saved pre-install baseline.
  ///
  /// `restoration_target` must be the metadata-only baseline captured before installation. The
  /// target is compared after uninstall so unrelated endpoints and default-input roles cannot be
  /// silently discarded.
  pub fn prepare_uninstall(
    installed: &ReleaseManifest,
    before: LifecycleInventory,
    restoration_target: &LifecycleInventory,
  ) -> Result<Self, LifecycleError> {
    installed.validate().map_err(LifecycleError::Manifest)?;
    before.validate()?;
    restoration_target.validate()?;
    before.require_installed_release(installed)?;
    restoration_target.require_product_absent()?;
    let mut coordinator = Self::new(
      LifecycleOperation::Uninstall,
      before,
      installed,
      Some(installed),
    )?;
    coordinator.journal.restoration_target = restoration_target.without_active_session();
    Ok(coordinator)
  }

  /// Returns the operation represented by this coordinator.
  #[must_use]
  pub const fn operation(&self) -> LifecycleOperation {
    self.operation
  }

  /// Returns the current pure lifecycle state.
  #[must_use]
  pub const fn state(&self) -> LifecycleState {
    self.machine.state()
  }

  /// Returns the serializable journal for operator review or persistence.
  #[must_use]
  pub const fn journal(&self) -> &LifecycleJournal {
    &self.journal
  }

  /// Records explicit user authorization before any backend mutation is permitted.
  pub fn authorize(&mut self) -> Result<(), LifecycleError> {
    self.transition(LifecycleEvent::UserAuthorized)
  }

  /// Stages installation, upgrade, or uninstall inside the backend's elevated boundary.
  pub fn stage<B: LifecycleBackend>(
    &mut self,
    backend: &mut B,
    package: Option<&PackagePreflight>,
  ) -> Result<(), LifecycleError> {
    Self::require_elevated(backend)?;
    let result = match self.operation {
      LifecycleOperation::Install | LifecycleOperation::Upgrade => {
        let package = package.ok_or(LifecycleError::CandidatePackageMissing)?;
        let package = self.require_candidate(package)?;
        backend
          .stage_package(&package)
          .map_err(|error| LifecycleError::Backend {
            action: "stage package",
            message: error.to_string(),
          })
          .and_then(|()| self.transition(LifecycleEvent::PackageStaged))
      }
      LifecycleOperation::Uninstall => {
        let installed = self
          .installed_manifest
          .as_ref()
          .ok_or(LifecycleError::InstalledManifestMissing)?;
        backend
          .stage_uninstall(installed)
          .map_err(|error| LifecycleError::Backend {
            action: "stage uninstall",
            message: error.to_string(),
          })
          .and_then(|()| self.transition(LifecycleEvent::UninstallRequested))
      }
    };
    if let Err(error) = result {
      self.record_failure(error.to_string());
      return Err(error);
    }
    self.capture_after_stage(backend)
  }

  /// Activates a staged install or upgrade and stops at, rather than crossing, a restart boundary.
  pub fn activate<B: LifecycleBackend>(
    &mut self,
    backend: &mut B,
    package: &PackagePreflight,
  ) -> Result<ActivationOutcome, LifecycleError> {
    Self::require_elevated(backend)?;
    if self.operation == LifecycleOperation::Uninstall {
      return Err(LifecycleError::OperationNotAllowed {
        operation: self.operation,
        state: self.state(),
      });
    }
    let package = self.require_candidate(package)?;
    let outcome = backend
      .activate_package(&package)
      .map_err(|error| LifecycleError::Backend {
        action: "activate package",
        message: error.to_string(),
      });
    let outcome = match outcome {
      Ok(outcome) => outcome,
      Err(error) => {
        self.record_failure(error.to_string());
        return Err(error);
      }
    };
    match outcome {
      ActivationOutcome::AwaitingUserRestart => {
        if let Err(error) = self.transition(LifecycleEvent::RestartRequired) {
          self.record_failure(error.to_string());
          return Err(error);
        }
        self.journal.pending_user_action = Some(LifecyclePendingAction::RestartWindows);
        self.capture_after_stage(backend)?;
      }
      ActivationOutcome::Activated => {
        if let Err(error) = self.transition(LifecycleEvent::ActivationCompleted) {
          self.record_failure(error.to_string());
          return Err(error);
        }
        self.capture_after_stage(backend)?;
      }
    }
    Ok(outcome)
  }

  /// Records a user-performed restart and returns to the staged state for explicit continuation.
  ///
  /// The method accepts an already captured read-only inventory and never invokes restart,
  /// shutdown, sign-out, or any other operating-system control path.
  pub fn record_user_restart(
    &mut self,
    observed: LifecycleInventory,
  ) -> Result<(), LifecycleError> {
    if self.state() != LifecycleState::AwaitingUserRestart {
      return Err(LifecycleError::OperationNotAllowed {
        operation: self.operation,
        state: self.state(),
      });
    }
    observed.validate()?;
    self.reject_mixed_release(&observed)?;
    self.journal.last_observed = Some(observed);
    self.journal.pending_user_action = None;
    self.transition(LifecycleEvent::UserRestartObserved)
  }

  /// Verifies the endpoint, package, trust, role, and session postconditions after activation.
  pub fn verify_activation<B: LifecycleBackend>(
    &mut self,
    backend: &mut B,
  ) -> Result<(), LifecycleError> {
    if self.state() != LifecycleState::Activated {
      return Err(LifecycleError::OperationNotAllowed {
        operation: self.operation,
        state: self.state(),
      });
    }
    let observed = match self.capture_observation(backend) {
      Ok(observed) => observed,
      Err(error) => {
        self.record_failure(error.to_string());
        return Err(error);
      }
    };
    let differences = self.activation_differences(&observed);
    if !differences.is_empty() {
      let error = LifecycleError::PostconditionMismatch {
        operation: self.operation,
        differences,
      };
      self.record_failure(error.to_string());
      return Err(error);
    }
    self.transition(LifecycleEvent::VerificationPassed)
  }

  /// Completes uninstall only when every saved restoration postcondition matches.
  pub fn uninstall<B: LifecycleBackend>(&mut self, backend: &mut B) -> Result<(), LifecycleError> {
    Self::require_elevated(backend)?;
    if self.operation != LifecycleOperation::Uninstall {
      return Err(LifecycleError::OperationNotAllowed {
        operation: self.operation,
        state: self.state(),
      });
    }
    backend
      .uninstall()
      .map_err(|error| LifecycleError::Backend {
        action: "uninstall package",
        message: error.to_string(),
      })
      .inspect_err(|error| {
        self.record_failure(error.to_string());
      })?;
    let observed = match self.capture_observation(backend) {
      Ok(observed) => observed,
      Err(error) => {
        self.record_failure(error.to_string());
        return Err(error);
      }
    };
    let comparison = observed.compare_to(&self.journal.restoration_target);
    if !comparison.is_clean() {
      let error = LifecycleError::PostconditionMismatch {
        operation: self.operation,
        differences: comparison.differences,
      };
      self.record_failure(error.to_string());
      return Err(error);
    }
    self.transition(LifecycleEvent::UninstallVerified)
  }

  /// Recovers an interrupted operation and marks it rolled back only after exact postconditions.
  pub fn rollback<B: LifecycleBackend>(&mut self, backend: &mut B) -> Result<(), LifecycleError> {
    Self::require_elevated(backend)?;
    if self.state() != LifecycleState::RecoveryRequired {
      return Err(LifecycleError::OperationNotAllowed {
        operation: self.operation,
        state: self.state(),
      });
    }
    backend
      .rollback(self.journal.prior_release.as_ref())
      .map_err(|error| LifecycleError::Backend {
        action: "rollback package",
        message: error.to_string(),
      })
      .inspect_err(|error| {
        self.record_failure(error.to_string());
      })?;
    let observed = match self.capture_observation(backend) {
      Ok(observed) => observed,
      Err(error) => {
        self.record_failure(error.to_string());
        return Err(error);
      }
    };
    let comparison = observed.compare_to(&self.journal.restoration_target);
    if !comparison.is_clean() {
      let error = LifecycleError::PostconditionMismatch {
        operation: self.operation,
        differences: comparison.differences,
      };
      self.record_failure(error.to_string());
      return Err(error);
    }
    self.transition(LifecycleEvent::RollbackVerified)
  }

  fn new(
    operation: LifecycleOperation,
    before: LifecycleInventory,
    staged_manifest: &ReleaseManifest,
    installed_manifest: Option<&ReleaseManifest>,
  ) -> Result<Self, LifecycleError> {
    let candidate = match operation {
      LifecycleOperation::Install | LifecycleOperation::Upgrade => {
        Some(LifecycleReleaseIdentity::from_manifest(staged_manifest))
      }
      LifecycleOperation::Uninstall => None,
    };
    let prior_release = installed_manifest
      .as_ref()
      .map(|manifest| LifecycleReleaseIdentity::from_manifest(manifest));
    let restoration_target = before.without_active_session();
    let machine = LifecycleStateMachine::new()
      .apply(LifecycleEvent::PreflightPassed)
      .map_err(LifecycleError::Transition)?;
    Ok(Self {
      operation,
      machine,
      journal: LifecycleJournal {
        schema_version: LIFECYCLE_JOURNAL_SCHEMA_VERSION,
        operation,
        state: LifecycleState::Preflighted,
        before,
        restoration_target,
        staged_release: candidate.clone(),
        prior_release,
        last_observed: None,
        pending_user_action: None,
        failure: None,
      },
      candidate,
      installed_manifest: installed_manifest.cloned(),
    })
  }

  fn transition(&mut self, event: LifecycleEvent) -> Result<(), LifecycleError> {
    self.machine = self
      .machine
      .apply(event)
      .map_err(LifecycleError::Transition)?;
    self.journal.state = self.machine.state();
    Ok(())
  }

  fn require_elevated<B: LifecycleBackend>(backend: &B) -> Result<(), LifecycleError> {
    if backend.has_elevated_authority() {
      Ok(())
    } else {
      Err(LifecycleError::ElevatedAuthorityRequired)
    }
  }

  fn require_candidate(
    &self,
    package: &PackagePreflight,
  ) -> Result<PackagePreflight, LifecycleError> {
    let expected = self
      .candidate
      .as_ref()
      .ok_or(LifecycleError::CandidatePackageMissing)?;
    let refreshed = preflight_package(&package.package_root).map_err(LifecycleError::Package)?;
    let actual = LifecycleReleaseIdentity::from_manifest(&refreshed.manifest);
    if &actual != expected {
      return Err(LifecycleError::CandidatePackageMismatch);
    }
    Ok(refreshed)
  }

  fn capture_after_stage<B: LifecycleBackend>(
    &mut self,
    backend: &mut B,
  ) -> Result<(), LifecycleError> {
    match self.capture_observation(backend) {
      Ok(_) => Ok(()),
      Err(error) => {
        self.record_failure(error.to_string());
        Err(error)
      }
    }
  }

  fn capture_observation<B: LifecycleBackend>(
    &mut self,
    backend: &mut B,
  ) -> Result<LifecycleInventory, LifecycleError> {
    let observed = backend
      .capture_inventory()
      .map_err(|error| LifecycleError::Backend {
        action: "capture lifecycle inventory",
        message: error.to_string(),
      })?;
    observed.validate()?;
    self.journal.last_observed = Some(observed.clone());
    Ok(observed)
  }

  fn record_failure(&mut self, message: String) {
    self.journal.failure = Some(message);
    if let Ok(machine) = self.machine.apply(LifecycleEvent::OperationFailed) {
      self.machine = machine;
      self.journal.state = machine.state();
    }
  }

  fn reject_mixed_release(&self, observed: &LifecycleInventory) -> Result<(), LifecycleError> {
    if let Some(release) = observed.product_release.as_deref() {
      let known = self
        .candidate
        .as_ref()
        .into_iter()
        .chain(self.journal.prior_release.as_ref())
        .find(|identity| identity.release_version == release);
      if let Some(identity) = known {
        if observed.runtime_version.as_deref() != Some(identity.runtime_version.as_str())
          || observed.driver_version.as_deref() != Some(identity.driver_version.as_str())
        {
          return Err(LifecycleError::MixedRelease(release.to_owned()));
        }
      } else {
        return Err(LifecycleError::MixedRelease(release.to_owned()));
      }
    } else if observed.runtime_version.is_some() || observed.driver_version.is_some() {
      return Err(LifecycleError::MixedRelease(
        "component identity without product release".to_owned(),
      ));
    }
    Ok(())
  }

  fn activation_differences(&self, observed: &LifecycleInventory) -> Vec<InventoryDifference> {
    let Some(candidate) = self.candidate.as_ref() else {
      return vec![InventoryDifference {
        field: "staged_release".to_owned(),
        expected: "candidate release".to_owned(),
        observed: "missing".to_owned(),
      }];
    };
    let mut differences = Vec::new();
    compare_expected(
      &mut differences,
      "product_release",
      &Some(candidate.release_version.clone()),
      &observed.product_release,
    );
    compare_expected(
      &mut differences,
      "runtime_version",
      &Some(candidate.runtime_version.clone()),
      &observed.runtime_version,
    );
    compare_expected(
      &mut differences,
      "driver_version",
      &Some(candidate.driver_version.clone()),
      &observed.driver_version,
    );
    compare_expected(
      &mut differences,
      "public_endpoint_name",
      &Some(PUBLIC_ENDPOINT_NAME.to_owned()),
      &observed.public_endpoint_name,
    );
    compare_expected(
      &mut differences,
      "public_endpoint_identity",
      &Some(PRODUCTION_ENDPOINT_IDENTITY.to_owned()),
      &observed.public_endpoint_identity,
    );
    compare_expected(
      &mut differences,
      "producer_interface_present",
      &true,
      &observed.producer_interface_present,
    );
    compare_contains(
      &mut differences,
      "services",
      PRODUCTION_SERVICE_NAME,
      &observed.services,
    );
    compare_contains(
      &mut differences,
      "driver_packages",
      PRODUCTION_DRIVER_IDENTITY,
      &observed.driver_packages,
    );
    compare_expected(
      &mut differences,
      "trust.production_signature_verified",
      &true,
      &observed.trust.production_signature_verified,
    );
    compare_expected(
      &mut differences,
      "trust.test_signing_enabled",
      &false,
      &observed.trust.test_signing_enabled,
    );
    compare_expected(
      &mut differences,
      "trust.signer_thumbprint",
      &Some(candidate.signer_thumbprint.clone()),
      &observed.trust.signer_thumbprint,
    );
    compare_expected(
      &mut differences,
      "default_input_roles",
      &self.journal.before.default_input_roles,
      &observed.default_input_roles,
    );
    compare_expected(
      &mut differences,
      "unrelated_audio_endpoints",
      &self.journal.before.unrelated_audio_endpoints,
      &observed.unrelated_audio_endpoints,
    );
    if let Some(previous_session) = self.journal.before.active_session_id.as_ref() {
      if observed.active_session_id.as_ref() == Some(previous_session) {
        differences.push(InventoryDifference {
          field: "active_session_id".to_owned(),
          expected: "new session or absent".to_owned(),
          observed: previous_session.clone(),
        });
      }
    }
    differences
  }
}

impl LifecycleInventory {
  /// Validates the metadata shape before it can be used as a lifecycle baseline or postcondition.
  pub fn validate(&self) -> Result<(), LifecycleError> {
    if self.schema_version != INVENTORY_SCHEMA_VERSION {
      return Err(LifecycleError::InvalidInventory {
        field: "schema_version",
        message: format!(
          "expected {}, got {}",
          INVENTORY_SCHEMA_VERSION, self.schema_version
        ),
      });
    }
    if self.product_release.is_none()
      && (self.runtime_version.is_some() || self.driver_version.is_some())
    {
      return Err(LifecycleError::InvalidInventory {
        field: "component_versions",
        message: "runtime and driver versions require a product release".to_owned(),
      });
    }
    if self.active_session_id.is_some() && !self.active_session {
      return Err(LifecycleError::InvalidInventory {
        field: "active_session_id",
        message: "an active session identity requires active_session=true".to_owned(),
      });
    }
    if self
      .trust
      .signer_thumbprint
      .as_deref()
      .is_some_and(str::is_empty)
    {
      return Err(LifecycleError::InvalidInventory {
        field: "trust.signer_thumbprint",
        message: "signer thumbprint must not be empty when present".to_owned(),
      });
    }
    Ok(())
  }

  #[must_use]
  pub fn product_absent(&self) -> bool {
    self.product_release.is_none()
      && self.runtime_version.is_none()
      && self.driver_version.is_none()
      && self.public_endpoint_name.is_none()
      && self.public_endpoint_identity.is_none()
      && !self.producer_interface_present
      && !self.services.contains(PRODUCTION_SERVICE_NAME)
      && !self.driver_packages.contains(PRODUCTION_DRIVER_IDENTITY)
      && !self.trust.production_signature_verified
      && self.trust.signer_thumbprint.is_none()
      && !self.trust.test_signing_enabled
      && !self.active_session
  }

  /// Returns a baseline that cannot accidentally reuse the old producer session or PCM.
  #[must_use]
  pub fn without_active_session(&self) -> Self {
    let mut target = self.clone();
    target.active_session = false;
    target.active_session_id = None;
    target
  }

  fn require_product_absent(&self) -> Result<(), LifecycleError> {
    if self.product_absent() {
      Ok(())
    } else {
      Err(LifecycleError::InvalidRestorationTarget)
    }
  }

  fn require_installed_release(&self, installed: &ReleaseManifest) -> Result<(), LifecycleError> {
    let identity = LifecycleReleaseIdentity::from_manifest(installed);
    if self.product_release.as_deref() != Some(identity.release_version.as_str())
      || self.runtime_version.as_deref() != Some(identity.runtime_version.as_str())
      || self.driver_version.as_deref() != Some(identity.driver_version.as_str())
      || self.public_endpoint_name.as_deref() != Some(PUBLIC_ENDPOINT_NAME)
      || self.public_endpoint_identity.as_deref() != Some(PRODUCTION_ENDPOINT_IDENTITY)
      || !self.producer_interface_present
      || !self.services.contains(PRODUCTION_SERVICE_NAME)
      || !self.driver_packages.contains(PRODUCTION_DRIVER_IDENTITY)
      || !self.trust.production_signature_verified
      || self.trust.test_signing_enabled
    {
      return Err(LifecycleError::InstalledInventoryMismatch);
    }
    Ok(())
  }
}

/// Errors that stop a lifecycle operation before it can claim success.
#[derive(Debug, Eq, PartialEq)]
pub enum LifecycleError {
  Manifest(ManifestError),
  Package(ReleaseError),
  Compatibility(CompatibilityError),
  Transition(super::TransitionError),
  InvalidInventory {
    field: &'static str,
    message: String,
  },
  InvalidRestorationTarget,
  ExistingProductState,
  InstalledInventoryMismatch,
  CandidatePackageMissing,
  InstalledManifestMissing,
  CandidatePackageMismatch,
  ElevatedAuthorityRequired,
  OperationNotAllowed {
    operation: LifecycleOperation,
    state: LifecycleState,
  },
  Backend {
    action: &'static str,
    message: String,
  },
  MixedRelease(String),
  PostconditionMismatch {
    operation: LifecycleOperation,
    differences: Vec<InventoryDifference>,
  },
}

impl Display for LifecycleError {
  fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
    match self {
      Self::Manifest(error) => write!(formatter, "manifest validation failed: {error}"),
      Self::Package(error) => write!(formatter, "package preflight failed: {error}"),
      Self::Compatibility(error) => write!(formatter, "release compatibility failed: {error}"),
      Self::Transition(error) => write!(formatter, "invalid lifecycle transition: {error}"),
      Self::InvalidInventory { field, message } => write!(formatter, "{field}: {message}"),
      Self::InvalidRestorationTarget => write!(
        formatter,
        "restoration target contains product-owned state and cannot be a clean baseline"
      ),
      Self::ExistingProductState => write!(
        formatter,
        "install requires an absent product baseline; use upgrade or recovery for existing state"
      ),
      Self::InstalledInventoryMismatch => write!(
        formatter,
        "saved inventory does not describe the installed production release"
      ),
      Self::CandidatePackageMissing => write!(formatter, "candidate package is required"),
      Self::InstalledManifestMissing => write!(formatter, "installed manifest is required"),
      Self::CandidatePackageMismatch => write!(formatter, "candidate package identity changed"),
      Self::ElevatedAuthorityRequired => write!(
        formatter,
        "machine-changing lifecycle work requires an elevated maintenance boundary"
      ),
      Self::OperationNotAllowed { operation, state } => {
        write!(
          formatter,
          "operation {operation:?} is not allowed from state {state:?}"
        )
      }
      Self::Backend { action, message } => write!(formatter, "{action} failed: {message}"),
      Self::MixedRelease(identity) => write!(
        formatter,
        "observed inventory contains an unverified mixed release: {identity}"
      ),
      Self::PostconditionMismatch {
        operation,
        differences,
      } => write!(
        formatter,
        "{operation:?} postconditions failed with {} difference(s)",
        differences.len()
      ),
    }
  }
}

impl Error for LifecycleError {}

fn compare_expected<T: Debug + Eq>(
  differences: &mut Vec<InventoryDifference>,
  field: &str,
  expected: &T,
  observed: &T,
) {
  if expected != observed {
    differences.push(InventoryDifference {
      field: field.to_owned(),
      expected: format!("{expected:?}"),
      observed: format!("{observed:?}"),
    });
  }
}

fn compare_contains(
  differences: &mut Vec<InventoryDifference>,
  field: &str,
  expected: &str,
  observed: &std::collections::BTreeSet<String>,
) {
  if !observed.contains(expected) {
    differences.push(InventoryDifference {
      field: field.to_owned(),
      expected: format!("contains {expected}"),
      observed: format!("{observed:?}"),
    });
  }
}
