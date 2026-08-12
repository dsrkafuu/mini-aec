## Context

The completed M1-M3 development path uses one private `MiniAECTransport` control device to move processed PCM into the public `MiniAEC Microphone` endpoint. `IoCreateDeviceSecure` currently applies a protected SDDL that grants generic-all only to SYSTEM and Administrators, so the Rust tray, bypass command and real-time AEC command must run elevated even after an administrator has installed the validation package.

The transport already has the important isolation primitives this change must preserve: buffered IOCTLs with read/write access bits, one owner `FILE_OBJECT`, one active session, strict session and sequence validation, a ten-frame driver-owned ring, silence on underrun and atomic stale-audio cleanup on handle closure. The current device is also created with the kernel `Exclusive` flag even though `MiniAecDispatchCreate` independently protects `OwnerFile` and returns `STATUS_DEVICE_BUSY`; the kernel flag can prevent the second create IRP from reaching that explicit busy path. Rust therefore guesses whether Win32 access denied means policy denial or contention from whether the process is elevated, which becomes invalid once an ordinary user is authorized.

This is a Windows driver security and validation change. It must not alter the public endpoint, protocol layout, audio format, AEC implementation, synchronization policy or tray architecture. Installation, test signing, device activation and rollback remain privileged and explicitly approved; only steady-state MiniAEC operation becomes non-elevated.

## Goals / Non-Goals

**Goals:**

- Allow the interactive windowless MiniAEC runtime to open the producer transport with an ordinary non-elevated token.
- Apply a narrow, reviewable device ACL while preserving SYSTEM/Administrator maintenance access and denying principals that do not represent a local interactive runtime.
- Preserve one-handle and one-session ownership, precise busy versus access-denied errors, bounded buffering and stale-audio cleanup across process failure and restart.
- Prove bypass and default-AEC consumption through `MiniAEC Microphone` from a process whose non-elevated identity is recorded in metadata.
- Preserve the existing explicit approval, complete rollback and manual operating-system restart boundaries.

**Non-Goals:**

- Production driver signing, installer, automatic update, service management or enterprise deployment policy.
- A privileged broker service, per-executable trust, code-signing identity checks or per-user dynamic device ACLs.
- Concurrent producers, per-session audio routing or arbitration between multiple interactive Windows sessions.
- IOCTL, diagnostics schema, PCM, ring-capacity, public endpoint, WASAPI, AEC3, Tauri UI or algorithm changes.

## Decisions

### Use a protected static device ACL for the interactive runtime

The control device will retain generic-all entries for SYSTEM and Builtin Administrators and add a generic-read/generic-write entry for the Interactive Users well-known SID. The protected DACL will not add entries for Everyone, Authenticated Users, Builtin Users, anonymous, guests or network logons. Read/write matches the existing `CreateFile` access and IOCTL access bits, while avoiding generic-all for the ordinary runtime.

A privileged service broker was considered because it could keep the driver Administrator/SYSTEM-only and authenticate clients over IPC. It is rejected for this change because it introduces a new service lifecycle, IPC protocol, installer dependency and additional stale-session failure boundary before the direct transport has reached a normal-user runtime milestone. Granting Builtin Users or Authenticated Users was also considered and rejected because those groups cover more non-interactive contexts than the windowless tray product requires. A per-user SID in the device ACL was rejected because the driver is machine-wide, may start before user logon and has no approved installer/service mechanism to update a dynamic descriptor safely.

### Move exclusive ownership entirely into the existing create dispatch

The control device will no longer request kernel `Exclusive` creation. `MiniAecDispatchCreate` will remain the single ownership gate, use the existing spin lock to assign one `OwnerFile`, and return `STATUS_DEVICE_BUSY` to every authorized second opener. Cleanup and close remain idempotent and clear the active session and all buffered PCM before releasing ownership.

Keeping both exclusivity mechanisms was considered and rejected because the kernel-level rejection can collapse contention into Win32 access denied before project code can return a distinct busy status. Allowing multiple handles and arbitrating only `OPEN_SESSION` was rejected because a diagnostics-only or abandoned handle could make ownership and cleanup ambiguous.

### Remove process-elevation guessing from the Rust adapter

The Windows transport adapter will map the explicit Win32 busy result to `SinkErrorKind::Busy` and genuine access denied to `SinkErrorKind::AccessDenied` without calling `IsUserAnAdmin` or linking `shell32`. The project-owned sink and engine error types remain unchanged, so tray and headless callers continue to distinguish sender contention, permission failure and driver absence without Windows security types escaping the adapter.

Retaining the elevation heuristic was considered and rejected because a non-elevated authorized process can now encounter both contention and policy denial, and token elevation is no longer a valid discriminator.

### Keep the transport and audio contracts byte-for-byte stable

The IOCTL codes, protocol version, request structures, diagnostics schema, sample format, session identity, sequence rules and ten-frame ring remain unchanged. The driver source provenance record will describe only the access-control and ownership-dispatch change. No AEC dependency or vendored WebRTC file changes.

Bumping the protocol version was considered and rejected because authorized callers send and receive identical bytes; access policy and create status are outside the wire layout.

### Split privileged lifecycle validation from ordinary runtime validation

The existing lifecycle plan, inventory, build, signing preparation, install, device restart, uninstall and rollback remain privileged and refuse mutation without the established confirmation switch and explicit user approval. A separate read-only runtime validation entry point will record that the current token is interactive and non-elevated, refuse to claim the normal-user gate from an elevated token, and exercise transport connection without changing driver, certificate, boot, device or default-role state.

After an approved development package is active, validation will cover a deterministic sender, real-time bypass, default AEC, a second-sender busy attempt, owner process exit, fresh reconnection, Windows Recorder and Discord consumption, and metadata-only evidence. Private audio remains under ignored `artifacts/`. If device activation or rollback reaches a reboot boundary, automation stops and only the user may restart Windows.

An automatic de-elevation helper was considered and rejected because it would complicate token provenance and could hide that the product process itself was launched incorrectly. The operator will start the runtime validation from a normal interactive terminal or the ordinary tray process, and the harness will verify rather than manufacture that security context.

## Risks / Trade-offs

- [Any local interactive process can attempt to inject audio or hold the single sender slot] → Limit the ACL to Interactive Users rather than broader user groups, preserve exclusive owner cleanup, expose busy distinctly, document this direct-access trust boundary and defer per-executable mediation to the production installer/security design.
- [A second interactive session can win the machine-wide sender slot] → Keep deterministic first-owner behavior and declare multi-session arbitration unsupported in this change; test one active interactive session and record the limitation.
- [Disabling kernel exclusivity could introduce an ownership race] → Retain the spin-lock-protected create dispatch as the only assignment point and add concurrent-open, cleanup, close and crash-recovery tests around one `OwnerFile`.
- [A malformed ACL could either keep normal users blocked or grant excessive rights] → Verify the exact protected SDDL in repository checks, inspect the installed device security descriptor during the approved run and test both non-elevated success and a caller outside the intended principal set where safely available.
- [Changing a test-signed driver can disturb devices, defaults or boot state] → Require the existing inventory, reviewed plan, explicit mutation approval and complete uninstall comparison; never automate an operating-system restart.
- [Passing on an administrator account with a filtered UAC token may not represent every standard account policy] → Record token elevation and group metadata, prefer a true standard interactive account when available, and describe exactly which identity was tested rather than generalizing beyond the evidence.

## Migration Plan

1. Add repository-level tests and checks for the intended SDDL, nonexclusive device creation, explicit busy status and Rust error mapping before building a driver.
2. Update the project-owned driver source and provenance record without changing protocol or vendored SysVAD baseline files beyond the recorded integration point.
3. Run format, Rust tests, strict Clippy, driver preflight, upstream verification and a clean unsigned validation-package build without changing Windows state.
4. Prepare a read-only inventory and exact install, activation, non-elevated validation and rollback plan; obtain explicit approval before every system-changing command.
5. Install and activate only the reviewed test-signed development package, stopping for a user-performed operating-system restart if Windows requires one.
6. From a verified non-elevated interactive process, run deterministic transport, bypass, default-AEC, contention, process-restart and client-consumption scenarios while retaining only metadata and private ignored recordings.
7. Execute the approved uninstall and certificate/package rollback, compare device and default-role state with the saved baseline and report any pending restart or discrepancy without claiming completion.
8. Update active documentation with the exact tested identity, results, residual direct-access risk and remaining production installer/signing work.

## Open Questions

- Whether a production release should retain direct Interactive Users access or replace it with a service-SID broker remains an M5 installer and threat-model decision; this change will provide runtime and contention evidence for that later choice.
- Whether multi-session Windows hosts require per-session ownership or an explicit single-session product restriction remains deferred until the supported deployment model is defined.
