## 1. Upstream and Toolchain Baseline

- [x] 1.1 Add `driver/windows/UPSTREAM.md` with the fixed `microsoft/Windows-driver-samples` repository, `audio/sysvad` path, commit `2ee527bfeb0aeb6be11f0a8b6dce4011b358ce89`, retrieval date, MS-PL identity, imported file inventory and a local-patch ledger.
- [x] 1.2 Add the applicable MS-PL license text and source notices required for the imported SysVAD files, and verify that no file comes from the unpinned `microsoft/audio` copy or another repository.
- [x] 1.3 Add a read-only Windows driver preflight command that reports Windows, Visual Studio, SDK, WDK, MSBuild and signing-tool versions without changing boot, certificate, driver or device state.
- [x] 1.4 Import the minimum fixed SysVAD source needed by the prototype and record every build-only adaptation outside upstream AEC code in the patch ledger.
- [x] 1.5 Build the unmodified or minimally adapted x64 Debug SysVAD baseline with the documented toolchain and record the clean-checkout command and result before adding a transport.

## 2. Shared Validation Contract

- [x] 2.1 Define a project-owned `VirtualMicrophoneSink` boundary for fixed-format session open, PCM write, diagnostics and close operations without exposing WaveRT, IOCTL or SysVAD types to the caller.
- [x] 2.2 Implement the minimal user-mode sender with a deterministic 48 kHz mono PCM16 signal, periodic audible markers, session identity, monotonic frame sequence logging and explicit error reporting.
- [x] 2.3 Define one versioned diagnostics schema for session transitions, accepted frames, rejected writes, underruns, overflows and driver restarts that the validation transport must emit.
- [x] 2.4 Add unit tests for signal determinism, frame sequencing, session reset and transport adapter error mapping without accessing a physical microphone or private `artifacts/` data.
- [x] 2.5 Document the five-minute input, endpoint-enumeration checks, Windows Recorder steps, sender restart sequence, driver restart sequence and common evidence fields used by the validation path.

## 3. Restricted Driver Ingress and Ring Buffer

- [x] 3.1 Adapt the pinned SysVAD source to expose exactly one public capture endpoint named `MiniAEC Microphone` plus a private driver control interface that is not an audio endpoint.
- [x] 3.2 Restrict the validation control interface to SYSTEM and Administrators, allow one active sender session, and cleanly release the session when its control handle closes or its process exits.
- [x] 3.3 Define and implement the versioned fixed-frame control protocol with protocol version, lengths, session identity, monotonic frame sequence and exactly 960 bytes of PCM payload per request.
- [x] 3.4 Validate every request before copying, reject unsupported versions, malformed lengths, stale sessions, partial frames and non-monotonic sequences, and return actionable status codes.
- [x] 3.5 Implement a synchronized driver-owned nonpaged ring buffer with capacity for 10 complete frames, no user-mode mapping and no partial-frame visibility across wraparound.
- [x] 3.6 Implement audio-clock-driven capture consumption, zero-valued underrun output, oldest-frame discard on overflow, atomic old-session flush and diagnostics for depth, high-water mark, underrun, overflow, discarded frames and rejected writes.

## 4. User-mode Adapter and Non-system Tests

- [x] 4.1 Implement the Windows `VirtualMicrophoneSink` adapter for the private control protocol and map access denial, driver absence, busy sender, version mismatch and rejected writes into project-owned errors.
- [x] 4.2 Extend the versioned diagnostics contract and sender logging with discarded-frame, current-depth and high-water-mark fields while preserving session and frame correlation.
- [x] 4.3 Add driver-boundary tests for malformed requests, ring wraparound, empty and full transitions, oldest-frame overflow discard, sender cleanup and new-session isolation.
- [x] 4.4 Add user-mode tests for protocol encoding, session open and close, monotonic sequence enforcement, error mapping and deterministic sender behavior without installing the driver.
- [x] 4.5 Build the x64 Debug test-signable driver package and sender from a clean checkout, verify the package declares no producer-only audio endpoint, and keep generated packages and certificates outside version control.

## 5. Approved Driver Lifecycle Validation

- [ ] 5.1 Present the exact test-mode, certificate, install, device-restart and uninstall commands plus rollback plan, and obtain explicit user approval before the first system-changing validation action.
- [ ] 5.2 Save a pre-install inventory of physical audio endpoints, default devices, relevant driver packages, services, certificates and test-signing state.
- [ ] 5.3 Install the approved test-signed validation package and verify that Windows adds exactly one public capture endpoint named `MiniAEC Microphone`, adds no producer-only audio endpoint, and does not change default or unrelated physical endpoints.
- [ ] 5.4 Record at least five minutes from `MiniAEC Microphone` with Windows Recorder and verify expected duration, ordered markers and diagnostics with no unexplained gap, stale segment, underrun, overflow or rejected write.
- [ ] 5.5 While one recording remains open, stop the sender, verify continuing silence without stale replay, restart the sender and verify capture resumes with a new session and no old-session PCM.
- [ ] 5.6 Execute the approved driver restart, reopen Windows Recorder after the endpoint returns, and verify a fresh sender session can be captured without reinstalling the package or exposing pre-restart PCM.
- [ ] 5.7 Execute the approved uninstall and rollback procedure, verify the endpoint, control interface, service, staged validation package and targeted test certificate are gone, and compare unrelated physical endpoints and boot state with the saved baseline.

## 6. Documentation and Final Acceptance

- [x] 6.1 Add a read-only-by-default validation entry point that prints planned system changes and requires explicit confirmation before test certificate, driver, restart or uninstall operations.
- [x] 6.2 Update `driver/windows/README.md`, `driver/windows/VALIDATION.md` and relevant architecture documentation with the fixed private control-interface transport, complete product signal-path diagram, fixed PCM protocol, 10-frame buffer, overflow policy, diagnostics, development-only signing limits and deferred production concerns.
- [ ] 6.3 Record clean-checkout commands, environment, evidence paths, pass/fail results and unresolved limitations, run the required workspace checks, and confirm that no generated driver package, certificate private key or private recording is committed.
