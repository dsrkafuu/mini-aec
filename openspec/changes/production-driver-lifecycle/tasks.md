## 1. Documentation and release contract

- [x] 1.1 Define the production release manifest fields, version compatibility ranges, supported Windows 11 x64 scope, public endpoint identity, and fixed transport protocol identity.
- [x] 1.2 Document the approved production signing inputs, trust evidence, private-key boundary, and the explicit separation from development test signing.
- [x] 1.3 Document lifecycle states, pre/post inventories, endpoint and default-input role checks, restart boundaries, rollback postconditions, and incomplete-operation recovery.
- [x] 1.4 Define the Windows compatibility matrix and metadata-only release evidence format for endpoint enumeration, ordinary-user runtime, Recorder, target meeting clients, contention, upgrade, recovery, uninstall, and rollback.
- [x] 1.5 Record the initial trusted single-user threat boundary and the known limitation that the current direct producer ACL does not provide per-executable trust; do not claim service-broker isolation.

## 2. Release manifest and package boundary

- [x] 2.1 Implement project-owned release manifest parsing and validation for product, runtime, driver, architecture, endpoint, protocol, and compatibility identities.
- [x] 2.2 Implement fail-closed package preflight that rejects missing, untrusted, development-only, malformed, architecture-incompatible, or runtime/driver-incompatible release inputs before machine mutation.
- [x] 2.3 Define a reproducible production package layout containing the versioned runtime, signed driver package, manifest, public trust metadata, and verification instructions without private signing material.
- [x] 2.4 Keep development validation package outputs and test-signing lifecycle entry points separate from production package outputs and make the separation inspectable.

## 3. Production lifecycle implementation

- [ ] 3.1 Implement the elevated lifecycle boundary for staged install, activation, post-activation verification, upgrade, rollback, and uninstall without moving PCM processing or UI work onto real-time audio workers.
- [ ] 3.2 Implement before/after inventory capture for package, service, endpoint, default-input role, trust, and active-session state and make lifecycle success depend on verified postconditions.
- [ ] 3.3 Implement compatible upgrade handling that activates only a matching runtime/driver pair and preserves the `MiniAEC Microphone` endpoint identity and fixed transport contract.
- [ ] 3.4 Implement interrupted-operation recovery that records staged state, rejects unverified mixed releases, and exposes a documented rollback or user-recovery result.
- [ ] 3.5 Implement uninstall and rollback checks for product endpoint absence, targeted package/service removal, stale-session isolation, unrelated endpoint preservation, and default-input role comparison.
- [x] 3.6 Preserve the ordinary-user runtime path, explicit producer ownership/busy behavior, session identity checks, and fresh-session PCM isolation while ensuring lifecycle operations do not require runtime elevation.
- [x] 3.7 Ensure any operating-system restart requirement is represented as a pending user action and that no installer, runtime, script, or agent path initiates, schedules, or invokes restart, shutdown, or sign-out.

## 4. Automated verification and release acceptance

- [x] 4.1 Add repository-level tests for manifest validation, package/trust separation, compatibility decisions, lifecycle state transitions, failure recovery, and default-role comparison without changing Windows state.
- [ ] 4.2 Run clean x64 package preflight and inspect the generated production package for endpoint identity, driver category, trust metadata, absence of test-mode activation, and absence of private signing material.
- [x] 4.3 Run `cargo fmt --all -- --check`, `.tools\cargo-webrtc.cmd test --workspace`, and `.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings` before system acceptance.
- [ ] 4.4 After separate explicit user approval, execute the Windows 11 x64 install and activation matrix with saved inventory, ordinary-user runtime, `MiniAEC Microphone` consumption, Windows Recorder, and at least one target meeting client.
- [ ] 4.5 After separate explicit user approval, execute compatible upgrade, interrupted-operation recovery, rollback, uninstall, endpoint/default-role comparison, and any required user-performed restart boundaries.
- [ ] 4.6 Record sender contention, session recovery, device or endpoint recovery, unrelated physical endpoint preservation, and all unexplained failures in metadata-only release evidence.
- [ ] 4.7 Review the complete release manifest, signing evidence, compatibility results, rollback evidence, and known threat-model limitations and mark the release distributable only when every required acceptance scenario passes.
