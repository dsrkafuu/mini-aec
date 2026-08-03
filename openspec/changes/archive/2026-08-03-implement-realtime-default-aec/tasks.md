## 1. Engine contracts and bypass compatibility

- [x] 1.1 Add project-owned processing-mode, explicit microphone/render endpoint configuration, `RunningAec` and degraded-state types without exposing WASAPI, WebRTC, Tauri or driver protocol types.
- [x] 1.2 Extend input descriptors, errors and factories with explicit capture and render-loopback roles while retaining exact-ID resolution and recursive `MiniAEC Microphone` rejection.
- [x] 1.3 Add project-owned `EchoCanceller` and factory contracts for render-first frame processing, capture output, reset/reconstruction, timing and actionable errors.
- [x] 1.4 Extend `EngineSnapshot` and validation events with mode, render, synchronization, AEC, degradation and bounded processing-time fields while keeping every existing bypass diagnostic meaningful.
- [x] 1.5 Update fake inputs, fake echo cancellers and fake sinks so bypass and AEC lifecycle tests can run deterministically without Windows system changes.
- [x] 1.6 Verify the existing bypass start, framing, queue, failure, stop and restart tests still pass with no render endpoint or echo canceller opened in bypass mode.

## 2. Timestamped framing and synchronization

- [x] 2.1 Add a fixed-capacity timestamped 480-sample frame representation that derives frame-start QPC time from packet timestamps and intra-packet sample offsets without steady-state allocation.
- [x] 2.2 Implement separate bounded microphone and render queues plus finite startup that selects the common origin, accounts for pre-origin frames and never publishes `RunningAec` before both roles establish a usable epoch.
- [x] 2.3 Implement capture-paced QPC pairing with documented capacity, timestamp-tolerance, finite-wait and sustained-skew constants.
- [x] 2.4 Implement counted silent render references, stale render discard and freshness-preserving queue-pressure behavior without unbounded latency or an automatic bypass transition.
- [x] 2.5 Implement discontinuity and timestamp-error handling that clears affected partial/unpaired frames, starts a new synchronization epoch and requests an AEC reset before further output.
- [x] 2.6 Implement terminal synchronization failure when sustained skew exceeds the documented M3 bound instead of claiming drift correction or repeatedly dropping/inserting whole frames indefinitely.
- [x] 2.7 Add synthetic tests for startup skew, packet/frame boundary timestamps, exact pairing, render underrun, stale render, queue overflow, discontinuity reset, sustained skew failure and stop/restart epoch isolation.

## 3. Frozen default M131 AEC adapter

- [x] 3.1 Add the existing pinned `webrtc-audio-processing 2.1.0` dependency to the real-time engine boundary without changing Cargo versions, the vendored source snapshot or `vendor/UPSTREAM.md` pins.
- [x] 3.2 Implement the adapter with `Processor::new(48_000)`, full echo cancellation and upstream-default AEC3 while explicitly leaving NS, AGC, experimental configuration and post-processing disabled.
- [x] 3.3 Reuse adapter-owned channel buffers, submit each render frame before its paired capture frame and return only finite normalized capture output.
- [x] 3.4 Implement adapter reset/reconstruction and project-owned error mapping without exposing WebRTC types through engine configuration, snapshots or tray integration.
- [x] 3.5 Add adapter tests for default configuration, render-before-capture ordering, finite output, reset isolation and rejection of invalid results using synthetic redistributable samples or generated in-memory signals.

## 4. Windows render-loopback input

- [x] 4.1 Extend exact endpoint resolution to validate capture versus render data-flow roles and retain friendly name plus native/requested format metadata for both inputs.
- [x] 4.2 Implement event-driven shared-mode WASAPI loopback capture for one explicit physical render endpoint with audio-engine conversion to the project 48 kHz mono `f32` contract.
- [x] 4.3 Ensure each microphone and render adapter creates, uses, stops and destroys its COM/WASAPI objects on its owning worker thread with finite cancellation waits and idempotent cleanup.
- [x] 4.4 Propagate render silence, discontinuity, timestamp error, device position and QPC timestamp metadata through the project-owned input contract.
- [x] 4.5 Add Windows adapter tests or read-only harness checks for wrong-role IDs, unavailable render devices, explicit endpoint selection, loopback format reporting, invalidation mapping and no default-device fallback.

## 5. Real-time AEC runtime and recovery

- [x] 5.1 Extend engine startup to resolve both AEC inputs, construct role workers, synchronizer, echo canceller and sink in a failure-safe order, publishing `RunningAec` only after the bounded timeline is ready.
- [x] 5.2 Add the processing worker that owns the synchronizer, echo canceller and virtual sink, processes render before capture and assigns sink protocol sequences only to dequeued output frames.
- [x] 5.3 Implement `Degraded` entry and recovery for bounded render-reference shortage and successful AEC reset, including an explicit degradation reason and healthy-frame recovery gate.
- [x] 5.4 Silence invalid AEC output frames and implement bounded adapter reconstruction; close the run and enter `Failed` when reconstruction or the documented repeated-failure policy is exhausted.
- [x] 5.5 Treat microphone/render invalidation, unrecoverable WASAPI errors, sustained skew and sink failures as terminal for the current run, clearing partial, queued and processed PCM without raw-microphone fallback.
- [x] 5.6 Make stop and explicit restart join all three workers with finite timeouts, close one sink session, reset sequence to zero and prevent PCM or synchronization/AEC state from crossing runs.
- [x] 5.7 Add bounded processing-time summaries and deadline/queue-pressure counters without file I/O, console I/O, unbounded channels, UI waits or uncontrolled steady-state allocation on real-time workers.
- [x] 5.8 Add deterministic runtime tests for healthy AEC, explicit bypass, temporary degradation and recovery, invalid AEC output, reconstruction failure, both input failures, sink failure, stop races and restart stale-audio isolation.

## 6. Headless and tray integration

- [x] 6.1 Add a headless real-time AEC command that requires exact microphone and render endpoint IDs, never installs a driver or changes Windows defaults and exits nonzero on terminal engine failure.
- [x] 6.2 Persist periodic and final AEC metadata-only JSONL evidence outside real-time workers under an ignored validation output path, including both endpoint identities, alignment, AEC, queue and sink summaries.
- [x] 6.3 Connect the windowless Tauri host to the project-owned controller and display stopped, starting, running AEC, degraded AEC, explicit bypass and failed state without moving PCM or WebRTC work into Tauri.
- [x] 6.4 Add explicit tray AEC/bypass selection and restart behavior supported by the engine contract while keeping device auto-follow, settings persistence, startup-at-login and WebView UI out of scope.
- [x] 6.5 Add controller-level tests that confirm CLI and tray surfaces cannot supply AEC tuning parameters, silently select fallback devices or represent an AEC failure as bypass.

## 7. Automated verification

- [x] 7.1 Run `cargo fmt --all -- --check` and fix only formatting introduced by this change.
- [x] 7.2 Run targeted engine, lab and tray tests through `.tools\cargo-webrtc.cmd` and verify all synthetic synchronization, AEC, failure and bypass-regression cases pass without an installed driver.
- [x] 7.3 Run `.tools\cargo-webrtc.cmd test --workspace` in the documented x64 Visual Studio environment and record the successful command result.
- [x] 7.4 Run `.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings` and resolve all warnings introduced by this change.
- [x] 7.5 Inspect runtime tests and metadata fixtures to confirm they contain no private PCM, meeting content, machine-specific endpoint IDs or unauthorized system mutations.

## 8. Documentation

- [x] 8.1 Update `README.md`, `docs/technical-plan.md` and `docs/aec-baseline.md` with the implemented M3 states, commands, synchronizer bounds, known elevated-path limitation and distinction from M4/M5.
- [x] 8.2 Document the real-time `EchoCanceller` adapter configuration and confirm `vendor/UPSTREAM.md` remains unchanged unless an unexpected build-layer patch is explicitly reviewed and recorded.
- [x] 8.3 Document headless AEC validation prerequisites, metadata fields, acoustic scenario procedure, failure interpretation and the fact that ordinary commands do not install, update or remove a driver.
- [x] 8.4 Review all active documentation and OpenSpec artifacts for stale claims that M1/M2 are pending, offline WAV output is product acceptance or real-time AEC includes tuning, drift correction or production distribution.

## 9. Separately approved Windows and acoustic acceptance

- [x] 9.1 Prepare a read-only machine inventory, exact build/install/restart/record/uninstall plan and rollback targets using the existing driver lifecycle documentation, then obtain explicit user approval before any system-changing command.
- [x] 9.2 After approval, build and install only the recorded development validation package, run the headless AEC engine with explicit physical microphone and render IDs and verify the driver and engine begin fresh sessions with accounted diagnostics.
- [x] 9.3 Record and assess far-end-only, near-end-only, double-talk and render-silence scenarios through `MiniAEC Microphone`, keeping all private recordings in ignored local paths, recording only reviewable metadata summaries and documenting any default-baseline quality miss for separately approved future work.
- [x] 9.4 Verify Windows Recorder and at least one target meeting application consume the real-time AEC output for the documented duration without unexplained gaps, stale segments, raw fallback or an extra producer-facing public endpoint.
- [x] 9.5 Execute the approved stop/start, input restart, AEC recovery and sender-contention cases and account for every discontinuity, silent reference, stale discard, reset, underrun, overflow, rejected write and terminal failure.
- [x] 9.6 Execute the approved rollback, verify the recorded validation device/package/certificate and default-role baseline are restored or report the exact pending difference, and do not mark acceptance complete while a rollback discrepancy remains.
- [x] 9.7 Update the M3 acceptance documentation with metadata-only results, known limitations and the explicit statement that normal-user access, production signing, installer architecture and long-run drift correction remain unvalidated.
