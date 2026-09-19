## Purpose

Define a reproducible, production-trusted Windows 11 x64 driver package for MiniAEC from the pinned SysVAD source, with verifiable INF/SYS/CAT coverage and a safe handoff to the existing production lifecycle.

## ADDED Requirements

### Requirement: Pinned SysVAD source provenance

The production driver package SHALL be generated only from the Microsoft `Windows-driver-samples` `audio/sysvad` commit `2ee527bfeb0aeb6be11f0a8b6dce4011b358ce89`, the imported-file inventory, and the local patch ledger recorded in `driver/windows/UPSTREAM.md`, and SHALL publish enough source and toolchain provenance to reproduce the package verification.

#### Scenario: Pinned source is accepted
- **WHEN** the recorded upstream repository, path, commit, imported files, local patch set, and declared x64 build prerequisites match the package inputs
- **THEN** the package process records the immutable source identity, source content digests, local patch identities, tool versions, and build commands in release evidence

#### Scenario: Source or patch provenance differs
- **WHEN** the checkout commit, imported-file set, source digest, or local patch ledger differs from the recorded baseline
- **THEN** production package generation and signing stop before producing an accepted release artifact and report the mismatched provenance field

### Requirement: Production driver identity and endpoint scope

The production driver package SHALL target Windows 11 x64, use a production-only driver and service identity, contain the production INF/SYS/CAT payload, expose exactly one supported public capture endpoint named `MiniAEC Microphone`, keep `MiniAECTransport` private to the producer path, and preserve the fixed 48 kHz mono PCM16 10 ms transport contract without adding a render endpoint.

#### Scenario: Production package contents are inspected
- **WHEN** a reviewer inspects a candidate package and its manifest
- **THEN** the package identifies the Windows 11 x64 target, production driver identity, `MiniAEC Microphone`, `MiniAECTransport`, protocol version 1, diagnostics schema 2, 48 kHz mono PCM16 frames of 480 samples, and capture-only endpoint scope

#### Scenario: Development package is presented as production
- **WHEN** a package contains `MiniAECValidation`, the `validation-x64-debug` identity, a development certificate, an implicit test-signing requirement, or a render-category entry
- **THEN** production package verification rejects it before lifecycle staging and reports the development or endpoint-scope violation

### Requirement: Lifecycle-compatible package manifest

The production package SHALL provide the existing `manifest.json` release identity and `trust/signing-evidence.json` public evidence at the paths defined by `production-driver-lifecycle`, SHALL identify the runtime and driver versions and their compatibility range, and SHALL expose stable relative paths and package identity for lifecycle preflight, staging, upgrade, rollback, and uninstall decisions.

#### Scenario: Package handoff is compatible
- **WHEN** a production package has valid manifest identity, driver paths, transport contract, trust summary, and runtime/driver compatibility metadata
- **THEN** the non-mutating package preflight returns a machine-readable pass that `production-driver-lifecycle` can use before authorization or staging

#### Scenario: Package handoff is incompatible
- **WHEN** the package manifest, driver identity, endpoint identity, transport contract, or runtime/driver compatibility range disagrees with the lifecycle contract
- **THEN** preflight fails with the exact incompatible field and does not allow the package to be treated as an installable or upgradable release

### Requirement: Formal production trust for INF, SYS, and CAT

The production package SHALL use the approved external Windows production signing route, SHALL prove that the exact packaged INF and SYS contents are covered by the signed CAT, SHALL verify the CAT's production trust chain, SHALL verify any embedded SYS signature required by that route, and SHALL record separate verification results for INF coverage, SYS coverage, and CAT signature without including private signing material.

#### Scenario: Formally signed package passes verification
- **WHEN** the packaged INF and SYS hashes match catalog members and the CAT verifies through the approved production trust chain with no test-signing prerequisite
- **THEN** the verifier records a production trust pass for the INF, SYS, and CAT, including signer identity, public chain metadata, verification tool version, and verification time or timestamp evidence

#### Scenario: Signing or catalog coverage fails
- **WHEN** the CAT is unsigned, signed by a development or unapproved identity, cannot be verified, or does not cover the exact packaged INF or SYS bytes
- **THEN** verification fails closed, identifies the affected file or trust check, and prevents the package from being handed to lifecycle staging

### Requirement: Reproducible package build and verification

The package process SHALL support a clean replay from the recorded source, build inputs, x64 Windows toolchain, and package metadata, SHALL compare a canonical payload digest and catalog member set across replays, and SHALL distinguish reproducible package content from variable detached-signature timestamps or certificate encoding.

#### Scenario: Clean replay reproduces the payload
- **WHEN** two clean package builds use the same pinned source, toolchain declaration, build configuration, INF inputs, and release metadata
- **THEN** the verifier obtains the same canonical INF/SYS payload digests and catalog coverage set, and the signed CAT is checked for trust and coverage without requiring its time-varying signature bytes to be identical

#### Scenario: Replay detects input drift
- **WHEN** a source file, tool version, build flag, INF input, package path, or release metadata value changes between the recorded build and a replay
- **THEN** the verifier reports the changed input and does not mark the package reproducible or production-ready

### Requirement: Read-only fail-closed package preflight

Production package preflight SHALL inspect package files, manifest data, provenance, hashes, catalog coverage, signatures, trust evidence, and development-separation markers without installing or updating a driver, writing the certificate store, changing BCD or TESTSIGNING, changing device or default-audio state, or invoking restart, shutdown, or sign-out.

#### Scenario: Valid package is preflighted
- **WHEN** a candidate package passes identity, provenance, endpoint scope, content digest, INF/SYS/CAT trust, compatibility, and private-material checks
- **THEN** preflight emits a reviewable pass report and leaves Windows device, certificate, boot, default-role, and restart state unchanged

#### Scenario: Preflight finds a release defect
- **WHEN** any required file, digest, manifest field, trust result, catalog member, provenance record, or production-separation check is missing or invalid
- **THEN** preflight emits an actionable failure report and stops before any machine-changing operation or lifecycle authorization can occur

### Requirement: Metadata-only release evidence

Each accepted package SHALL include or reference reviewable metadata containing source commit and license provenance, imported-file and local-patch identity, build environment and commands, package file digests, catalog coverage, signer and public trust-chain metadata, verification results, and lifecycle compatibility, and SHALL exclude PCM, meeting content, private recordings, private keys, and machine-specific secret material.

#### Scenario: Release evidence is reviewed
- **WHEN** a reviewer receives the production package and public evidence
- **THEN** the reviewer can reproduce the verification inputs and determine why INF, SYS, CAT, manifest compatibility, and development-separation checks passed without requiring access to private audio or signing secrets

#### Scenario: Private or sensitive material is discovered
- **WHEN** package assembly or evidence validation finds a private signing suffix, private key material, PCM, meeting recording, or `artifacts/` content
- **THEN** assembly rejects the candidate and reports the prohibited material without copying it into a release artifact

### Requirement: Explicit handoff to production-driver-lifecycle

The package capability SHALL provide an immutable, preflighted driver artifact and public evidence for the `production-driver-lifecycle` contract, SHALL leave installation, activation, upgrade, rollback, uninstall, and restart boundaries to that lifecycle, and SHALL never claim successful machine activation from package verification alone.

#### Scenario: Lifecycle receives a verified package
- **WHEN** `production-driver-lifecycle` receives a package with a passing package-preflight report and matching release manifest
- **THEN** it can use the package identity and evidence as inputs to its own authorization and staged-operation state without reusing development test-signing identities

#### Scenario: Package verification completes without installation
- **WHEN** package build, signing, or verification completes in the repository or a clean build environment
- **THEN** the result is limited to package readiness and no driver, certificate, device, boot, default-role, restart, shutdown, or sign-out mutation is performed or implied
