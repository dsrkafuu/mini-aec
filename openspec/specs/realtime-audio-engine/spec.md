# Realtime audio engine Specification

## Purpose

Define the independent, bounded and observable real-time engine that captures explicit physical input roles and supplies fresh normalized or echo-cancelled PCM through a selected, paired VB-CABLE output route.

## Requirements

### Requirement: Independent real-time engine lifecycle
The system SHALL provide a Tauri-independent real-time engine that accepts project-owned configuration and lifecycle commands, reports project-owned state and can start, stop and explicitly restart either one physical-microphone bypass run or one dual-input AEC-enabled run with an explicit VB-CABLE output pair without depending on a CLI, WebView or async UI runtime.

#### Scenario: AEC-enabled engine starts successfully
- **WHEN** a stopped engine receives a valid AEC configuration with available explicit physical microphone, physical render-loopback and paired VB-CABLE output endpoints
- **THEN** it transitions through Starting to RunningAec
- **THEN** it reports both configured source identities, both VB-CABLE endpoint identities and a new run, synchronization epoch, AEC instance and output-session identity

#### Scenario: Bypass engine starts successfully
- **WHEN** a stopped engine receives a valid explicit bypass configuration with an available physical microphone and paired VB-CABLE output endpoints
- **THEN** it transitions through Starting to RunningBypass without opening render loopback or constructing an echo canceller

#### Scenario: Engine stops normally
- **WHEN** a running or degraded engine receives a stop command
- **THEN** it stops accepting both source roles, closes source, AEC and VB-CABLE render resources, clears partial, queued, converted and processed PCM and transitions to Stopped

#### Scenario: Engine restarts explicitly
- **WHEN** a stopped or failed engine receives a new start command after its prior run has been closed
- **THEN** it creates distinct run, synchronization, AEC when applicable and output-session identities and exposes no partial, queued, converted or processed PCM from the prior run

### Requirement: Explicit physical source isolation
The engine SHALL require exact Windows endpoint IDs for every configured physical input role, SHALL resolve and report display metadata before capture, SHALL reject the paired VB-CABLE recording endpoint as its physical microphone source, SHALL reject the paired VB-CABLE playback endpoint as its physical render-loopback source and SHALL NOT silently fall back to Windows defaults or other endpoints.

#### Scenario: Explicit physical microphone and render are selected
- **WHEN** an AEC configuration resolves to one active physical capture endpoint, one distinct active physical render endpoint and one distinct supported VB-CABLE pair
- **THEN** the engine opens exactly the physical endpoints in their configured capture and render-loopback roles and reports all source and output identities

#### Scenario: Explicit bypass source is selected
- **WHEN** a bypass configuration resolves to an active physical capture endpoint distinct from the paired VB-CABLE recording endpoint
- **THEN** the engine opens exactly that capture endpoint without requiring or opening a physical render endpoint

#### Scenario: MiniAEC is selected recursively
- **WHEN** the configured physical microphone resolves to the paired `CABLE Output` recording endpoint
- **THEN** start fails with an actionable invalid-source error before opening render, AEC or output resources

#### Scenario: VB-CABLE playback endpoint is selected as the AEC reference
- **WHEN** the configured physical render endpoint resolves to the paired `CABLE Input` playback endpoint
- **THEN** start fails with an actionable post-AEC-loop error before opening microphone, loopback, AEC or output resources

#### Scenario: Configured source is unavailable
- **WHEN** any endpoint required by the selected mode is missing, inactive or has the wrong data-flow role
- **THEN** start fails without opening another endpoint or changing Windows default-device policy

### Requirement: Fixed-format normalization and framing
The engine SHALL transform event-driven physical microphone packets into finite 48 kHz mono samples, assemble exact 10 ms frames of 480 samples across arbitrary packet boundaries and submit only complete frames to the output boundary; endpoint-specific channel, sample and mix-format adaptation SHALL remain outside the capture and AEC frame contract.

#### Scenario: Packets cross frame boundaries
- **WHEN** the source supplies valid packets whose frame counts do not align to 480-sample boundaries
- **THEN** the engine preserves sample order across packet boundaries and emits each complete 480-sample frame exactly once

#### Scenario: Source packet is silent
- **WHEN** WASAPI marks a source packet silent
- **THEN** the engine contributes fresh zero-valued samples for that packet duration without reusing earlier microphone PCM

#### Scenario: Source contains non-finite or out-of-range samples
- **WHEN** normalization receives NaN, infinity or a finite sample outside `[-1.0, 1.0]`
- **THEN** it replaces non-finite values with zero, clamps finite values to the supported range and records the sanitization without submitting invalid samples

#### Scenario: Source discontinuity is reported
- **WHEN** the source reports a data discontinuity or timestamp error while a partial frame is buffered
- **THEN** the engine records the condition and clears the partial frame so one output frame never combines samples from opposite sides of the discontinuity

### Requirement: Bounded real-time data path
The engine SHALL use preallocated bounded storage on its capture-to-sink path, SHALL keep at most four complete unread frames in its user-space queue and SHALL NOT perform file I/O, console I/O, unbounded queueing or UI-runtime waits on real-time threads.

#### Scenario: Sink keeps pace
- **WHEN** complete frames arrive while fewer than four unread frames are queued
- **THEN** the engine preserves their order and submits them without an engine-local discard

#### Scenario: Engine queue overflows
- **WHEN** a complete frame arrives while four unread frames are queued
- **THEN** the engine discards the oldest unread complete frame, retains the newest frame and increments local overflow and discarded-frame diagnostics

#### Scenario: Backpressure subsides
- **WHEN** the sink resumes consuming after one or more local overflow events
- **THEN** submitted protocol sequences remain monotonic and gap-free because sequence numbers are assigned at dequeue time

### Requirement: Explicit bypass output
While RunningBypass, the engine SHALL send normalized physical microphone frames directly through one VB-CABLE output session, SHALL identify the state as bypass and SHALL NOT invoke WebRTC AEC3 or represent bypass as an AEC failure fallback.

#### Scenario: Bypass is running
- **WHEN** the source and VB-CABLE output are healthy and the engine is RunningBypass
- **THEN** physical microphone frames are available to ordinary clients from the paired `CABLE Output` endpoint in capture-clock order
- **THEN** snapshots explicitly report RunningBypass rather than an AEC-enabled state

#### Scenario: Sink session begins
- **WHEN** a new bypass run opens its selected VB-CABLE playback endpoint
- **THEN** it creates a fresh output session with empty conversion and render queues

### Requirement: Failure isolation and stale-audio prevention
The engine SHALL treat physical-device invalidation, unrecoverable WASAPI failure, missing or ambiguous VB-CABLE endpoints, output access failure and rejected output writes as terminal failures for the current run, SHALL clear all partial, queued, converted and processed PCM and SHALL require an explicit restart rather than silently changing source or output or continuing raw output through another path.

#### Scenario: Physical source fails while running
- **WHEN** the selected microphone is invalidated or its WASAPI stream fails
- **THEN** the engine closes the output session, clears buffered PCM, enters Failed and reports the source error without selecting another microphone

#### Scenario: Virtual sink fails while running
- **WHEN** the selected VB-CABLE playback endpoint disappears or its render stream fails
- **THEN** the engine stops capture, closes surviving resources, clears buffered PCM, enters Failed and reports the mapped output error

#### Scenario: Restart follows a failure
- **WHEN** the operator explicitly restarts after a failed run and all configured source and paired output endpoints are available
- **THEN** capture resumes in a new run and output session without submitting PCM from before the failure

### Requirement: Metadata-only engine diagnostics
The engine SHALL expose bounded snapshots and validation events containing lifecycle, mode, both source roles when applicable, both VB-CABLE endpoint identities, framing, synchronization, AEC, queue, conversion and output counters needed to explain continuity and recovery, and SHALL NOT include PCM or meeting content in logs.

#### Scenario: Bypass snapshot is requested
- **WHEN** a controller requests a snapshot for a bypass run
- **THEN** it reports bypass state, microphone identity, VB-CABLE pair identities, run and output-session identity, capture and silence counts, discontinuities, timestamp errors, normalized frames, queue depth and high-water mark, local overflows and discards, rendered output frames, conversion counts, output failures and the last project-owned error without claiming render-reference or AEC activity

#### Scenario: AEC snapshot is requested
- **WHEN** a controller requests a snapshot for an AEC-enabled run
- **THEN** it includes the bypass-era fields plus physical render identity and counters, synchronization and skew evidence, AEC processing and recovery evidence, current degradation reason and AEC-specific state

#### Scenario: Diagnostics are persisted
- **WHEN** the headless validation command writes periodic events or a final result
- **THEN** it writes metadata only to an ignored evidence path outside real-time workers and does not write PCM or meeting content

### Requirement: End-to-end bypass acceptance
The project SHALL provide a headless validation path that runs the engine with an explicit physical microphone, renders to an already installed supported VB-CABLE playback endpoint and verifies the resulting stream through its paired recording endpoint without downloading, installing, updating or removing VB-CABLE or changing driver, certificate, boot, device or default-role state.

#### Scenario: Continuous bypass run succeeds
- **WHEN** Windows Recorder captures the paired `CABLE Output` endpoint for at least five minutes while the engine bypasses an active physical microphone to `CABLE Input`
- **THEN** the recording has the expected duration with no stale segment or unexplained gap
- **THEN** engine and output diagnostics account for every discontinuity, underrun, overflow, discard, conversion or output failure

#### Scenario: Source and engine restart are validated
- **WHEN** the approved validation sequence stops the engine and starts a new bypass run while the recording client remains open or is reopened as documented
- **THEN** capture resumes from a fresh output session without replaying pre-stop microphone PCM

#### Scenario: Validation lacks driver authorization
- **WHEN** automated tests or an unapproved validation run execute without a supported installed VB-CABLE pair
- **THEN** they use fake output sinks or stop with an actionable prerequisite result and do not download software or change certificates, drivers, devices, boot configuration or Windows default audio roles
