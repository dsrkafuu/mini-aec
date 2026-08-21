## Purpose

Define the trusted, recoverable Windows lifecycle that turns the validated MiniAEC runtime and `MiniAEC Microphone` endpoint into a distributable product without weakening the development validation boundary.

## ADDED Requirements

### Requirement: Verifiable production package identity

The distributable MiniAEC package SHALL identify its product, driver, runtime, version and supported Windows architecture, SHALL use the approved production trust path, and SHALL verify package integrity and compatibility before changing machine state.

#### Scenario: Trusted package is ready for installation
- **WHEN** a user starts installation with a package whose identity, integrity, architecture and production trust requirements are valid
- **THEN** the installer presents the package version and planned system components before requesting authorization for machine changes

#### Scenario: Package verification fails
- **WHEN** package identity, integrity, architecture, trust or compatibility verification fails
- **THEN** installation stops before changing driver, device, certificate, boot or default-audio state and reports the exact failed check

### Requirement: Product installation exposes the supported endpoint

The production installation SHALL install and activate the project-owned `MiniAEC Microphone` capture endpoint, SHALL keep the private producer interface out of ordinary audio endpoint enumeration, SHALL preserve unrelated physical audio endpoints, and SHALL report any endpoint or default-input role change caused by Windows.

#### Scenario: Product installation succeeds
- **WHEN** an authorized installation completes on a supported Windows 11 x64 system
- **THEN** Windows exposes exactly one supported public capture endpoint named `MiniAEC Microphone`
- **THEN** ordinary recording clients can select and consume that endpoint while the existing physical endpoints remain available

#### Scenario: Installation reaches a restart boundary
- **WHEN** endpoint activation or package registration cannot complete without an operating-system restart
- **THEN** the installer explains the pending state and required user action, does not silently restart the operating system, and does not claim activation is complete before verification succeeds

### Requirement: Ordinary-user product runtime

The installed product SHALL provide a supported non-elevated interactive runtime path that can publish bypass or frozen-default-AEC PCM to `MiniAEC Microphone`, SHALL expose actionable access or ownership errors, and SHALL not require a UAC prompt for ordinary start, stop or restart operations.

#### Scenario: Ordinary user starts the installed runtime
- **WHEN** an interactive non-elevated user starts MiniAEC after a supported production installation
- **THEN** the runtime opens the supported producer path and ordinary capture clients receive fresh bypass or frozen-default-AEC output through `MiniAEC Microphone` without elevation

#### Scenario: Another local process contends for the producer path
- **WHEN** a second local process attempts to publish while the supported producer session is owned
- **THEN** the runtime reports a distinct ownership or busy condition, does not mix sessions or expose stale PCM, and can reconnect after the owner releases the session

### Requirement: Compatible upgrade and interrupted-operation recovery

The product lifecycle SHALL determine whether a runtime and driver package can be upgraded together, SHALL reject incompatible combinations before activation, SHALL preserve the public endpoint contract across a compatible upgrade, and SHALL leave either the previous working release or an explicitly recoverable state after an interrupted operation.

#### Scenario: Compatible upgrade completes
- **WHEN** an authorized upgrade passes identity, compatibility and preflight checks
- **THEN** the new runtime and driver operate as one compatible release, `MiniAEC Microphone` retains its public identity and protocol behavior, and the prior release remains available through the documented rollback path until the upgrade is verified

#### Scenario: Incompatible upgrade is attempted
- **WHEN** the requested runtime and driver versions cannot safely operate together
- **THEN** the lifecycle refuses activation before publishing PCM, preserves the last known working installation, and reports the incompatible identities and required recovery action

#### Scenario: Upgrade is interrupted
- **WHEN** installation, activation or verification stops unexpectedly after an upgrade has begun
- **THEN** the lifecycle records the incomplete state, prevents an unverified mixed release from being used, and offers a documented recovery path without deleting unrelated physical audio state

### Requirement: Complete rollback and uninstall

The product lifecycle SHALL support removal of the production package and all product-owned device, service and producer-interface state, SHALL prevent old-session PCM from surviving a rollback or reinstall, SHALL compare the resulting endpoint and default-input roles with the saved pre-install baseline, and SHALL not claim success while any targeted state remains.

#### Scenario: Product uninstall completes
- **WHEN** an authorized uninstall finishes and any required user-performed restart or follow-up has completed
- **THEN** `MiniAEC Microphone`, its production producer interface, targeted package and service state are absent, unrelated physical endpoints remain available, and saved default-input roles match the pre-install baseline

#### Scenario: Rollback verification finds a difference
- **WHEN** an endpoint, package, service, session, or saved default-input role differs from the recorded baseline after rollback
- **THEN** the lifecycle reports each remaining difference, keeps the result incomplete, and requires explicit user-directed recovery before claiming rollback success

### Requirement: Development and production lifecycle separation

The production release SHALL remain distinguishable from the development test-signing workflow, SHALL never enable test signing or install development-only identities as a side effect of production installation, and SHALL keep the frozen AEC baseline, fixed PCM contract and bounded synchronization contract unchanged.

#### Scenario: Production installation is inspected
- **WHEN** a reviewer inspects a production package and its installation evidence
- **THEN** it contains production package identity and trust evidence, contains no implicit test-mode activation, and clearly identifies the supported runtime and driver versions

#### Scenario: Development package is used on a production system
- **WHEN** a development-only package is presented to the production lifecycle
- **THEN** the lifecycle rejects it as non-production and does not install or activate it through the production path

### Requirement: Product release compatibility evidence

Each production release SHALL have reviewable Windows 11 x64 evidence for installation, endpoint enumeration, ordinary-user runtime, Windows Recorder consumption, at least one target meeting application, sender ownership, upgrade, rollback, uninstall, device or session recovery, and preservation of unrelated endpoint state.

#### Scenario: Release candidate passes the compatibility matrix
- **WHEN** a release candidate is evaluated on the supported Windows 11 x64 matrix
- **THEN** every required scenario produces a recorded pass or an explicitly documented unsupported result, and no release is marked distributable with an unexplained endpoint, access, stale-audio, rollback or client-continuity failure

#### Scenario: Compatibility evidence is incomplete
- **WHEN** one or more required matrix scenarios lack evidence or have an unexplained failure
- **THEN** the release remains non-distributable and the missing or failed scenario is identified as a release blocker
