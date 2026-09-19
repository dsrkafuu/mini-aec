# MiniAEC Microphone driver boundary

> Legacy development evidence only. OpenSpec change `adopt-vb-cable-output` retires this project-owned SysVAD path; it is not a supported product dependency, release path, installer, or signing strategy. The active product writes processed PCM to separately installed VB-CABLE `CABLE Input`, and target applications consume `CABLE Output`. Do not build, sign, install, restart, or remove this driver for current MiniAEC work. The directory is retained only until replacement acceptance permits source cleanup.

This directory contains the Windows 11 x64 validation driver for the single public capture endpoint `MiniAEC Microphone`. It is derived from the pinned Microsoft SysVAD snapshot recorded in [`UPSTREAM.md`](UPSTREAM.md); the validation package is development-only and is not a production installer or signing path.

## Current validation status

M1 validated deterministic user-mode signal transport, the single public capture endpoint, sender and session isolation, underrun silence, restart behavior, capture-client consumption and complete rollback. M2 reused the same transport for a five-minute physical-microphone bypass run and validated stop/start isolation, sender contention, device restart and stale-audio prevention on the elevated development path.

The completed M3 change consumes this transport through the same project-owned sink boundary without changing its IOCTL layout, public endpoint name, fixed PCM contract or driver ring behavior. Repository-level synthetic tests and a separately approved elevated run exercised Windows Recorder and Discord consumption, acoustic scenarios, render silence/recovery, sender contention, stop/start isolation and complete rollback. Client consumption succeeded; double-talk remained understandable but had obvious near-end swallowing, which is recorded as a frozen default-algorithm quality limitation for later work rather than tuned in M3. The later `enable-normal-user-virtual-microphone-access` development package passed separately approved non-elevated transport, bypass, default-AEC, Windows Recorder, Discord, contention, owner-exit/reconnect and complete rollback acceptance with the Interactive Users read/write policy. M4 then completed the approved 30-minute K7/Realtek speakers stability gate and, after the user's manual restart, verified by read-only inventory that the validation device, endpoint, package, certificate, service and service registry entry were absent, TESTSIGNING was off, and the saved default input/output roles were restored. Production signing and installer architecture remain unsolved.

## Selected transport

The driver exposes one ordinary audio capture endpoint and one private non-audio control device named `MiniAECTransport`. User-mode product code depends on the project-owned `VirtualMicrophoneSink` trait; only `mini-aec-windows-transport` knows the IOCTL and binary layouts.

The private WaveRT render-sink candidate was rejected because an active render endpoint that WASAPI can open also participates in the Windows audio endpoint model and cannot reliably be both usable and absent from ordinary application enumeration. The selected control interface does not create a producer-facing audio endpoint and does not map driver memory into user mode.

The current source defines a protected DACL that grants full control to SYSTEM and Administrators and only generic read/write to Interactive Users. It does not grant the producer interface to Everyone, Authenticated Users, Builtin Users, anonymous, guests or network logons. One project-owned create-dispatch owner holds the sender slot; a second authorized open receives an explicit busy result. Closing the handle, terminating its process or shutting down the driver releases ownership, closes the session and flushes all buffered PCM. Any local interactive process can still contend for this machine-wide slot, so per-executable trust, multi-session arbitration and a possible service-SID broker remain later installer/security decisions.

## Fixed protocol and buffer

Protocol version 1 accepts 48 kHz mono signed PCM16 in exact 10 ms frames: 480 samples and 960 payload bytes. Every open, write, diagnostics, and close request carries fixed lengths; session operations carry a nonzero 128-bit session identity, and writes carry a sequence that starts at zero and increases by one.

The driver owns a synchronized nonpaged ring with 10 complete-frame slots, equal to 100 ms or 9,600 PCM bytes. User mode cannot map the ring or publish partial frames. WaveRT capture consumes according to the audio clock. An empty ring produces fresh zero-valued silence; a full ring discards the oldest unread complete frame before accepting the newest. A new session atomically removes all previous-session audio.

Diagnostics schema 2 reports session state and identity, last accepted sequence, current depth, high-water mark, session opens/closes/resets, accepted frames, rejected writes, underruns, overflows, discarded frames, and driver restarts.

## Build without installation

Run the read-only preflight and clean unsigned package build from the repository root:

```powershell
driver\windows\scripts\preflight.ps1
driver\windows\scripts\verify-runtime-access-policy.ps1
driver\windows\scripts\verify-upstream.ps1 -CheckoutRoot .tools\sysvad-upstream
driver\windows\scripts\build-validation.ps1
.tools\cargo-webrtc.cmd build -p mini-aec-sender -p mini-aec-lab --release
```

The driver script removes only ignored build output below `driver/windows`, rebuilds x64 Debug, verifies the INF contains `MiniAEC Microphone` and no render category, and writes an ignored package to `driver/windows/out/validation-x64-debug/`. The package contains the INF, SYS, and an unsigned CAT prepared by WDK Inf2Cat. It creates no certificate and makes no driver, device, boot, or certificate-store change.

## Normal-user runtime validation

Preview the separate runtime-only harness at any privilege level:

```powershell
driver\windows\scripts\runtime-access-validation.ps1 -Action Plan
```

After an administrator has installed and activated only the separately approved development package, open an ordinary non-elevated interactive PowerShell. The harness refuses elevated and non-interactive tokens, records token elevation/session metadata under ignored `artifacts/`, reads and verifies the installed control-device DACL, and never invokes signing, installation, device restart, uninstall, boot or default-role commands:

```powershell
driver\windows\scripts\runtime-access-validation.ps1 -Action Identity
driver\windows\scripts\runtime-access-validation.ps1 -Action Transport
driver\windows\scripts\runtime-access-validation.ps1 -Action Contention
driver\windows\scripts\runtime-access-validation.ps1 -Action Bypass -MicrophoneId "<exact-physical-capture-endpoint-id>" -DurationSeconds 300
driver\windows\scripts\runtime-access-validation.ps1 -Action Aec -MicrophoneId "<exact-physical-capture-endpoint-id>" -RenderId "<exact-physical-render-endpoint-id>" -DurationSeconds 300
```

`Transport` opens one session, writes one deterministic frame, reads diagnostics and closes without requiring a capture client to drain the ring. `Contention` holds one probe handle, requires a second process to report busy, and then proves fresh reconnection after owner exit. Bypass and AEC continue to require exact endpoint IDs and retain metadata-only evidence; Windows Recorder and the target meeting application may be operated separately for listening and continuity acceptance.

Long-run M4 validation may use `long-run-validation.ps1` to automate looped `ffplay` render activity, continuous FFmpeg DirectShow consumption of `MiniAEC Microphone`, private FLAC capture, the same non-elevated `Aec` action, child-process cleanup and schema version 2 analysis. Preview it with `-PlanOnly`; pass exact endpoint IDs and explicit FFmpeg/ffplay paths for a real run. Machine-verified process coverage does not replace the required truthful listening observation over the captured output. The exact clock-analysis, operator-observation, final 30-minute duration gate, evidence-triggered reassessment and privacy rules are documented in [`docs/long-run-audio-stability.md`](../../docs/long-run-audio-stability.md). This orchestration adds no lifecycle mutation and does not authorize installation, restart or rollback commands.

## Lifecycle safety boundary

Preview the exact local paths and planned mutations:

```powershell
driver\windows\scripts\validation-lifecycle.ps1 -Action Plan
```

Save a read-only endpoint, default-role, PnP, driver-package, certificate, Secure Boot, and boot-configuration inventory:

```powershell
.tools\cargo-webrtc.cmd build -p mini-aec-lab
driver\windows\scripts\validation-lifecycle.ps1 -Action Inventory
```

Every signing, install, restart, and uninstall action prints the same plan and refuses to proceed without `-ConfirmSystemChanges`. Do not supply that switch until the user has explicitly approved the reviewed commands and rollback plan in [`VALIDATION.md`](VALIDATION.md).

The M3 headless command, metadata fields, acoustic scenarios, failure interpretation, and its additional approval boundary are documented in [`docs/realtime-aec-validation.md`](../../docs/realtime-aec-validation.md). Endpoint enumeration and ordinary engine commands are read-only with respect to driver/device state; they do not replace the lifecycle inventory, plan, explicit approval, or rollback verification.

The endpoint is intentionally eligible for selection as the Windows system default input. Windows may assign one or more default input roles to `MiniAEC Microphone` when the package is installed; this Windows-originated change is accepted only when the before/after roles are recorded. Uninstall must verify that Windows restores the saved roles automatically or stop, report the exact difference, and obtain separate approval before restoring them. DevCon restart and package removal can leave a driver restart or service deletion pending until a full Windows reboot; the lifecycle script never performs that reboot itself. Agents must never initiate, schedule, or invoke a restart, shutdown, or sign-out command; they must stop and let the user save work and perform any required restart manually.

Test signing is only a local development mechanism. A distributable MiniAEC build requires a separately approved production certificate, package/installer design, upgrade policy, Windows compatibility validation, and the applicable Microsoft signing route.

The production lifecycle contract and non-mutating package preflight are documented in [`docs/production-driver-lifecycle.md`](../../docs/production-driver-lifecycle.md). The production build, external signing boundary, canonical replay rules and public evidence layout are documented in [`docs/production-driver-package.md`](../../docs/production-driver-package.md). The `validation-x64-debug` output, `MiniAECValidation` identity, test certificate and TESTSIGNING state remain development-only and must never be presented as a production release package.
