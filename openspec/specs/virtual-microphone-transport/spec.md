# Virtual microphone transport Specification

## Purpose

Define the isolated, bounded and recoverable development transport that injects deterministic PCM into the single public `MiniAEC Microphone` capture endpoint.

## Requirements

### Requirement: Single public virtual microphone endpoint
The system SHALL expose one Windows capture endpoint named `MiniAEC Microphone` for the validation driver, SHALL make it eligible for user selection as a default input device in Windows system sound settings, SHALL NOT apply an endpoint property that prevents that selection, and SHALL NOT expose the private producer transport as an additional public render or capture endpoint.

#### Scenario: Driver starts successfully
- **WHEN** the validation driver is installed and started
- **THEN** Windows audio endpoint enumeration shows exactly one new public capture endpoint named `MiniAEC Microphone`
- **THEN** Windows system sound settings shows `MiniAEC Microphone` as a selectable default input candidate
- **THEN** ordinary Windows recording and playback endpoint lists show no producer-only endpoint

#### Scenario: User selects the virtual microphone as default input
- **WHEN** the user selects `MiniAEC Microphone` as the default device for a Windows input role
- **THEN** Windows reports `MiniAEC Microphone` as the current default endpoint for that role
- **THEN** an ordinary recording client opened through that default capture role can capture from `MiniAEC Microphone`

### Requirement: Restricted private producer interface
The system SHALL accept user-mode PCM through a versioned driver control interface that is not an audio endpoint or globally named shared-memory mapping, SHALL grant SYSTEM and Administrators full control, SHALL grant interactive Windows users only the device read and write access required by the producer protocol, SHALL grant no producer access to anonymous, guest, network-only or other callers outside those principals, and SHALL allow at most one open sender handle and one active sender session.

#### Scenario: Non-elevated interactive sender opens the interface
- **WHEN** a non-elevated interactive user opens the private control interface while no sender handle is active
- **THEN** the driver accepts one handle with protocol read/write access without a UAC prompt or an additional public render or capture endpoint

#### Scenario: Privileged sender opens the interface
- **WHEN** SYSTEM or an Administrator opens the private control interface while no sender handle is active
- **THEN** the driver accepts one handle and preserves the same producer protocol and session behavior

#### Scenario: Unauthorized sender opens the interface
- **WHEN** a caller outside the authorized system, administrator and interactive-user principals attempts to open the private control interface
- **THEN** the driver returns an access-denied result without assigning sender ownership or changing buffered PCM

#### Scenario: Concurrent sender opens the interface
- **WHEN** any authorized caller attempts to open a second producer handle while another handle owns the interface
- **THEN** the driver returns an explicit busy result without changing the active handle, session or buffered PCM

#### Scenario: Runtime access policy is audited
- **WHEN** the validation package access policy is inspected before installation
- **THEN** it shows protected full-control entries for SYSTEM and Administrators, a read/write-only entry for interactive users and no producer grant for Everyone, anonymous, guest or network-only principals

### Requirement: Non-elevated product runtime
The system SHALL allow the windowless MiniAEC runtime and headless validation commands to open the installed private producer transport, publish bypass or default-AEC PCM and read transport diagnostics from a non-elevated interactive Windows process without a UAC prompt or privileged helper.

#### Scenario: Non-elevated bypass reaches the public endpoint
- **WHEN** an approved development driver is installed and a non-elevated interactive user starts the bypass engine with an explicit physical microphone endpoint
- **THEN** the engine opens one producer session without elevation and an ordinary recording client captures fresh physical-microphone audio from `MiniAEC Microphone`

#### Scenario: Non-elevated default AEC reaches the public endpoint
- **WHEN** an approved development driver is installed and a non-elevated interactive user starts the default-AEC engine with explicit physical microphone and render endpoint IDs
- **THEN** the engine reaches its normal AEC lifecycle through one producer session and an ordinary recording client captures processed audio from `MiniAEC Microphone`

#### Scenario: Non-elevated runtime restarts
- **WHEN** the non-elevated MiniAEC process closes or exits and another non-elevated MiniAEC process opens a fresh session
- **THEN** the new process can publish audio without elevation and no PCM, session identity or frame sequence from the prior process is exposed after the new session begins

### Requirement: Validated fixed-frame protocol
The system SHALL accept only complete 10 ms frames containing 480 samples and 960 bytes of 48 kHz mono PCM16, and SHALL validate the protocol version, header length, payload length, session identity and monotonic frame sequence before copying PCM into driver-owned memory.

#### Scenario: Sender submits a valid frame
- **WHEN** the active sender submits a complete frame with the negotiated protocol version, current session identity and next frame sequence
- **THEN** the driver accepts the complete frame and reports its sequence as accepted

#### Scenario: Sender submits a malformed or stale frame
- **WHEN** a write has an unsupported version, invalid length, stale session identity, non-monotonic sequence or partial PCM payload
- **THEN** the driver rejects the entire write, increments the rejected-write diagnostics and does not expose any part of its PCM to capture clients

### Requirement: Driver-owned bounded ring buffer
The system SHALL copy accepted frames into a driver-owned ring buffer with a fixed capacity of 10 complete frames, SHALL NOT map driver ring memory into user mode, and SHALL synchronize producer and capture access without exposing partial frames.

#### Scenario: Capacity is available
- **WHEN** a valid frame arrives while fewer than 10 unread frames are buffered
- **THEN** the driver appends the complete frame without delaying capture until the ring is full

#### Scenario: Buffer overflows
- **WHEN** a valid frame arrives while 10 unread frames are buffered
- **THEN** the driver discards the oldest unread complete frame, accepts the new frame and increments overflow and discarded-frame diagnostics

### Requirement: Deterministic PCM injection
The system SHALL accept a deterministic 48 kHz mono PCM16 stream from the minimal user-mode sender through a project-owned virtual microphone sink boundary and SHALL make the corresponding signal available from `MiniAEC Microphone`.

#### Scenario: Sender feeds the validation pattern
- **WHEN** the sender writes the documented deterministic signal with session and frame sequence diagnostics
- **THEN** Windows Recorder captures the expected signal markers from `MiniAEC Microphone` in the expected order
- **THEN** the sender log and captured duration can be correlated without using private recordings

### Requirement: Continuous capture
The system SHALL consume buffered PCM according to the capture audio clock, maintain a monotonic capture timeline while a sender supplies valid PCM and report transport underrun, overflow, discarded frame, rejected write and session transition counters needed to explain discontinuities.

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
The system SHALL accept a new sender session after the prior sender closes or exits and SHALL atomically discard all unconsumed PCM belonging to the prior session before exposing new-session PCM.

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
