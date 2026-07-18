## ADDED Requirements

### Requirement: Single public virtual microphone endpoint
The system SHALL expose one Windows capture endpoint named `MiniAEC Microphone` for the validation driver and SHALL NOT expose the private producer transport as an additional public render or capture endpoint.

#### Scenario: Driver starts successfully
- **WHEN** the validation driver is installed and started
- **THEN** Windows audio endpoint enumeration shows exactly one new public capture endpoint named `MiniAEC Microphone`
- **THEN** ordinary Windows recording and playback endpoint lists show no producer-only endpoint

### Requirement: Deterministic PCM injection
The system SHALL accept a deterministic 48 kHz mono PCM16 stream from the minimal user-mode sender through a project-owned virtual microphone sink boundary and SHALL make the corresponding signal available from `MiniAEC Microphone`.

#### Scenario: Sender feeds the validation pattern
- **WHEN** the sender writes the documented deterministic signal with session and frame sequence diagnostics
- **THEN** Windows Recorder captures the expected signal markers from `MiniAEC Microphone` in the expected order
- **THEN** the sender log and captured duration can be correlated without using private recordings

### Requirement: Continuous capture
The system SHALL maintain a monotonic capture timeline while a sender supplies valid PCM and SHALL report transport underrun, overflow, rejected write, and session transition counters needed to explain discontinuities.

#### Scenario: Five-minute continuous recording
- **WHEN** Windows Recorder captures `MiniAEC Microphone` for at least five minutes while the sender continuously produces the validation pattern
- **THEN** the recording has the expected duration and ordered periodic markers without an unexplained gap, stale segment, or session reset
- **THEN** transport diagnostics contain no unaccounted underrun, overflow, or rejected write

### Requirement: Deterministic sender absence behavior
The system SHALL advance the capture clock with zero-valued silence whenever no sender data is available and SHALL NOT replay PCM retained from an earlier point in the current or previous sender session.

#### Scenario: Sender exits during recording
- **WHEN** the sender process exits while Windows Recorder keeps `MiniAEC Microphone` open
- **THEN** the capture stream transitions to silence without replaying an earlier validation marker
- **THEN** the capture timeline continues until the recording client stops or the driver is restarted

### Requirement: Sender restart recovery
The system SHALL accept a new sender session after the prior sender exits and SHALL discard all unconsumed PCM belonging to the prior session before exposing new-session PCM.

#### Scenario: Sender restarts during one recording
- **WHEN** the sender is stopped, the endpoint produces silence, and a new sender process starts while Windows Recorder keeps recording
- **THEN** capture resumes with the new session marker in order
- **THEN** no PCM or frame sequence from the previous session appears after the new session begins

### Requirement: Driver restart recovery
The system SHALL return to an injectable and recordable state after the validation driver is disabled and re-enabled or otherwise restarted.

#### Scenario: Driver restarts
- **WHEN** an approved validation step restarts the driver and Windows Recorder reopens `MiniAEC Microphone` after the endpoint returns
- **THEN** a newly started sender can inject the validation pattern and Windows Recorder can capture it
- **THEN** the new driver and sender sessions do not expose PCM retained before the restart

### Requirement: Transport comparison gate
The system SHALL evaluate the private WaveRT render sink and restricted shared-ring transport with the same input, duration, endpoint visibility checks, recovery scenarios and diagnostics, and SHALL retain only one selected transport in the default validation build.

#### Scenario: One candidate satisfies all hard gates
- **WHEN** the comparison evidence shows that one or both candidates satisfy public endpoint isolation, deterministic underrun, sender restart and driver restart requirements
- **THEN** the decision record selects the qualifying candidate according to the documented preference rule and records the measurements and rationale
- **THEN** the non-selected prototype is excluded from the default validation build

#### Scenario: Neither candidate satisfies all hard gates
- **WHEN** both candidates fail at least one hard gate
- **THEN** implementation stops with the failures recorded and no candidate is represented as the accepted transport
