# Spec Delta

## MODIFIED Requirements

### Requirement: Monotonic long-run evidence
The system SHALL record a versioned metadata-only event stream for a configured real-time AEC run that identifies the exact physical microphone, physical render and paired VB-CABLE endpoints, the requested duration, monotonic elapsed time, lifecycle state, synchronization epoch, per-input device position and QPC timestamp, synchronization delta, queue and discard counters, AEC recovery and processing counters, and VB-CABLE output counters without recording PCM or meeting content.

#### Scenario: Long-run recording starts
- **WHEN** a long-run run starts with exact active physical microphone and render endpoint IDs and an available supported VB-CABLE pair
- **THEN** the evidence begins with a started event containing a new run, synchronization, AEC and output-session identity plus a zero-based monotonic elapsed time
- **THEN** periodic events are recorded outside real-time workers at the documented bounded interval until the run stops or fails

#### Scenario: Long-run recording completes
- **WHEN** the engine reaches the configured duration and stops normally
- **THEN** the event stream ends with a final event that preserves the last counters and identifies the run as normally completed

#### Scenario: Long-run recording fails
- **WHEN** the engine enters `Failed` before the configured duration
- **THEN** the event stream records a failed event and the actionable engine error before the validation command exits unsuccessfully

### Requirement: Thirty-minute characterization gate
The project SHALL provide a documented 30-minute functional-stability gate and a separate clock-drift characterization gate for the K7 physical microphone and the current active Realtek speakers physical render endpoint through the frozen default AEC3 path, with MiniAEC rendering to the supported `CABLE Input` endpoint and an ordinary capture client continuously consuming its paired `CABLE Output` for the scored interval.

#### Scenario: Characterization run is valid
- **WHEN** the exact physical endpoints and VB-CABLE pair remain active, render packets are present for the drift-scored interval, `CABLE Output` is continuously consumed, at least 30 minutes of required evidence is captured and observation coverage satisfies the documented threshold
- **THEN** the analyzer produces a conclusive drift disposition and a separate functional-stability result

#### Scenario: Functional gate completes with insufficient active render coverage
- **WHEN** the exact physical endpoints and VB-CABLE pair remain active, `CABLE Output` is continuously consumed, at least 30 minutes of metadata evidence is captured, the engine completes without a functional-stability failure and active render coverage is below the clock-drift analyzer threshold
- **THEN** the report marks the functional-stability gate passed when its continuity, boundedness, safety and client-consumption conditions are satisfied, marks clock-drift characterization `inconclusive` and makes no clock-drift compensation or long-run drift claim

#### Scenario: Functional stability passes
- **WHEN** the scored interval completes without terminal engine failure, unexplained discontinuity or reset, stale-audio submission, output failure, invalid AEC output, processing deadline miss, growing queue depth or unaccounted client interruption
- **THEN** the report marks the 30-minute functional-stability gate passed while preserving every nonzero bounded degradation and output counter for review

#### Scenario: Functional stability fails
- **WHEN** the engine terminates early or any required continuity, safety, boundedness, output or client-consumption condition is violated
- **THEN** the report marks the functional-stability gate failed independently of the clock-drift disposition and identifies the first failing condition plus supporting counters

### Requirement: Private and non-mutating validation boundary
The system SHALL keep raw long-run events, reports, endpoint identities and any associated private recordings below ignored `artifacts/`, and analysis SHALL NOT install, update, restart or remove drivers or devices, modify certificates or boot configuration, change Windows default audio roles, upload evidence or initiate a system restart.

#### Scenario: Analysis runs on retained evidence
- **WHEN** the report command reads a previously captured event stream
- **THEN** it performs no audio-device, driver, certificate, boot, default-role, network or PCM mutation and writes generated output only below an approved ignored evidence root

#### Scenario: Installed product path is unavailable
- **WHEN** a requested real-device gate lacks an installed supported VB-CABLE pair
- **THEN** the validation stops with an actionable prerequisite and does not attempt download, installation, signing, device activation or system restart

#### Scenario: Redistributable synthetic evidence is tested
- **WHEN** automated tests exercise rate estimation, discontinuities, insufficient data, drift classification and functional failure
- **THEN** they use synthetic metadata without private recordings or Windows system mutation
