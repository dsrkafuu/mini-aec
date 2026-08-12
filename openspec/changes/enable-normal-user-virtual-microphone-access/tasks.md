## 1. Driver access policy and ownership

- [x] 1.1 Add a reviewable project-owned transport access-policy definition that grants SYSTEM and Administrators full control, grants Interactive Users only generic read/write, protects the DACL and grants no producer access to broader user or network principals.
- [x] 1.2 Create `MiniAECTransport` as nonexclusive at the I/O manager layer so every authorized create request reaches the existing project-owned owner arbitration.
- [x] 1.3 Keep `MiniAecDispatchCreate` as the single spin-lock-protected owner assignment point and verify a second authorized handle receives `STATUS_DEVICE_BUSY` without changing the active session or ring.
- [x] 1.4 Verify cleanup, close, process termination and driver shutdown release the owner exactly once, close any active session and clear all buffered PCM before another handle can connect.
- [x] 1.5 Add repository-level policy checks that fail if the intended protected SDDL, least-privilege Interactive Users rights, nonexclusive create setting or explicit busy path regresses.
- [x] 1.6 Confirm the IOCTL values, protocol structures, diagnostics schema, PCM format, ten-frame ring and public endpoint definition remain byte-for-byte unchanged.

## 2. Rust transport behavior

- [x] 2.1 Remove the `IsUserAnAdmin` elevation probe and `shell32` dependency from `mini-aec-windows-transport` connection handling.
- [x] 2.2 Map the driver's explicit busy result to `SinkErrorKind::Busy` and retain genuine policy denial as `SinkErrorKind::AccessDenied` without exposing Windows token or status types through project-owned boundaries.
- [x] 2.3 Update transport unit tests for driver unavailable, access denied, explicit busy, session lifecycle and malformed response behavior without requiring an installed driver.
- [x] 2.4 Verify engine, headless and tray controller error mapping still distinguishes driver unavailable, sink access denied and sender busy and never represents any of them as bypass.

## 3. Non-elevated validation tooling

- [x] 3.1 Add a separate read-only normal-user validation entry point that refuses to claim acceptance from an elevated or non-interactive token and records metadata-only token elevation, interactive-session and process identity evidence outside real-time workers.
- [x] 3.2 Ensure the normal-user validation entry point can check transport availability, open a deterministic sender session and report access denied, driver unavailable or sender busy without changing packages, certificates, boot state, devices or default audio roles.
- [x] 3.3 Add a deterministic contention procedure that keeps one authorized non-elevated producer active, verifies a second connection reports busy, then verifies a fresh connection succeeds after owner exit without stale session or PCM state.
- [x] 3.4 Integrate explicit bypass and default-AEC runtime commands with the non-elevated validation procedure while continuing to require exact physical microphone and render endpoint IDs and metadata-only ignored evidence paths.
- [x] 3.5 Keep all signing, installation, device restart, uninstall and default-role restoration actions exclusively in the existing confirmed privileged lifecycle and ensure neither validation path attempts to de-elevate, self-elevate or invoke an operating-system restart.

## 4. Automated verification and clean build

- [x] 4.1 Run the new access-policy and contention checks against repository sources and synthetic transport backends and confirm no Windows system state changes.
- [x] 4.2 Run `cargo fmt --all -- --check` and fix only formatting introduced by this change.
- [x] 4.3 Run targeted `mini-aec-windows-transport`, engine, lab and tray tests through `.tools\cargo-webrtc.cmd` and verify permission changes do not alter protocol, lifecycle, AEC or bypass behavior.
- [x] 4.4 Run `.tools\cargo-webrtc.cmd test --workspace` in the documented x64 Visual Studio environment and record the successful result.
- [x] 4.5 Run `.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings` and resolve all warnings introduced by this change.
- [x] 4.6 Run `driver\windows\scripts\preflight.ps1`, `driver\windows\scripts\verify-upstream.ps1 -CheckoutRoot .tools\sysvad-upstream` and a clean `driver\windows\scripts\build-validation.ps1` without installing the package.
- [x] 4.7 Inspect the unsigned validation package and project diff to confirm the public endpoint, protocol, vendored SysVAD baseline and frozen WebRTC/AEC files are unchanged and no generated package, certificate, private PCM or machine-specific endpoint identity is tracked.
- [x] 4.8 Run `git diff --check` and review all new validation output defaults to confirm private recordings remain only under ignored `artifacts/` paths.

## 5. Documentation and provenance

- [x] 5.1 Update `driver/windows/UPSTREAM.md` with the exact project-owned access-control and create-dispatch changes while preserving the pinned SysVAD source and complete local patch inventory.
- [x] 5.2 Update `driver/windows/README.md` and `driver/windows/VALIDATION.md` with the Interactive Users read/write contract, direct-access contention risk, normal-user validation commands, privileged lifecycle separation, manual-restart boundary and rollback procedure.
- [x] 5.3 Update `README.md`, `docs/technical-plan.md`, `docs/aec-baseline.md` and `docs/realtime-aec-validation.md` to remove stale Administrator-only runtime claims while retaining production signing, installer, multi-session and long-run drift limitations.
- [x] 5.4 Document that the protocol, AEC3 defaults, WebRTC pins, virtual endpoint name and windowless tray architecture are unchanged and that `vendor/UPSTREAM.md` requires no update.
- [x] 5.5 Review all active documentation and OpenSpec artifacts for claims that normal-user runtime access implies production distribution, per-executable authorization, multi-user arbitration or permission to automate system restart.

## 6. Approved Windows runtime acceptance and rollback

- [x] 6.1 Prepare a read-only machine inventory and exact build, test-sign, install, activation, non-elevated validation, uninstall and rollback plan with recorded device, package, certificate, boot and default-role targets, then obtain explicit user approval before any system-changing command.
- [x] 6.2 After approval, build and install only the reviewed development package and verify its installed control-device security descriptor matches the intended protected SYSTEM/Administrators/Interactive Users policy without changing unrelated endpoints or default roles beyond recorded Windows-originated behavior.
- [x] 6.3 If installation, activation or rollback requires an operating-system restart, stop after reporting the reason and wait for the user to save work and perform the restart manually before continuing the approved plan.
- [x] 6.4 From a verified non-elevated interactive process, run deterministic transport access and contention checks and record the tested token context, successful read/write access, explicit busy result, owner-exit cleanup and fresh reconnection.
- [x] 6.5 Run non-elevated real-time bypass through `MiniAEC Microphone` and verify Windows Recorder captures fresh physical-microphone audio without a UAC prompt, access denial, unexplained gap or stale replay.
- [x] 6.6 Run non-elevated frozen-default AEC through `MiniAEC Microphone` and verify Windows Recorder plus Discord or another target meeting application consume processed audio without permission-related failure or raw-microphone fallback; treat the previously recorded double-talk quality limitation as unchanged unless identical-input evidence proves otherwise.
- [x] 6.7 Execute the approved process exit/restart and sender-contention sequence while a capture client remains open or is reopened as documented and account for session, sequence, underrun, overflow, discard and failure diagnostics without stale PCM crossing owners.
- [x] 6.8 Execute the approved uninstall and certificate/package rollback, verify the saved device, service, endpoint, default-role, certificate and boot baseline is restored or report the exact pending difference, and never claim completion while a rollback discrepancy remains.
- [x] 6.9 Update acceptance documentation with metadata-only results, the exact non-elevated identity tested, residual local interactive-process and multi-session risks, remaining production installer/signing work and final rollback status.
