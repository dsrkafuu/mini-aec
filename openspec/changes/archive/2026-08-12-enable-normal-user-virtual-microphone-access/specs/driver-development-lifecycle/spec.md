## ADDED Requirements

### Requirement: Non-elevated runtime acceptance
The approved development validation SHALL prove that the installed driver can be consumed by a non-elevated interactive MiniAEC process, SHALL record evidence that the runtime process is not elevated, and SHALL keep every package, certificate, device, boot and rollback mutation in the separately approved privileged lifecycle.

#### Scenario: Runtime identity is verified
- **WHEN** the normal-user validation command starts after the approved driver installation
- **THEN** it records metadata sufficient to show that the current process is interactive and not elevated before opening `MiniAECTransport`

#### Scenario: Non-elevated end-to-end validation succeeds
- **WHEN** the non-elevated runtime executes the approved bypass and default-AEC scenarios and Windows Recorder plus at least one target meeting application consume `MiniAEC Microphone`
- **THEN** the clients receive fresh output for the documented duration without an access-denied failure, unexplained gap, stale replay or raw-microphone fallback caused by the permission change

#### Scenario: Non-elevated sender contention is validated
- **WHEN** one non-elevated process owns the producer handle and a second authorized process attempts to connect
- **THEN** the second process reports sender busy rather than access denied, the first session continues unchanged and a later connection succeeds after the first handle closes

#### Scenario: Runtime validation lacks an installed driver
- **WHEN** the non-elevated runtime validation runs without an installed authorized development driver
- **THEN** it stops with an actionable driver-unavailable result and does not request elevation or change certificates, packages, devices, boot configuration or default audio roles

## MODIFIED Requirements

### Requirement: Explicit approval for system changes
The validation workflow SHALL treat test-mode changes, certificate installation, driver installation, device restart, driver removal and any active restoration of a saved default input device as system-changing actions that require explicit user approval before execution, SHALL treat ordinary non-elevated MiniAEC start, stop, restart and capture-client consumption as runtime actions that do not require elevation or system-change approval after the driver is installed, and SHALL never initiate, schedule or invoke an operating-system restart, shutdown or sign-out.

#### Scenario: Validation is invoked without approval
- **WHEN** a contributor runs the default lifecycle validation entry point without authorizing system changes
- **THEN** it reports prerequisites and the commands that would run without modifying boot configuration, certificate stores, drivers, device state or default input policy

#### Scenario: An approved driver operation runs
- **WHEN** the user explicitly approves a test-signing or driver lifecycle operation
- **THEN** the workflow records the command, result and affected device or package identity needed for rollback

#### Scenario: Installed runtime starts without elevation
- **WHEN** an approved development driver is already active and an interactive user starts, stops or explicitly restarts MiniAEC without an elevated token
- **THEN** the runtime operation proceeds without a UAC prompt, privileged helper or mutation of package, certificate, boot, device-installation or default-role state

#### Scenario: A workflow reaches a restart boundary
- **WHEN** installation, device activation or rollback cannot complete without an operating-system restart
- **THEN** the workflow reports why a restart is needed and stops so the user can save work and perform the restart manually
