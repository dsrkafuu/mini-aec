# Driver development lifecycle Specification

## Purpose

Define the auditable, reproducible, explicitly approved development lifecycle for building, test-signing, installing, restarting, validating and fully rolling back the MiniAEC Windows driver.

## Requirements

### Requirement: Auditable SysVAD baseline
The project SHALL identify one Microsoft SysVAD source with an immutable commit, source path, license, imported file inventory and complete local patch inventory, and SHALL keep the applicable MS-PL license notice with redistributed source.

#### Scenario: Upstream record is reviewed
- **WHEN** a contributor reviews the Windows driver upstream record
- **THEN** it identifies `microsoft/Windows-driver-samples`, `audio/sysvad`, commit `2ee527bfeb0aeb6be11f0a8b6dce4011b358ce89` and the Microsoft Public License
- **THEN** every imported or modified upstream file can be traced to the fixed baseline

### Requirement: Reproducible validation driver build
The project SHALL document and check the exact Windows, Visual Studio, SDK and WDK prerequisites used to produce an x64 Debug validation driver package and SHALL separate generated packages, certificates and machine-specific outputs from committed source.

#### Scenario: Prerequisites are available
- **WHEN** the documented supported toolchain is installed and the validation build command runs from a clean source checkout
- **THEN** it produces the expected driver binary, INF and catalog inputs or signed catalog without requiring unrecorded source changes
- **THEN** generated binaries and certificate private keys remain outside version control

#### Scenario: Prerequisites are missing or incompatible
- **WHEN** a required Visual Studio, SDK, WDK or signing prerequisite is missing or incompatible
- **THEN** the build or preflight check stops with an actionable diagnostic before changing driver, certificate store or boot state

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

### Requirement: Test-signed installation isolation and default eligibility
The validation package SHALL use development-only identities and test signing, SHALL identify itself as non-production, SHALL expose `MiniAEC Microphone` as a user-selectable default input candidate in Windows system sound settings, and SHALL install without altering unrelated physical audio endpoints. The validation workflow SHALL NOT actively force a default input change, but SHALL record the default endpoint for each input role before and after installation and SHALL accept a Windows-originated automatic selection of the new active endpoint.

#### Scenario: Validation driver is installed
- **WHEN** the approved test-signed package is installed on the development machine
- **THEN** `MiniAEC Microphone` appears in Windows system sound settings and can be selected by the user as a default input device
- **THEN** the workflow records whether Windows automatically changed any default input role and treats the recorded automatic selection as accepted installation behavior
- **THEN** the pre-install inventory of unrelated physical capture and render endpoints remains present and unchanged

### Requirement: Clean driver restart
The validation workflow SHALL support an approved disable-enable or equivalent driver restart and SHALL record whether the public endpoint disappears, returns and accepts a fresh stream as specified by the transport capability.

#### Scenario: Approved restart completes
- **WHEN** the driver restart procedure is executed after saving the current endpoint inventory
- **THEN** the procedure reports the expected endpoint removal and return transitions
- **THEN** post-restart transport validation can run without reinstalling the package

### Requirement: Complete uninstall recovery
The validation workflow SHALL remove the validation device, driver package, service registrations and validation-only control interfaces, and SHALL provide checks that compare the resulting audio device state and default input roles with the saved pre-install baseline. The workflow SHALL NOT claim rollback succeeded until the saved default input roles are restored; if Windows does not restore them automatically, any active restoration SHALL require explicit user approval.

#### Scenario: Validation driver is uninstalled
- **WHEN** the approved uninstall and rollback procedure completes
- **THEN** `MiniAEC Microphone` and all validation-only producer interfaces are absent
- **THEN** no validation driver service or staged package targeted by the procedure remains
- **THEN** unrelated physical audio endpoints match the saved pre-install baseline
- **THEN** every default input role matches the saved pre-install baseline

#### Scenario: Uninstall is incomplete
- **WHEN** any endpoint, service, package or validation certificate targeted for removal remains, or any default input role differs from the saved pre-install baseline
- **THEN** the workflow reports the exact remaining identity or default-role difference and does not claim that rollback succeeded
