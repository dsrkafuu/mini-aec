## Purpose

Define how MiniAEC captures, analyzes, and accepts metadata-only evidence for sustained real-time audio stability so clock-drift correction is introduced only when the existing product path demonstrates a repeatable need.

## ADDED Requirements

### Requirement: Monotonic long-run evidence
The system SHALL record a versioned metadata-only event stream for a configured real-time AEC run that identifies the exact physical microphone and render endpoints, the requested duration, monotonic elapsed time, lifecycle state, synchronization epoch, per-input device position and QPC timestamp, synchronization delta, queue and discard counters, AEC recovery and processing counters, and virtual microphone transport counters without recording PCM or meeting content.

#### Scenario: Long-run recording starts
- **WHEN** a long-run run starts with exact active physical microphone and render endpoint IDs and an available `MiniAEC Microphone` sink
- **THEN** the evidence begins with a started event containing a new run, synchronization, AEC, and sink-session identity and a zero-based monotonic elapsed time
- **THEN** periodic events are recorded outside real-time workers at the documented bounded interval until the run stops or fails

#### Scenario: Long-run recording completes
- **WHEN** the engine reaches the configured duration and stops normally
- **THEN** the event stream ends with a final event that preserves the last counters and identifies the run as normally completed

#### Scenario: Long-run recording fails
- **WHEN** the engine enters `Failed` before the configured duration
- **THEN** the event stream records a failed event and the actionable engine error before the validation command exits unsuccessfully

### Requirement: QPC-anchored clock-rate analysis
The system SHALL analyze each input's device-position/QPC observations on stable synchronization epochs, SHALL estimate microphone and render effective frame rates and their relative drift in parts per million, and SHALL reject intervals containing non-monotonic positions, non-monotonic QPC, timestamp errors, discontinuities, epoch changes, or insufficient render activity from authoritative drift calculations.

#### Scenario: Stable observations are sufficient
- **WHEN** both inputs provide enough valid device-position/QPC observations across the required measurement window
- **THEN** the report includes each input's effective frame rate, relative drift in parts per million, usable duration, observation coverage, synchronization-delta trend, and the method and thresholds used

#### Scenario: A discontinuity divides the run
- **WHEN** either input reports a discontinuity, timestamp error, or synchronization-epoch change
- **THEN** the analyzer closes the current clean segment, excludes the crossing interval from rate estimation, reports the reset, and never treats positions from opposite sides as one continuous clock observation

#### Scenario: Evidence is insufficient
- **WHEN** missing timestamps, inactive render data, observation gaps, invalid ordering, or short clean segments prevent the required estimate
- **THEN** the report returns an `inconclusive` disposition with explicit data-quality reasons instead of reporting zero drift or passing the stability gate

### Requirement: Deterministic drift disposition
The system SHALL classify an analyzable run as `bounded-synchronizer-sufficient` or `clock-drift-compensation-required` using documented deterministic rules that consider relative-rate direction across clean windows, accumulated phase error relative to the existing pairing tolerance, synchronization-delta trend, stale-frame and silent-reference growth, queue behavior, and synchronization failure.

#### Scenario: Existing synchronizer is sufficient
- **WHEN** valid clean windows do not show persistent relative drift capable of exhausting the existing pairing tolerance and the run has no drift-attributable recurring whole-frame discard, silent-reference insertion, growing queue pressure, or synchronization failure
- **THEN** the report returns `bounded-synchronizer-sufficient` and includes the supporting measurements

#### Scenario: Persistent drift requires compensation work
- **WHEN** valid clean windows show consistent directional relative drift whose accumulated phase error reaches the existing pairing tolerance during the gate, or drift produces recurring whole-frame discard, silent-reference insertion, growing queue pressure, or terminal synchronization failure
- **THEN** the report returns `clock-drift-compensation-required`, identifies the triggering evidence, and does not alter the running synchronizer or enable a correction algorithm

#### Scenario: Conflicting signals cannot be resolved
- **WHEN** clock-rate, synchronization-delta, and counter evidence disagree beyond the documented uncertainty bounds
- **THEN** the report returns `inconclusive` and identifies the additional measurement needed rather than choosing a favorable disposition

### Requirement: Thirty-minute characterization gate
The project SHALL provide a documented 30-minute characterization gate for the K7 physical microphone and the current active Realtek speakers physical render endpoint through the frozen default AEC3 path and `MiniAEC Microphone`, with an ordinary capture client consuming the public endpoint for the scored interval.

#### Scenario: Characterization run is valid
- **WHEN** the exact target endpoints remain active, render packets are present for the drift-scored interval, `MiniAEC Microphone` is continuously consumed, at least 30 minutes of required evidence is captured, and observation coverage satisfies the documented threshold
- **THEN** the analyzer produces a conclusive drift disposition and a separate functional-stability result

#### Scenario: Functional stability passes
- **WHEN** the scored interval completes without terminal engine failure, unexplained discontinuity or reset, stale-audio replay, rejected sink write, sink failure, invalid AEC output, processing deadline miss, growing queue depth, or unaccounted client interruption
- **THEN** the report marks the 30-minute functional-stability gate passed while preserving every nonzero bounded degradation and transport counter for review

#### Scenario: Functional stability fails
- **WHEN** the engine terminates early or any required continuity, safety, boundedness, or client-consumption condition is violated
- **THEN** the report marks the functional-stability gate failed independently of the clock-drift disposition and identifies the first failing condition plus supporting counters

### Requirement: Thirty-minute acceptance and evidence-triggered reassessment
The project SHALL treat a conclusive 30-minute target-hardware result as the final real-device duration gate for this change, SHALL preserve the versioned metadata-only logging and analysis contract as the diagnostic baseline for early product versions, and SHALL require new evidence and a separately specified change before making a longer-duration run or drift correction mandatory.

#### Scenario: Thirty-minute result completes stability acceptance
- **WHEN** the 30-minute characterization returns `bounded-synchronizer-sufficient` and passes functional stability
- **THEN** the current product path satisfies the long-run stability gate for this change without a mandatory two-hour run

#### Scenario: Thirty-minute result requires compensation
- **WHEN** the 30-minute characterization returns `clock-drift-compensation-required`
- **THEN** a separate change specifies, implements, and verifies clock-drift compensation against the retained baseline evidence before the affected path can be accepted

#### Scenario: Early-version diagnostics trigger reassessment
- **WHEN** retained metadata from an early product version shows recurring synchronization maintenance, growing queue pressure, unexplained discontinuity, or another sustained stability risk not resolved by the accepted baseline
- **THEN** a separate change defines the evidence scope and duration of any extended validation or correction instead of applying an unconditional two-hour gate retroactively

### Requirement: Private and non-mutating validation boundary
The system SHALL keep raw long-run events, reports, endpoint identities, and any associated private recordings below ignored `artifacts/` or an existing ignored driver-validation output root, and analysis SHALL NOT install, update, restart, or remove drivers or devices, modify certificates or boot configuration, change Windows default audio roles, upload evidence, or initiate a system restart.

#### Scenario: Analysis runs on retained evidence
- **WHEN** the report command reads a previously captured event stream
- **THEN** it performs no audio-device, driver, certificate, boot, default-role, network, or PCM mutation and writes generated output only below an approved ignored evidence root

#### Scenario: Installed product path is unavailable
- **WHEN** a requested real-device gate lacks an installed and authorized `MiniAEC Microphone` development package
- **THEN** the validation stops with an actionable prerequisite and does not attempt installation, signing, device activation, or system restart

#### Scenario: Redistributable synthetic evidence is tested
- **WHEN** automated tests exercise rate estimation, discontinuities, insufficient data, drift classification, and functional failure
- **THEN** they use synthetic metadata without private recordings or Windows system mutation
