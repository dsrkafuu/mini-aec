## Context

M1 established a project-owned virtual microphone transport with a fixed 48 kHz mono PCM16 protocol, bounded driver ring, session isolation and development-safe lifecycle. M2 added `mini-aec-engine`, one event-driven physical microphone capture worker, normalization and 10 ms framing, a four-frame latest-wins queue, a dedicated virtual-sink worker, explicit bypass state, metadata-only snapshots and an elevated headless acceptance path.

The retained `mini-aec-lab` path can already capture a physical microphone and render loopback together, align recorded tracks from QPC timestamps and process them offline with the frozen upstream-default WebRTC M131 AEC3 baseline. That code proves device access and algorithm viability but is not suitable for the product hot path because it uses fixed-duration capture, file-backed orchestration and offline allocation patterns.

M3 must turn those proven pieces into a bounded real-time product path without changing the AEC algorithm, virtual microphone protocol or driver lifecycle. The current validation driver grants producer access only to Administrators and SYSTEM, so machine acceptance remains an elevated development activity; this change does not choose the normal-user production architecture.

## Goals / Non-Goals

**Goals:**

- Add explicit, role-specific physical microphone and physical render endpoint configuration for AEC mode while retaining an explicit bypass mode.
- Capture both WASAPI streams on native worker threads, derive 48 kHz mono 10 ms frame timestamps from packet QPC metadata and pair them on a bounded common timeline.
- Run the frozen `webrtc-audio-processing 2.1.0` full echo canceller through a project-owned `EchoCanceller` interface and keep WebRTC types inside one adapter.
- Keep capture, synchronization, processing and sink submission bounded, preallocated where required and independent from Tauri or an async runtime.
- Surface healthy AEC, explicit bypass, degraded AEC, terminal failure and recovery through project-owned states, snapshots, the headless harness and the existing windowless tray.
- Validate the default algorithm again on the actual `MiniAEC Microphone` path with far-end-only, near-end-only, double-talk, render-silence and restart scenarios.

**Non-Goals:**

- AEC3 tuning, experimental configuration, dependency upgrades, side-by-side algorithm selection or product-facing profiles.
- Asynchronous resampling, a claim of long-run drift closure or the M4 thirty-minute and two-hour stability gates.
- Automatic default-device following, fallback endpoint selection, hot device switching without an explicit restart or persistence of device choices.
- Driver protocol, endpoint, DACL, signing, installation, upgrade or uninstall changes.
- Noise suppression, gain control, EQ, dereverberation, voice enhancement, WebView UI or PCM handling in Tauri.

## Decisions

### Model microphone and render loopback as explicit input roles

`EngineConfig` will carry a processing mode, an exact physical capture endpoint ID and, for AEC mode, an exact physical render endpoint ID. Bypass mode requires only the capture endpoint and remains visibly distinct. The engine resolves both descriptors before publishing a running state, rejects the public `MiniAEC Microphone` as a physical capture source and never substitutes a Windows default or another endpoint.

The project-owned input contracts will distinguish capture and render-loopback roles without exposing WASAPI types. Each Windows adapter instance will create, use and destroy its COM and WASAPI objects on its own worker thread. Reusing the lab capture loop directly was rejected because its file-oriented lifetime and packet ownership do not satisfy the real-time contract; adding a cross-platform audio abstraction was rejected because it would hide the Windows loopback and timestamp semantics required here.

### Use three real-time workers and role-specific bounded queues

One microphone worker and one render-loopback worker will normalize packets and publish timestamped 480-sample `f32` frames into separate preallocated bounded queues. One processing worker will own the synchronizer, `EchoCanceller` instance and `VirtualMicrophoneSink` session so render submission, capture processing and sink sequencing remain single-threaded and deterministic.

Capture drives product output cadence. The processing worker advances a 10 ms timeline from the later of the first usable microphone and render frame starts, discards and counts frames that precede that origin, and pairs each capture interval with the render interval selected from QPC-derived frame-start timestamps. It drains stale render frames, treats a temporarily missing render interval as an explicit silent reference, records the shortage and never blocks microphone output waiting indefinitely for render. Queue capacities, timestamp tolerance and sustained-skew failure thresholds will be named constants covered by deterministic synthetic tests and documented with the implementation; they must bound memory and added waiting rather than growing to conceal drift.

Starting a run waits only for a finite initialization window. If both roles cannot establish a usable timeline, start fails without publishing `RunningAec`. A discontinuity clears only affected partial framing and unpaired queued frames, resets the synchronizer and echo canceller, and prevents samples from opposite sides of the discontinuity from being combined. Whole-frame discard or silent-reference insertion is a counted recovery action, not the M4 drift-control solution.

Using arrival order alone was rejected because the devices have independent clocks and scheduling jitter. Resampling immediately was rejected because M3 does not yet have sustained-drift evidence. An unbounded timestamp reorder buffer was rejected because it converts device failure into growing latency and memory.

### Put WebRTC behind a frame-oriented project boundary

The engine will define an `EchoCanceller` contract that accepts one normalized render frame before one normalized capture frame, returns a normalized capture result, exposes reset and reports project-owned processing diagnostics and errors. A factory creates one instance per AEC run on the processing worker. The WebRTC adapter will construct `Processor::new(48_000)`, enable only `EchoCanceller::Full`, leave AEC3 at upstream defaults and keep NS, AGC and product post-processing disabled.

WebRTC configuration and channel buffers stay inside the adapter. The adapter will reuse preallocated channel storage for steady-state frames and will validate finite output before it reaches PCM16 conversion. Calling the existing offline function from the runtime was rejected because it owns file-scale vectors and report generation. Exposing WebRTC configuration through `EngineConfig` was rejected because it would reintroduce unsupported product tuning and couple the engine contract to the frozen dependency.

### Separate explicit bypass, healthy AEC and degraded AEC states

The state machine extends to `Stopped → Starting → RunningAec|RunningBypass → Stopping → Stopped`, with `RunningAec` able to enter `Degraded` or `Failed`. `Degraded` is AEC mode with an observable temporary impairment such as a missing render reference or an adapter reset; it is never represented as bypass. A healthy aligned frame after the defined recovery gate returns the run to `RunningAec`.

A short render-reference shortage supplies a silent reference, increments diagnostics and marks the run degraded without blocking capture. An AEC processing error or non-finite output silences the current output frame, resets or reconstructs the adapter outside capture callbacks and records the event. Failure to reconstruct, sustained timeline skew, role invalidation, unrecoverable WASAPI failure or sink failure terminates the run, clears all partial and queued PCM, closes the sink session and requires explicit restart. No AEC fault path sends the pre-AEC microphone frame directly to the sink.

Automatically falling back to `RunningBypass` was rejected because it can expose uncancelled far-end speech without user consent. Terminating on the first temporary render underrun was rejected because normal scheduler jitter should have bounded, visible recovery. Continuing indefinitely through repeated AEC resets was rejected because it would hide a broken product path.

### Extend metadata-only observability without recording content

`EngineSnapshot` will retain the existing source, framing, queue and sink fields and add render descriptor and packet counters, per-role discontinuities and timestamps, synchronizer origin and current delta, paired frames, silent references, stale discards, alignment resets, maximum observed absolute skew, AEC processed frames, reset/rebuild count, non-finite output count, processing-time P50/P95/P99 or equivalent bounded histogram summaries, current mode and degradation reason. Snapshot and tray updates remain low-frequency copies and contain no PCM.

The processing worker will update preallocated counters or bounded summaries only. JSONL evidence remains the responsibility of the headless controller outside real-time workers and is written below ignored validation output. Optional acoustic recordings remain private under ignored local paths and require explicit operator action.

### Keep tray integration thin and acceptance headless-first

The Tauri host will construct or connect to the project-owned engine controller, present current stopped/starting/running AEC/degraded/bypass/failed status, allow only explicit AEC enable or bypass selection supported by the engine contract, restart the engine and exit cleanly. Tauri does not enumerate PCM buffers, invoke WebRTC or wait on real-time workers from an event callback. Endpoint selection can remain an explicit development configuration surface for this change; a settings frontend is not introduced.

Automated acceptance uses fake role-specific inputs, fake echo cancellers and fake sinks to cover timing, discontinuity, shortage, failure and restart deterministically without touching Windows system state. The approved machine path uses the existing installed validation driver and elevated headless harness, then records through `MiniAEC Microphone` in Windows Recorder and at least one target meeting application. Driver install, test-mode, certificate and rollback actions remain separately reviewed and are not performed by ordinary tests or the AEC command.

### Separate default-baseline functional acceptance from later algorithm optimization

M3 accepts the frozen upstream-default M131 path when its transport, lifecycle, failure isolation, client consumption, rollback and acoustic scenario execution are complete and reviewable. The scenario matrix characterizes the default algorithm rather than authorizing tuning inside this change. A quality miss such as understandable but obviously swallowed near-end speech during double-talk must remain visible in active documentation and must not be described as meeting the desired quality target, but it does not reopen verified transport or lifecycle behavior. Any attempt to improve that result requires a separately approved future change, identical-input old/new processing and preservation of the demonstrated far-end removal.

Treating every default-algorithm quality miss as permission to tune M3 was rejected because it would defeat the frozen-baseline boundary and make completion depend on an open-ended subjective search. Ignoring the miss or describing it as passing was also rejected because that would erase the evidence needed to scope later algorithm work.

## Risks / Trade-offs

- [Independent microphone and render clocks can drift beyond the M3 pairing tolerance] → Record QPC delta, queue depth, silent references, stale discards and maximum skew; enter an explicit degraded or failed state at the bounded threshold and use the evidence to design M4 asynchronous resampling.
- [A silent reference during render shortage can temporarily expose uncancelled echo] → Mark the run degraded immediately, count the exact duration, keep the shortage bounded and fail sustained loss instead of presenting it as healthy AEC or bypass.
- [WebRTC processing or reconstruction can exceed the 10 ms frame budget] → Own the adapter on one processing worker, preallocate steady-state buffers, record bounded processing-time distributions and fail acceptance on deadline misses or growing queues.
- [Shared-mode Windows conversion can obscure native device-clock behavior] → Retain native/requested format and continuous QPC/device-position diagnostics; do not claim drift closure until M4 evidence exists.
- [The driver DACL prevents an ordinary user tray process from producing audio] → Keep M3 machine acceptance elevated and development-only, avoid changing the transport contract and defer the service/DACL/installer decision to M5.
- [Acoustic acceptance depends on private room recordings and subjective listening] → Keep recordings ignored, use a fixed scenario matrix and metadata report, preserve the frozen algorithm and compare the product output consistently before proposing any tuning.
- [The frozen default algorithm can meet functional goals while missing a subjective quality target] → Record the miss explicitly, complete only the verified M3 functional scope and require a separate identical-input change before optimizing the algorithm.
- [Adding tray control before production permissions can look more complete than it is] → Label the path as development-only in status and documentation and do not describe M3 as distributable or ordinary-user ready.

## Migration Plan

1. Extend project-owned engine types, role-specific fake inputs, AEC factory/adapter contracts, states and snapshots while preserving the existing bypass behavior and tests.
2. Add deterministic synchronizer, discontinuity, render-shortage, AEC-reset and bounded-queue tests using synthetic timestamped frames.
3. Add the Windows render-loopback adapter and the frozen default M131 implementation behind the project boundaries, then pass format, workspace tests and strict Clippy without an installed driver.
4. Add the headless real-time AEC command and thin tray state/control wiring without changing driver or Windows policy.
5. Update README and active design/baseline documentation, then review the exact elevated validation and rollback plan separately.
6. Run approved end-to-end acoustic and restart acceptance through `MiniAEC Microphone`, preserve only metadata summaries in reviewable locations, document any default-baseline quality miss without tuning it in M3 and fully roll back development driver state when requested by the validation plan.

Source rollback is an ordinary Git revert to the M2 bypass implementation. A failed AEC run never migrates persistent data and can be restarted explicitly in bypass only when the operator selects bypass. Machine rollback uses the existing driver lifecycle and may remove only the recorded validation device, package and certificate after explicit approval; this change itself does not authorize those actions.

## Open Questions

- The exact normal-user producer identity remains an M5 distribution decision; M3 evidence should not be used to select a DACL change, privileged service or installer strategy implicitly.
- M3 will record the synchronizer thresholds and acoustic test durations chosen during implementation and acceptance. Any request to turn observed long-run skew into active resampling expands scope into M4 and requires a separate change.
