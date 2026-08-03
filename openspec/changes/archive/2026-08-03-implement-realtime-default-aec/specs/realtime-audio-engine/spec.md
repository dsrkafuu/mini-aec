## MODIFIED Requirements

### Requirement: Independent real-time engine lifecycle
The system SHALL provide a Tauri-independent real-time engine that accepts project-owned configuration and lifecycle commands, reports project-owned state and can start, stop and explicitly restart either one physical-microphone bypass run or one dual-input AEC-enabled run without depending on a CLI, WebView or async UI runtime.

#### Scenario: AEC-enabled engine starts successfully
- **WHEN** a stopped engine receives a valid AEC configuration with available explicit physical microphone, physical render-loopback and virtual microphone sink endpoints
- **THEN** it transitions through Starting to RunningAec
- **THEN** it reports both configured source identities and a new run, synchronization epoch, AEC instance and sink-session identity

#### Scenario: Bypass engine starts successfully
- **WHEN** a stopped engine receives a valid explicit bypass configuration with an available physical microphone and virtual microphone sink
- **THEN** it transitions through Starting to RunningBypass without opening render loopback or constructing an echo canceller

#### Scenario: Engine stops normally
- **WHEN** a running or degraded engine receives a stop command
- **THEN** it stops accepting both source roles, closes source, AEC and sink resources, clears partial, queued and processed PCM and transitions to Stopped

#### Scenario: Engine restarts explicitly
- **WHEN** a stopped or failed engine receives a new valid start command after its prior run has been closed
- **THEN** it creates distinct run, synchronization, AEC when applicable and sink-session identities, restarts sink sequencing at zero and exposes no partial or queued PCM from the prior run

### Requirement: Explicit physical source isolation
The engine SHALL require exact Windows endpoint IDs for every configured physical input role, SHALL resolve and report display metadata before capture, SHALL reject `MiniAEC Microphone` as its own physical microphone source and SHALL NOT silently fall back to Windows defaults or other endpoints.

#### Scenario: Explicit physical microphone and render are selected
- **WHEN** an AEC configuration resolves to one active non-MiniAEC capture endpoint and one active physical render endpoint
- **THEN** the engine opens exactly those endpoints in their configured capture and render-loopback roles and reports both IDs and friendly names

#### Scenario: Explicit bypass source is selected
- **WHEN** a bypass configuration resolves to an active non-MiniAEC capture endpoint
- **THEN** the engine opens exactly that capture endpoint without requiring or opening a render endpoint

#### Scenario: MiniAEC is selected recursively
- **WHEN** the configured physical microphone resolves to the public `MiniAEC Microphone` capture endpoint
- **THEN** start fails with an actionable invalid-source error before opening render, AEC or virtual microphone sink resources

#### Scenario: Configured source is unavailable
- **WHEN** any endpoint required by the selected mode is missing, inactive or has the wrong data-flow role
- **THEN** start fails without opening another endpoint or changing Windows default-device policy

### Requirement: Metadata-only engine diagnostics
The engine SHALL expose bounded snapshots and validation events containing lifecycle, mode, both source roles when applicable, framing, synchronization, AEC, queue and sink counters needed to explain continuity and recovery, and SHALL NOT include PCM or meeting content in logs.

#### Scenario: Bypass snapshot is requested
- **WHEN** a controller requests a snapshot for a bypass run
- **THEN** it reports bypass state, microphone identity, run and session identity, capture and silence counts, discontinuities, timestamp errors, normalized frames, queue depth and high-water mark, local overflows and discards, accepted sink frames, sink failures and the last project-owned error without claiming render or AEC activity

#### Scenario: AEC snapshot is requested
- **WHEN** a controller requests a snapshot for an AEC-enabled run
- **THEN** it includes the bypass-era fields plus render identity and counters, synchronization and skew evidence, AEC processing and recovery evidence, current degradation reason and AEC-specific state

#### Scenario: Diagnostics are persisted
- **WHEN** the headless validation command writes periodic events or a final result
- **THEN** it writes metadata only to an ignored evidence path outside real-time workers and does not write PCM or meeting content
