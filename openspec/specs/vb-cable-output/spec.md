# VB-CABLE output Specification

## Purpose

Define MiniAEC's supported use of a separately installed VB-CABLE pair as the external Windows audio bridge from processed PCM to ordinary recording applications.

## Requirements

### Requirement: User-owned external prerequisite
The product SHALL require the user to obtain, install, update, license and remove VB-CABLE separately from MiniAEC, SHALL direct the user to the official VB-Audio source, and SHALL NOT bundle, redistribute, silently install, silently update or silently uninstall VB-CABLE.

#### Scenario: VB-CABLE is not installed
- **WHEN** MiniAEC preflight cannot find one supported VB-CABLE endpoint pair
- **THEN** start fails with an actionable prerequisite message that identifies the official source and does not download software, request elevation or change driver, device, certificate, boot or default-audio state

#### Scenario: Installation or removal requires a restart
- **WHEN** the external VB-CABLE installer reports that Windows must restart
- **THEN** MiniAEC does not initiate, schedule or invoke a restart, shutdown or sign-out and leaves the decision and action to the user

### Requirement: Paired VB-CABLE signal route
The product SHALL render MiniAEC's output to the VB-CABLE playback endpoint conventionally displayed as `CABLE Input`, SHALL treat the paired recording endpoint conventionally displayed as `CABLE Output` as the downstream microphone selected by the user, and SHALL NOT claim that either endpoint is owned or named by MiniAEC.

#### Scenario: Supported pair is available
- **WHEN** preflight resolves exactly one supported active VB-CABLE playback endpoint and its paired active recording endpoint
- **THEN** MiniAEC reports both endpoint identities and identifies the route as `MiniAEC -> CABLE Input -> CABLE Output -> target application`

#### Scenario: Target application selects the recording side
- **WHEN** an ordinary recording or meeting application captures the resolved `CABLE Output` endpoint while MiniAEC renders to its paired `CABLE Input`
- **THEN** the application receives the MiniAEC output stream and does not need access to a private MiniAEC driver interface

### Requirement: Deterministic endpoint discovery and selection
The product SHALL resolve VB-CABLE endpoints by Windows endpoint identity, data-flow role and pair evidence, SHALL require an exact unambiguous active pair before starting, and SHALL NOT select an endpoint solely because a friendly name contains `CABLE` or silently fall back to a Windows default endpoint.

#### Scenario: Exactly one supported pair resolves
- **WHEN** the configured endpoint identities resolve to one active playback endpoint and its paired active recording endpoint with the expected data-flow roles
- **THEN** preflight succeeds and records the exact identities and display metadata used for the run

#### Scenario: Pair is missing or ambiguous
- **WHEN** an endpoint is absent, inactive, has the wrong data-flow role, cannot be paired deterministically or multiple candidates remain without an explicit selection
- **THEN** preflight fails before audio capture or output starts and reports the identities that require user action

### Requirement: Feedback-safe source separation
The product SHALL reject the resolved VB-CABLE recording endpoint as MiniAEC's physical microphone source and SHALL reject the resolved VB-CABLE playback endpoint as the physical render endpoint whose loopback is used as the AEC reference.

#### Scenario: VB-CABLE recording side is selected as microphone source
- **WHEN** the physical microphone configuration resolves to the same endpoint as the supported `CABLE Output` recording side
- **THEN** start fails with an actionable feedback-risk error before opening capture, AEC or output resources

#### Scenario: VB-CABLE playback side is selected as render reference
- **WHEN** the AEC render configuration resolves to the same endpoint as the supported `CABLE Input` playback side
- **THEN** start fails with an actionable post-AEC-loop error before opening loopback, AEC or output resources

### Requirement: Bounded format adaptation and rendering
The product SHALL accept complete 10 ms 48 kHz mono frames from the engine output boundary, SHALL adapt them deterministically to the active VB-CABLE playback format when required, and SHALL render through bounded storage and finite waits without accumulating unbounded latency.

#### Scenario: Endpoint accepts the engine format
- **WHEN** the active VB-CABLE playback endpoint accepts the engine's 48 kHz mono stream format
- **THEN** complete frames are rendered in order without an unnecessary sample-rate conversion

#### Scenario: Endpoint requires a compatible different mix format
- **WHEN** the active VB-CABLE playback endpoint exposes a supported mix format different from the engine frame representation
- **THEN** the output adapter performs deterministic channel and sample-format conversion while preserving duration, order and finite sample values

#### Scenario: Output falls behind
- **WHEN** VB-CABLE cannot consume output within the bounded queue and timing policy
- **THEN** MiniAEC preserves freshest-audio behavior, records the overflow or discard and enters the specified degraded or failed state instead of growing latency without bound

### Requirement: Clean stop, failure and restart behavior
The product SHALL stop submitting output, close the VB-CABLE render session and clear all partial, converted and queued PCM when MiniAEC stops or the output route fails, and a later explicit restart SHALL begin with a fresh session without submitting PCM retained from the prior run.

#### Scenario: MiniAEC stops while a client remains on CABLE Output
- **WHEN** a running MiniAEC session receives a stop command
- **THEN** it closes its `CABLE Input` render session, clears all retained output PCM and submits no stale prior-run audio to VB-CABLE

#### Scenario: VB-CABLE becomes unavailable
- **WHEN** the selected playback endpoint is invalidated or its render stream fails
- **THEN** MiniAEC stops the current run, clears output state, reports an actionable output failure and requires an explicit restart without selecting another endpoint

#### Scenario: MiniAEC restarts explicitly
- **WHEN** the user starts MiniAEC after the previous run has fully closed and the same supported pair is available
- **THEN** output resumes in a new session and no buffered or converted PCM from the previous run is rendered

### Requirement: Metadata-only output diagnostics
The product SHALL expose metadata sufficient to explain VB-CABLE selection, format negotiation, render progress, queue pressure, conversion, invalidation, failure and restart without recording PCM or meeting content.

#### Scenario: Output snapshot is requested
- **WHEN** a controller requests a runtime snapshot
- **THEN** it reports the selected playback and recording identities, active format, rendered frame count, queue depth and high-water mark, overflow and discard counts, conversion counts, endpoint invalidations, output failures and last project-owned error without PCM

### Requirement: Ordinary-client product acceptance
The project SHALL validate bypass and frozen-default AEC output by having Windows Recorder and at least one target meeting application capture the paired `CABLE Output` endpoint while MiniAEC renders to `CABLE Input`, and automated tests SHALL NOT install, update or remove VB-CABLE or mutate Windows driver, certificate, boot or default-role state.

#### Scenario: Ordinary clients consume the product route
- **WHEN** the approved runtime validation executes with an already installed supported VB-CABLE pair
- **THEN** Windows Recorder and the target meeting application receive the expected fresh MiniAEC stream from `CABLE Output` without unexplained gaps, stale replay or raw-microphone fallback

#### Scenario: Automated validation lacks the prerequisite
- **WHEN** automated tests run without an installed supported VB-CABLE pair
- **THEN** they use fake output endpoints or stop with an actionable prerequisite result without downloading, installing or mutating system audio state
