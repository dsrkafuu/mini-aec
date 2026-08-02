## ADDED Requirements

### Requirement: Explicit physical render-loopback reference
The system SHALL require an exact active physical render endpoint identity for every AEC-enabled run, SHALL capture the Windows render-loopback stream for exactly that endpoint and SHALL NOT silently follow the Windows default render device or substitute another endpoint.

#### Scenario: Explicit render endpoint is opened
- **WHEN** an AEC-enabled run starts with an active physical render endpoint ID
- **THEN** the engine opens loopback capture for exactly that endpoint and reports its identity and display metadata

#### Scenario: Configured render endpoint is unavailable
- **WHEN** the configured render endpoint is missing, inactive or cannot provide loopback capture
- **THEN** the AEC-enabled start fails without opening another render endpoint or publishing a healthy running state

#### Scenario: Render endpoint changes while running
- **WHEN** the selected render endpoint is invalidated or its loopback stream fails during an AEC-enabled run
- **THEN** the current run stops, clears unpaired and processed PCM, closes the virtual microphone session and reports an actionable render-source failure until an explicit restart

### Requirement: Bounded QPC-based dual-input synchronization
The system SHALL derive 10 ms microphone and render frame timing from WASAPI QPC and device-position metadata, SHALL pair both roles on a bounded common 48 kHz timeline and SHALL bound waiting, buffering, stale-frame discard and silent-reference insertion with observable counters.

#### Scenario: Both streams establish a common timeline
- **WHEN** microphone and render inputs provide valid initial packets within the finite startup window
- **THEN** the synchronizer selects a common origin, accounts for frames before that origin and emits ordered microphone/render frame pairs without combining samples from non-overlapping discontinuity epochs

#### Scenario: Render frame is temporarily unavailable
- **WHEN** a microphone interval is ready but no matching render interval arrives within the bounded tolerance
- **THEN** the engine supplies a fresh silent render reference for that interval, marks AEC degraded and increments the render-underrun and silent-reference diagnostics without blocking microphone cadence

#### Scenario: Render frame is stale
- **WHEN** one or more queued render frames fall before the next eligible microphone interval
- **THEN** the synchronizer discards the stale complete frames, increments discard diagnostics and does not increase product latency to replay them

#### Scenario: Input discontinuity occurs
- **WHEN** either input reports a discontinuity or timestamp error across a partially framed or queued interval
- **THEN** the engine clears affected partial and unpaired frames, starts a new synchronization epoch, resets AEC state and never combines samples from opposite sides of the discontinuity

#### Scenario: Sustained skew exceeds the M3 bound
- **WHEN** microphone and render timing cannot be paired within the documented finite skew threshold after bounded recovery
- **THEN** the run stops with an actionable synchronization failure rather than growing a queue, repeatedly dropping whole frames indefinitely or claiming long-run drift correction

### Requirement: Replaceable frozen-default echo canceller
The system SHALL process aligned frames through a project-owned `EchoCanceller` boundary whose initial adapter uses `webrtc-audio-processing 2.1.0`, the FreeDesktop WebRTC M131 snapshot, full echo cancellation and upstream-default AEC3 parameters with noise suppression, gain control and product post-processing disabled.

#### Scenario: Aligned pair is processed
- **WHEN** an AEC-enabled run receives one aligned render frame and microphone frame
- **THEN** the engine submits the render frame before the microphone frame, emits only the processed finite microphone result and increments AEC processing diagnostics

#### Scenario: Adapter is constructed
- **WHEN** a new AEC-enabled run starts or an approved recovery reconstructs the adapter
- **THEN** it creates a fresh default M131 full-echo-canceller instance without accepting product tuning parameters from engine, CLI or tray configuration

#### Scenario: Explicit bypass is selected
- **WHEN** the operator starts the engine in bypass mode
- **THEN** the engine does not construct or invoke the echo-canceller adapter and reports `RunningBypass` visibly

### Requirement: Safe AEC degradation and recovery
The system SHALL distinguish healthy AEC, explicit bypass, temporary AEC degradation and terminal failure, SHALL silence an output frame whose AEC result is invalid and SHALL NOT silently send the pre-AEC microphone frame because render, synchronization or AEC processing failed.

#### Scenario: Render reference is temporarily missing
- **WHEN** a bounded render-reference shortage occurs during an otherwise valid AEC run
- **THEN** the engine enters or remains in a visible degraded AEC state, processes the interval with the defined silent-reference behavior and returns to healthy AEC only after the recovery gate is satisfied

#### Scenario: AEC returns an error or non-finite output
- **WHEN** the adapter fails to process a frame or produces a non-finite sample
- **THEN** the engine sends fresh silence for that output interval, records the failure and resets or reconstructs the adapter without sending the raw microphone frame

#### Scenario: AEC reconstruction succeeds
- **WHEN** the adapter is reconstructed within the bounded recovery policy and subsequent aligned processing succeeds
- **THEN** the engine resumes `RunningAec` with a fresh AEC state and no queued PCM from before the reset

#### Scenario: AEC recovery fails
- **WHEN** the adapter cannot be reconstructed or repeated failures exceed the documented recovery bound
- **THEN** the engine closes the sink session, clears buffered PCM, enters `Failed` and requires an explicit restart

### Requirement: Bounded real-time AEC path
The system SHALL keep microphone capture, render capture, synchronization, AEC processing and virtual microphone submission on bounded storage and finite waits, SHALL avoid file and console I/O or UI-runtime waits on real-time workers and SHALL expose processing deadline and queue-pressure evidence.

#### Scenario: Real-time processing keeps pace
- **WHEN** both inputs and the sink remain healthy during an AEC-enabled run
- **THEN** every eligible 10 ms interval is processed and submitted in order without unbounded allocation, growing queue depth or unexplained frame loss

#### Scenario: Processing misses its budget
- **WHEN** AEC processing or sink submission causes a bounded queue to overflow or a processing interval to exceed the documented real-time budget
- **THEN** the engine preserves freshest-audio behavior, records the overflow or deadline miss and enters the specified degraded or failed state instead of accumulating latency

#### Scenario: Validation evidence is written
- **WHEN** the headless controller persists periodic or final AEC evidence
- **THEN** file I/O occurs outside real-time workers and contains metadata rather than PCM or meeting content

### Requirement: AEC-specific metadata diagnostics
The system SHALL expose bounded metadata-only diagnostics sufficient to explain render capture, synchronization, AEC health, processing timing, degradation and recovery without including PCM or meeting content.

#### Scenario: AEC snapshot is requested
- **WHEN** a controller requests a snapshot during or after an AEC-enabled run
- **THEN** it reports both endpoint identities, per-role packet/frame/silence/discontinuity/timestamp counters, synchronization origin and delta, paired frames, silent references, stale discards, alignment resets, maximum observed skew, AEC processed frames, resets or rebuilds, invalid outputs, processing-time summaries, current mode, degradation reason and the existing sink diagnostics

#### Scenario: Tray reads engine status
- **WHEN** the windowless tray host refreshes its low-frequency status
- **THEN** it receives only project-owned state and metadata and can distinguish stopped, starting, running AEC, degraded AEC, explicit bypass and failed without accessing PCM or WebRTC types

### Requirement: End-to-end default AEC acceptance
The project SHALL validate the frozen default M131 AEC3 path through `MiniAEC Microphone` with ordinary capture clients and an acoustic scenario matrix, while automated tests SHALL NOT install a driver or mutate boot, certificate, device or default-role state.

#### Scenario: Far-end-only playback is validated
- **WHEN** selected-speaker playback is captured as the render reference and returns acoustically through the selected physical microphone
- **THEN** `MiniAEC Microphone` contains no intelligible returned far-end speech after convergence and the metadata accounts for alignment, reset, underrun and processing behavior

#### Scenario: Near-end-only speech is validated
- **WHEN** the local speaker is silent and near-end speech enters the selected physical microphone
- **THEN** `MiniAEC Microphone` preserves understandable natural speech with intact starts and endings and no unexplained level pumping or persistent coloration

#### Scenario: Double-talk is validated
- **WHEN** near-end speech overlaps active selected-speaker playback before and after AEC convergence
- **THEN** the near-end remains understandable without obvious swallowing or pumping while far-end removal remains effective

#### Scenario: Render silence and restart are validated
- **WHEN** playback becomes silent or either selected input is explicitly restarted according to the approved validation sequence
- **THEN** the engine exposes bounded degraded silence or clean recovery as specified, starts a fresh synchronization and AEC epoch and never replays stale PCM

#### Scenario: Public endpoint is consumed
- **WHEN** Windows Recorder and at least one target meeting application capture `MiniAEC Microphone` during the approved AEC validation
- **THEN** they receive the processed stream for the expected duration without unexplained gaps, stale segments or an additional producer-facing public audio endpoint

#### Scenario: Validation lacks system authorization
- **WHEN** ordinary automated tests or an unapproved machine run execute without the authorized development driver lifecycle
- **THEN** they use fake inputs, fake echo cancellers and fake sinks or stop with an actionable unavailable/access-denied result without changing Windows system state
