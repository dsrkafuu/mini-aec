## ADDED Requirements

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

## MODIFIED Requirements

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
