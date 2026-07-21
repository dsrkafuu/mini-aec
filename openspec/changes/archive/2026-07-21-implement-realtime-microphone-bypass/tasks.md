## 1. Engine Boundary and Test Scaffolding

- [x] 1.1 Add the Tauri-independent `mini-aec-engine` workspace crate with project-owned engine configuration, command, state, snapshot, source and source-factory contracts and a dependency on `mini-aec-transport` rather than Windows, CLI, Tauri or WebRTC types in its public API.
- [x] 1.2 Add deterministic fake audio-source and virtual-sink adapters that can script packet sizes, silence, discontinuities, timestamp errors, source invalidation, backpressure and mapped sink failures without accessing Windows devices or private recordings.
- [x] 1.3 Add state-transition tests for start, normal stop, idempotent cleanup, failed start, terminal runtime failure and explicit restart with distinct run/session identities and no old-run PCM.

## 2. Normalization, Framing and Bounded Queue

- [x] 2.1 Implement and test finite `f32` sanitization, clamping and deterministic PCM16 conversion at zero, full-scale, out-of-range, NaN and infinity boundaries.
- [x] 2.2 Implement a preallocated packet accumulator that emits exact ordered 480-sample frames across arbitrary packet boundaries, converts silent packets to fresh zeros and clears partial PCM on discontinuity or timestamp error.
- [x] 2.3 Implement the four-frame synchronized latest-wins queue with complete-frame visibility, oldest-frame discard, dequeue-time protocol sequencing and current-depth, high-water, overflow and discarded-frame diagnostics.
- [x] 2.4 Add deterministic tests proving queue wraparound, backpressure recovery, gap-free submitted sequences, bounded storage and stale-frame isolation across stop, failure and restart.

## 3. Engine Runtime and Diagnostics

- [x] 3.1 Implement the `Stopped → Starting → RunningBypass → Stopping → Stopped` lifecycle and terminal `Failed` transition with one capture worker, one sink worker and finite cancellation/join behavior.
- [x] 3.2 Open exactly one virtual sink session per run, assign submitted sequences from zero at dequeue time, and close and flush the session after capture stops producing frames.
- [x] 3.3 Map source invalidation, unrecoverable capture errors, driver absence, access denial, sender contention, version mismatch and rejected writes into project-owned errors that stop both sides, clear buffered PCM and require explicit restart.
- [x] 3.4 Implement metadata-only `EngineSnapshot` and validation events for source identity, run/session identity, capture and silence counts, discontinuities, timestamp errors, output frames, queue depth/high-water, local overflow/discard, sink acceptance/failure and last error without logging PCM.
- [x] 3.5 Add a synthetic five-minute engine soak test that uses fake adapters, verifies exact frame accounting and bounded queue state, and completes with no unexplained loss, stale segment or unbounded resource growth.

## 4. Windows Physical Microphone Adapter

- [x] 4.1 Implement Windows capture-endpoint enumeration and exact-ID resolution for the engine, retain friendly/native-format metadata, require an explicit ID and reject the public `MiniAEC Microphone` endpoint before opening a sink session.
- [x] 4.2 Implement event-driven shared-mode WASAPI capture requesting 48 kHz mono `f32` with audio-engine conversion, preallocated packet storage and propagation of silence, discontinuity, timestamp-error, device-position and QPC metadata.
- [x] 4.3 Implement finite event waits, cancellation, stream stop and device-invalidation cleanup without file I/O, console I/O, unbounded queues or Tauri/async-runtime waits on the capture thread.
- [x] 4.4 Refactor only reusable endpoint-enumeration or WASAPI setup behavior from `mini-aec-lab`, preserve the existing `devices`, `capture` and offline AEC commands, and add regression tests for their argument and output contracts where practical.

## 5. Headless Bypass Validation Surface

- [x] 5.1 Add a headless `mini-aec-lab bypass` command requiring an explicit physical microphone endpoint ID and duration, connecting the existing Windows virtual microphone adapter and exiting nonzero on start or runtime failure.
- [x] 5.2 Write periodic and final metadata-only engine snapshots below an ignored run evidence directory, print the run path outside the real-time workers and refuse any evidence path that would place private PCM under version control.
- [x] 5.3 Add no-driver, access-denied, recursive-MiniAEC-source, busy-sender and device-invalidated validation tests that produce actionable errors without changing BCD, certificates, drivers, devices or Windows default audio roles.
- [x] 5.4 Build the engine and headless command on Windows and confirm normal unit/integration tests need neither Administrator privileges nor an installed validation driver.

## 6. Documentation and Non-Mutating Acceptance

- [x] 6.1 Update the root README and technical plan to mark the M1 transport validation complete, identify M2 real-time bypass as active and retain render loopback, clock alignment, AEC3, tray integration and production driver access as later milestones.
- [x] 6.2 Document the explicit-ID source policy, four-frame engine queue, thread model, lifecycle, failure behavior, diagnostics, development-only elevated end-to-end limitation and privacy boundary without presenting bypass as an AEC failure fallback.
- [x] 6.3 Run `cargo fmt --all -- --check`, `.tools\cargo-webrtc.cmd test --workspace` and `.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings`, and confirm no generated package, certificate material, metadata log or private recording is tracked.

## 7. Approved End-to-End Windows Validation and Rollback

- [x] 7.1 Save a new read-only pre-change Inventory and present the exact TESTSIGNING, certificate, install, recording, restart, uninstall and boot-state rollback commands and impacts; obtain explicit user approval separately before every system-changing action.
- [x] 7.2 After approved test-signing and installation, verify exactly one `MiniAEC Microphone` endpoint, no producer endpoint, the expected default-role behavior and unchanged unrelated physical endpoints before running the engine.
- [x] 7.3 Run the elevated headless engine with an explicitly selected physical microphone, record `MiniAEC Microphone` for at least five minutes and verify expected duration plus engine/driver counters with no unexplained gap, stale segment, rejected write, overflow, discard or discontinuity.
- [x] 7.4 Stop the engine while capture remains open, verify fresh silence without old microphone replay, start a distinct run and verify capture resumes from a new sink session and sequence zero without pre-stop PCM.
- [x] 7.5 Validate one approved driver or device restart and one controlled source/sink failure, then explicitly restart the engine and verify endpoint return, terminal-failure reporting and new-run isolation without reinstalling unless the approved lifecycle requires it.
- [x] 7.6 Execute the separately approved uninstall and rollback, remove only the recorded device, package and certificate, restore the saved default input roles and TESTSIGNING state, complete any required reboot, and verify final Inventory matches the saved baseline before declaring M2 accepted.
