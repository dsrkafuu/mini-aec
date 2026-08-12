## Why

MiniAEC currently requires an elevated process to open the private `MiniAECTransport` producer interface because the validation driver grants access only to SYSTEM and Administrators. The windowless tray application must be able to run in an ordinary interactive user session without elevation before the virtual microphone path can become a usable product runtime.

## What Changes

- Permit a non-elevated interactive Windows user to open the private producer interface with only the access needed to submit PCM and read transport diagnostics.
- Preserve the single-sender session, fixed protocol, driver-owned bounded ring, stale-audio prevention and public `MiniAEC Microphone` endpoint behavior while changing the runtime access boundary.
- Add repository checks and an explicitly approved development-driver validation that distinguish privileged package lifecycle actions from non-elevated MiniAEC runtime operation.
- Validate non-elevated bypass and default-AEC output through `MiniAEC Microphone`, including sender contention, process exit/restart, client consumption and complete rollback evidence.
- Update the driver provenance, security model, validation procedure and milestone documentation with the new access contract and its local-process contention boundary.
- Keep production signing, installer/upgrade design, long-run drift correction, AEC tuning, protocol changes, a privileged broker service, per-executable authorization and multi-user session arbitration out of scope.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `virtual-microphone-transport`: Replace the Administrator-only producer requirement with least-privilege access for non-elevated interactive users while retaining one active sender and all existing protocol, buffering and recovery guarantees.
- `driver-development-lifecycle`: Require the approved development lifecycle to verify that installation and rollback remain privileged operations while the installed runtime transport and end-to-end capture path work from a non-elevated interactive process.

## Impact

- Affects the project-owned Windows control-device security descriptor, related driver source inventory, validation scripts or harnesses, Rust transport diagnostics/tests, and driver/product documentation.
- Requires an explicitly approved rebuild, test-sign, install, device activation, non-elevated runtime validation and full rollback on Windows 11 x64; no agent may initiate a restart, shutdown or sign-out.
- Does not change the IOCTL layout, PCM format, endpoint name, WebRTC dependency, default AEC3 parameters, WASAPI capture/synchronization behavior or Tauri windowless architecture.
- The development package remains non-distributable until production signing, installer, upgrade and broader security decisions are completed separately.
