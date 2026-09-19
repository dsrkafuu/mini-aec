# Spec Delta

## MODIFIED Requirements

### Requirement: Explicit physical render-loopback reference
The system SHALL require an exact active physical render endpoint identity for every AEC-enabled run, SHALL capture the Windows render-loopback stream for exactly that endpoint, SHALL reject the selected VB-CABLE playback endpoint as the reference source and SHALL NOT silently follow the Windows default render device or substitute another endpoint.

#### Scenario: Explicit render endpoint is opened
- **WHEN** an AEC-enabled run starts with an active physical render endpoint ID distinct from the selected VB-CABLE playback endpoint
- **THEN** the engine opens loopback capture for exactly that physical endpoint and reports its identity and display metadata

#### Scenario: Configured render endpoint is unavailable
- **WHEN** the configured physical render endpoint is missing, inactive or cannot provide loopback capture
- **THEN** the AEC-enabled start fails without opening another render endpoint or publishing a healthy running state

#### Scenario: Render endpoint changes while running
- **WHEN** the selected physical render endpoint is invalidated or its loopback stream fails during an AEC-enabled run
- **THEN** the current run stops, clears unpaired and processed PCM, closes the VB-CABLE output session and reports an actionable render-source failure until an explicit restart

### Requirement: Safe AEC degradation and recovery
The system SHALL distinguish healthy AEC, explicit bypass, temporary AEC degradation and terminal failure, SHALL submit fresh silence when an AEC result is invalid and SHALL NOT silently send the pre-AEC microphone frame because render, synchronization or AEC processing failed.

#### Scenario: Render reference is temporarily missing
- **WHEN** a bounded render-reference shortage occurs during an otherwise valid AEC run
- **THEN** the engine enters or remains in a visible degraded AEC state, processes the interval with the defined silent-reference behavior and returns to healthy AEC only after the recovery gate is satisfied

#### Scenario: AEC returns an error or non-finite output
- **WHEN** the adapter fails to process a frame or produces a non-finite sample
- **THEN** the engine submits fresh silence for that output interval, records the failure and resets or reconstructs the adapter without sending the raw microphone frame

#### Scenario: AEC reconstruction succeeds
- **WHEN** the adapter is reconstructed within the bounded recovery policy and subsequent aligned processing succeeds
- **THEN** the engine resumes `RunningAec` with a fresh AEC state and no queued PCM from before the reset

#### Scenario: AEC recovery fails
- **WHEN** the adapter cannot be reconstructed or repeated failures exceed the documented recovery bound
- **THEN** the engine closes the VB-CABLE output session, clears buffered PCM, enters `Failed` and requires an explicit restart

### Requirement: Bounded real-time AEC path
The system SHALL keep microphone capture, render capture, synchronization, AEC processing and VB-CABLE output submission on bounded storage and finite waits, SHALL avoid file and console I/O or UI-runtime waits on real-time workers and SHALL expose processing deadline, format-conversion and queue-pressure evidence.

#### Scenario: Real-time processing keeps pace
- **WHEN** both physical inputs and the VB-CABLE output remain healthy during an AEC-enabled run
- **THEN** every eligible 10 ms interval is processed and submitted in order without unbounded allocation, growing queue depth or unexplained frame loss

#### Scenario: Processing misses its budget
- **WHEN** AEC processing, output conversion or VB-CABLE rendering causes a bounded queue to overflow or a processing interval to exceed the documented real-time budget
- **THEN** the engine preserves freshest-audio behavior, records the overflow or deadline miss and enters the specified degraded or failed state instead of accumulating latency

#### Scenario: Validation evidence is written
- **WHEN** the headless controller persists periodic or final AEC evidence
- **THEN** file I/O occurs outside real-time workers and contains metadata rather than PCM or meeting content

### Requirement: AEC-specific metadata diagnostics
The system SHALL expose bounded metadata-only diagnostics sufficient to explain physical render capture, synchronization, AEC health, processing timing, VB-CABLE output, degradation and recovery without including PCM or meeting content.

#### Scenario: AEC snapshot is requested
- **WHEN** a controller requests a snapshot during or after an AEC-enabled run
- **THEN** it reports physical microphone, physical render and paired VB-CABLE endpoint identities, per-role packet/frame/silence/discontinuity/timestamp counters, synchronization origin and delta, paired frames, silent references, stale discards, alignment resets, maximum observed skew, AEC processed frames, resets or rebuilds, invalid outputs, processing-time summaries, output format and counters, current mode, degradation reason and the last project-owned error

#### Scenario: Tray reads engine status
- **WHEN** the windowless tray host refreshes its low-frequency status
- **THEN** it receives only project-owned state and metadata and can distinguish stopped, starting, running AEC, degraded AEC, explicit bypass, missing VB-CABLE and failed without accessing PCM or WebRTC types

### Requirement: End-to-end default AEC functional acceptance and quality characterization
The project SHALL validate the frozen default M131 AEC3 path by rendering to `CABLE Input` and consuming the paired `CABLE Output` with ordinary capture clients and an acoustic scenario matrix, SHALL record any default-baseline quality shortfall without claiming that the affected quality target passed, and automated tests SHALL NOT install, update or remove VB-CABLE or mutate boot, certificate, device or default-role state. Algorithm optimization for a recorded shortfall requires a separately approved future change and identical-input old/new evidence.

#### Scenario: Far-end-only playback is validated
- **WHEN** selected-speaker playback is captured as the render reference and returns acoustically through the selected physical microphone
- **THEN** the stream captured from `CABLE Output` contains no intelligible returned far-end speech after convergence and the metadata accounts for alignment, reset, underrun, processing and output behavior

#### Scenario: Near-end-only speech is validated
- **WHEN** the local speaker is silent and near-end speech enters the selected physical microphone
- **THEN** the stream captured from `CABLE Output` preserves understandable natural speech with intact starts and endings and no unexplained level pumping or persistent coloration

#### Scenario: Double-talk is validated
- **WHEN** near-end speech overlaps active selected-speaker playback before and after AEC convergence
- **THEN** the assessment records whether the desired target of understandable near-end speech without obvious swallowing or pumping is met while far-end removal remains effective
- **THEN** any miss against that target is documented as a known default-algorithm quality limitation for later work without invalidating separately verified output, lifecycle, safety or client-consumption behavior

#### Scenario: Render silence and restart are validated
- **WHEN** playback becomes silent or either selected physical input is explicitly restarted according to the approved validation sequence
- **THEN** the engine exposes bounded degraded silence or clean recovery as specified, starts a fresh synchronization and AEC epoch and never submits stale PCM

#### Scenario: Public endpoint is consumed
- **WHEN** Windows Recorder and at least one target meeting application capture the paired `CABLE Output` endpoint during the approved AEC validation
- **THEN** they receive the processed stream for the expected duration without unexplained gaps, stale segments, raw-microphone fallback or a MiniAEC-owned public audio endpoint

#### Scenario: Validation lacks system authorization
- **WHEN** ordinary automated tests or a machine run execute without a supported installed VB-CABLE pair
- **THEN** they use fake inputs, fake echo cancellers and fake output sinks or stop with an actionable prerequisite result without changing Windows system state
