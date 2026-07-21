## Context

The completed M1 change proved that a test-signed SysVAD-derived driver can expose one selectable `MiniAEC Microphone`, accept fixed 10 ms PCM16 frames through the private `VirtualMicrophoneSink` adapter, survive sender and driver restart, and roll back cleanly. The remaining user-space paths are not product-real-time: `mini-aec-lab` owns command-line orchestration, allocates packet vectors, writes WAV and JSON files, and stops after a fixed capture duration, while the Tauri host has only disabled tray items.

M2 must establish a reusable real-time engine without introducing render-loopback synchronization or AEC3. The validation driver still requires an elevated producer and test signing, so this change uses a headless elevated validation command for end-to-end evidence and deliberately defers normal-user tray integration and production driver identity. Automated tests must use fake sources and sinks and must not mutate BCD, certificate stores, drivers, devices or default audio policy.

## Goals / Non-Goals

**Goals:**

- Create a Tauri-independent `mini-aec-engine` library with project-owned configuration, command, state, snapshot, audio-source and virtual-sink boundaries.
- Capture an explicitly selected physical microphone using event-driven shared-mode WASAPI, normalize it to finite 48 kHz mono `f32`, assemble exact 480-sample frames, convert to PCM16 and send them to `VirtualMicrophoneSink`.
- Keep the hot path bounded and observable, with no file or console I/O, unbounded queue, runtime wait or stale-frame replay.
- Define deterministic start, stop, source failure, sink failure and explicit restart behavior.
- Add a headless validation command that records metadata and counters while private Windows Recorder evidence remains ignored.

**Non-Goals:**

- Render loopback, dual-clock QPC alignment, drift compensation, WebRTC AEC3 and any AEC tuning.
- Tauri menu integration, automatic startup, settings UI or background service packaging.
- Driver protocol, endpoint, DACL, INF, signing, installation or production-distribution changes.
- Automatic device fallback or silently sending raw microphone audio as a future AEC failure policy.

## Decisions

### Engine library owns contracts; Windows details stay in an adapter module

`mini-aec-engine` will expose `EngineConfig`, `EngineCommand`, `EngineState`, `EngineSnapshot` and project-owned `AudioInput`/`AudioInputFactory` boundaries. A Windows-only module will implement those boundaries with WASAPI. The engine will depend on `mini-aec-transport` but not on Tauri, CLI types, WDK types or WebRTC types. `mini-aec-lab` may call public engine APIs for the headless bypass command, but the engine will not depend on lab file writers.

This keeps the future Tauri host replaceable and makes lifecycle, framing and failure tests platform-independent. Reusing the current lab capture function directly was rejected because its fixed-duration threads, per-packet `Vec` allocation, synchronous channel and file-writing receiver encode diagnostic rather than product behavior. Creating a second Windows-audio crate is deferred until render-loopback work demonstrates another consumer that justifies that extra package boundary.

### Require an explicit physical endpoint ID

`EngineConfig` will require a capture endpoint ID; it will not silently open the current Windows default because installation can make `MiniAEC Microphone` the default input. Before opening WASAPI, the adapter will resolve the ID, retain its friendly name for diagnostics, and reject the public MiniAEC endpoint. A missing, inactive or ambiguous source is a start failure rather than a fallback to another microphone.

Endpoint names are display metadata, not selection identities. The headless command may enumerate names and IDs for the operator, but persisted or command input uses the ID. Automatic default-device following and device switching are deferred to a later tray/product lifecycle change.

### Request one normalized WASAPI stream, then enforce the project format

The Windows adapter will use event-driven shared-mode capture with audio-engine conversion enabled and request 48 kHz mono `f32`. It will propagate packet frame count, silence, discontinuity, timestamp-error, device position and QPC timestamp metadata. The engine will replace non-finite samples with zero, clamp finite samples to `[-1.0, 1.0]`, maintain a preallocated sample accumulator across arbitrary packet boundaries, and emit exact 480-sample frames.

PCM16 conversion will be deterministic and covered by boundary-value tests. Silent WASAPI packets become fresh zero samples. A discontinuity increments diagnostics and resets only the partial frame accumulator so samples from opposite sides of a discontinuity are never combined into one output frame.

Relying on the Windows Audio Engine for shared-mode sample-rate and channel conversion is acceptable for this bypass milestone and matches the existing lab baseline. A project-owned asynchronous resampler is deferred until the dual-clock AEC path produces drift evidence.

### Separate capture and sink work with a four-frame latest-wins queue

The WASAPI capture thread will normalize and frame packets, then publish complete frames into a preallocated synchronized ring with capacity four frames, or 40 ms. A dedicated sink thread owns one `VirtualMicrophoneSink` session and submits frames in order. When the engine queue is full, it discards the oldest unread complete frame, increments overflow and discarded-frame counters, and preserves the newest live audio. The sink sequence is assigned when a frame is dequeued, so a local discard cannot create a protocol sequence gap.

Four frames bound user-space queueing while allowing short scheduling jitter; the driver already has its independently validated ten-frame ring. Blocking the capture callback was rejected because it can cause WASAPI discontinuities, and dropping the newest frame was rejected because it converts temporary backpressure into increasing audible latency.

The hot path will use preallocated packet, accumulator and queue storage. It will not write files, print, wait on Tauri, use an async runtime or send through an unbounded channel. Snapshot counters may use atomics or short non-audio locks whose bounded use is demonstrated by tests.

### Use explicit state transitions and terminal failure until restart

The initial state machine is `Stopped → Starting → RunningBypass → Stopping → Stopped`, with `Starting` or `RunningBypass` able to enter `Failed`. Start opens the physical source and private sink session before publishing `RunningBypass`; stop first prevents new source frames, then closes and flushes the engine queue and sink session. Every new start creates a new sink session, resets protocol sequence to zero and clears partial or queued PCM.

Physical-device invalidation, unrecoverable WASAPI errors, driver absence, access denial, sender contention, protocol mismatch or rejected writes are terminal for the current run. The engine records a project-owned error category, closes the surviving side, clears buffered audio and enters `Failed`. Recovery is an explicit stop/restart command with the same or a new configuration. Automatic retry and automatic source replacement were rejected for M2 because they can hide device-policy changes and complicate stale-audio guarantees.

### Report bounded metadata, never PCM

`EngineSnapshot` will report state, configured endpoint ID and display name, run/session identity, captured packets and samples, silent packets, discontinuities, timestamp errors, normalized/output frames, current and high-water queue depth, local overflows/discards, accepted sink frames, sink failures, last source positions/timestamps and the last project-owned error. Snapshots and JSONL validation events contain no PCM.

The headless `mini-aec-lab bypass` command will require an explicit microphone endpoint ID and duration, connect the existing Windows virtual sink, periodically emit snapshots below an ignored evidence directory, and exit nonzero on engine failure. It will refuse to run if the selected source resolves to `MiniAEC Microphone`. End-to-end driver installation or test-mode changes remain separate lifecycle actions requiring explicit approval.

## Risks / Trade-offs

- [The validation control interface requires Administrator or SYSTEM, so the tray cannot consume it as an ordinary user product path] → Keep M2 validation headless and elevated, record this as a development-only limitation, and defer production identity/DACL/service architecture to the installation change.
- [Windows shared-mode conversion can hide device-native format details and is not the eventual dual-clock drift solution] → Record native and requested formats plus packet/QPC diagnostics, and defer a project-owned resampler until M3/M4 evidence requires it.
- [A four-frame queue plus the ten-frame driver ring permits bounded but nonzero latency] → Record queue high-water marks and driver diagnostics during acceptance; fail the gate if latency grows or overflow is unexplained.
- [Oldest-frame discard preserves freshness but creates a discontinuity under sustained overload] → Count every overflow/discard, expose it in snapshots, and require zero unexplained loss in the continuous bypass acceptance run.
- [Stopping two native threads can race with device invalidation or a blocked control request] → Use cancellation signaling and finite wait timeouts, make close idempotent, and add deterministic race-oriented fake-adapter tests.
- [Physical microphone recordings are private and cannot be committed as fixtures] → Use synthetic fake-source frames for automated tests and keep Windows Recorder files ignored; commit only metadata-only evidence summaries.

## Migration Plan

1. Add the engine contracts, state machine, deterministic framing/conversion logic and fake-adapter tests without Windows or driver access.
2. Add the Windows WASAPI input adapter and refactor only reusable device-enumeration behavior from the lab while preserving existing lab commands.
3. Add the bounded capture-to-sink runtime and headless bypass command, then pass workspace format, test and lint gates without an installed driver.
4. Update top-level documentation to mark M1 complete and describe the M2 development-only elevated validation boundary.
5. After separately reviewing test-mode, certificate, install and rollback commands, run an approved end-to-end validation through `MiniAEC Microphone`, save ignored evidence and fully roll back the driver lifecycle.

Rollback of source changes is an ordinary Git revert because no data migration or persistent application state is introduced. Machine rollback follows the existing `driver-development-lifecycle` spec and must remove only the recorded validation device, package and certificate and restore the saved boot/default-device baseline. This change does not authorize those operations.

## Open Questions

- The production mechanism that lets a normal-user tray process submit PCM to the signed driver remains intentionally unresolved; M2 evidence should inform whether the later product uses a revised device ACL, a privileged service or another approved boundary.
- Automatic physical-device following and tray-visible device selection remain deferred until the explicit-ID bypass path is stable.
