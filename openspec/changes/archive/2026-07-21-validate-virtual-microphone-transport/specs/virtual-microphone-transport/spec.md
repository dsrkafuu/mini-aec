## ADDED Requirements

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
The system SHALL accept user-mode PCM through a versioned driver control interface that is not an audio endpoint or globally named shared-memory mapping, SHALL restrict the validation interface to SYSTEM and Administrators, and SHALL allow at most one active sender session.

#### Scenario: Authorized sender opens the interface
- **WHEN** an authorized sender opens the private control interface while no sender session is active
- **THEN** the driver creates one active sender session without adding a public render or capture endpoint

#### Scenario: Unauthorized or concurrent sender opens the interface
- **WHEN** a caller lacks the required access or a second sender tries to open the interface while a session is active
- **THEN** the driver rejects the request without changing the active session or its buffered PCM

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
