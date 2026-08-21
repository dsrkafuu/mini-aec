## Context

M1 through M4 establish a project-owned `MiniAEC Microphone` endpoint, a bounded real-time engine, a frozen M131 default-AEC path, non-elevated development runtime access, and a passed 30-minute stability gate. The current Windows lifecycle is intentionally development-only: test signing, explicit system-change approval, saved inventories, user-performed restart boundaries, and complete rollback. Production signing, release packaging, coordinated runtime/driver versions, and a production rollback contract are not yet defined. See proposal.md for the motivation and specs/production-driver-lifecycle/spec.md for the observable contract.

The design must preserve the fixed 48 kHz mono PCM16/10 ms transport, the public endpoint name, the project-owned transport boundary, the non-elevated runtime contract, the frozen WebRTC M131 adapter, and the rule that the agent never initiates operating-system restart, shutdown, sign-out, installation, or rollback actions.

## Goals / Non-Goals

**Goals:**

- Make the driver and Rust/Tauri runtime a versioned, mutually compatible production release unit with reviewable package identity and signing evidence.
- Provide a staged install, upgrade, verification, rollback, and uninstall lifecycle that records before/after device and default-input state and never claims success before postconditions are checked.
- Keep machine-changing operations in an explicit elevated installer or maintenance boundary while ordinary runtime and tray operations remain non-elevated.
- Preserve the existing real-time audio contracts and make release acceptance cover `MiniAEC Microphone` end to end through ordinary Windows clients.
- Define an initial single-user Windows desktop threat boundary that is honest about local same-user producer contention and does not claim per-executable trust that the current transport cannot enforce.

**Non-Goals:**

- Changing AEC3 parameters, WebRTC pins, synchronization policy, PCM format, driver ring capacity, or the M3/M4 evidence schema.
- Adding a WebView, settings frontend, device-selection UI, automatic default-device following, or a new audio endpoint.
- Introducing a service broker or per-executable attestation mechanism in this change; those are separate hardening work if the target distribution threat model requires them.
- Automating test-signing, production installation, device activation, rollback, or operating-system restart from the agent or ordinary runtime.
- Handling private recordings as release artifacts or adding telemetry that contains PCM or meeting content.

## Decisions

### 1. Use one versioned release manifest for the runtime and driver

The release unit will contain the Rust/Tauri runtime, the signed driver package, and a machine-readable manifest that identifies the product version, driver version, runtime version, supported Windows architecture, transport protocol version, public endpoint name, and compatibility bounds. The manifest is the compatibility source of truth used before install, upgrade, and activation.

Keeping the runtime and driver in one release unit prevents an independently updated driver from being paired with an older sender or engine. Independently versioned packages were rejected for the first production lifecycle because they make partial upgrades and rollback harder to reason about; a future protocol migration can introduce explicit compatibility ranges without changing this release model.

### 2. Keep lifecycle authority separate from the runtime

An elevated installer or maintenance boundary will own package registration, driver/service changes, production certificate or trust prerequisites, and removal. The tray host and headless audio runtime will remain non-elevated and will only start, stop, restart, and consume an already installed product. Lifecycle operations will use a staged state machine: preflight, authorized, staged, activated, verified, or recoverable-failure/rollback.

Embedding installation in the tray host was rejected because it would mix low-frequency UI lifecycle control with UAC-sensitive machine changes and make an ordinary runtime failure look like a package failure. The lifecycle boundary must be able to stop at a restart boundary, report the exact pending operation, and wait for the user rather than restarting Windows.

### 3. Preserve the current direct interactive producer boundary for the initial product scope

The first production target is a trusted single-user interactive Windows desktop. It will retain the existing non-elevated producer path, protected device ACL, one-owner sender slot, explicit busy result, session identity, sequence validation, and stale-session flush. Release evidence will state that any local interactive process permitted by the ACL can contend for the machine-wide producer slot; this is an availability/integrity limitation of the initial threat model, not a claim of per-executable trust.

A service broker or per-executable trust scheme was considered, but is not selected for M5 because it would introduce a new real-time IPC boundary and a new privileged component while the current audio path is already accepted. Broader multi-user, hostile-local-process, or enterprise distribution requirements must first produce a separate security change that specifies the broker boundary and its latency/failure behavior.

### 4. Treat the development package as a separate lifecycle

Development test-signed packages, test-mode prerequisites, lifecycle scripts, validation inventories, and user-approved rollback remain under the existing `driver-development-lifecycle` contract. The production package must carry production identity and trust evidence and must not enable test signing or reuse development-only package identities. The two workflows may share build inputs and verification helpers only when the output and system-state boundaries remain explicit.

This separation avoids weakening the safe development workflow to make it look like a release installer and avoids allowing a development package to be mistaken for a distributable product.

### 5. Make upgrade and rollback transactional at the product boundary

Before a machine-changing operation, the lifecycle records the current product/package identity, endpoint inventory, driver/service identity, certificate or trust state relevant to the product, default-input roles, and active-session status. It stages the new release, verifies compatibility before activation, activates only the matching runtime/driver pair, and runs post-activation checks through `MiniAEC Microphone`. If activation or verification fails, it restores the prior package and recorded state where possible, or reports a recoverable incomplete state without claiming success.

Uninstall follows the same inventory discipline and verifies absence of the product endpoint, producer interface, targeted package and service state, stale session/PCM state, and default-role differences. Windows-originated role changes are recorded; active role restoration remains an explicitly authorized operation. A required operating-system restart is a user-performed boundary and is represented as pending until read-only verification completes.

### 6. Keep the signing secret outside the repository

The repository will record the production signing route, package identity evidence, public certificate or chain metadata, and reproducible verification commands, but never store private signing keys or machine-specific certificate material. The release process will fail closed when the required production trust evidence is missing or when a development identity is supplied.

The exact external certificate identity and packaging service are release inputs rather than hard-coded application dependencies. The release manifest and verification contract remain stable if the approved signing provider changes.

### 7. Verify the product path with layered evidence

Repository tests and package inspection will cover manifest compatibility, lifecycle state transitions, protocol identity, ACL policy, and rollback decision logic without changing Windows state. An explicitly approved Windows 11 x64 acceptance run will cover installation, endpoint enumeration, non-elevated runtime, Windows Recorder, at least one target meeting client, contention, upgrade, restart/recovery where applicable, uninstall, default-role comparison, and complete rollback.

Private recordings, if explicitly produced by the user for listening, remain under ignored `artifacts/` and are not part of the distributable evidence set. Metadata and operator observations are sufficient for automated release decisions; no release gate will treat a process-alive check as proof of audible continuity.

## Risks / Trade-offs

- [Production signing route or Windows packaging prerequisites are unavailable] → Keep the change in a non-distributable preflight state, report the missing prerequisite, and do not substitute test signing for production evidence.
- [A lifecycle operation is interrupted at a driver or restart boundary] → Persist staged-state identity and pre-change inventory, stop before claiming success, and require user-performed restart or explicit rollback verification.
- [A compatible-looking runtime and driver still disagree at runtime] → Require manifest compatibility checks plus a post-activation `MiniAEC Microphone` smoke test before marking the release active.
- [Windows changes a default input role during installation] → Record all roles before and after, preserve unrelated endpoints, and require explicit authorization before any active restoration during rollback.
- [A local interactive process contends with the producer interface] → Preserve explicit busy ownership and session flush behavior, surface the limitation in release evidence, and block broader threat models until a separate broker/security change is approved.
- [Installer implementation adds latency or blocking work to the audio path] → Keep lifecycle code outside capture, synchronization, AEC, and sink workers; run real-time regression checks before release acceptance.
- [Compatibility matrix evidence includes private audio] → Store only user-requested recordings under ignored `artifacts/`; keep committed and distributable evidence metadata-only.

## Migration Plan

1. Freeze the current development package and capture a clean read-only inventory and compatibility baseline before introducing production packaging artifacts.
2. Add the release manifest, production package verification, and lifecycle state model without changing the existing development scripts or runtime audio contracts.
3. Build and inspect a production release candidate using externally supplied signing evidence; stop if production trust prerequisites are missing.
4. Run repository-level lifecycle and compatibility tests without system mutation, then obtain separate explicit approval for the Windows installation and acceptance run.
5. Install the candidate, perform the approved end-to-end matrix, and retain metadata-only release evidence plus any user-managed private listening material under ignored paths.
6. If acceptance fails, use the recorded package identity and inventory to roll back; the user performs any required operating-system restart, and the lifecycle is not marked complete until read-only post-rollback checks pass.
7. Promote the candidate only after all required scenarios pass and the release manifest, signing evidence, compatibility results, and rollback evidence are reviewable.

