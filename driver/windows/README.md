# MiniAEC Microphone driver boundary

This directory contains the Windows 11 x64 validation driver for the single public capture endpoint `MiniAEC Microphone`. It is derived from the pinned Microsoft SysVAD snapshot recorded in [`UPSTREAM.md`](UPSTREAM.md); the validation package is development-only and is not a production installer or signing path.

## Selected transport

The driver exposes one ordinary audio capture endpoint and one private non-audio control device named `MiniAECTransport`. User-mode product code depends on the project-owned `VirtualMicrophoneSink` trait; only `mini-aec-windows-transport` knows the IOCTL and binary layouts.

The private WaveRT render-sink candidate was rejected because an active render endpoint that WASAPI can open also participates in the Windows audio endpoint model and cannot reliably be both usable and absent from ordinary application enumeration. The selected control interface does not create a producer-facing audio endpoint and does not map driver memory into user mode.

The validation DACL grants access only to SYSTEM and Administrators. One open control handle owns the sender slot. Closing the handle or terminating its process releases the slot, closes the session, and flushes all buffered PCM.

## Fixed protocol and buffer

Protocol version 1 accepts 48 kHz mono signed PCM16 in exact 10 ms frames: 480 samples and 960 payload bytes. Every open, write, diagnostics, and close request carries fixed lengths; session operations carry a nonzero 128-bit session identity, and writes carry a sequence that starts at zero and increases by one.

The driver owns a synchronized nonpaged ring with 10 complete-frame slots, equal to 100 ms or 9,600 PCM bytes. User mode cannot map the ring or publish partial frames. WaveRT capture consumes according to the audio clock. An empty ring produces fresh zero-valued silence; a full ring discards the oldest unread complete frame before accepting the newest. A new session atomically removes all previous-session audio.

Diagnostics schema 2 reports session state and identity, last accepted sequence, current depth, high-water mark, session opens/closes/resets, accepted frames, rejected writes, underruns, overflows, discarded frames, and driver restarts.

## Build without installation

Run the read-only preflight and clean unsigned package build from the repository root:

```powershell
driver\windows\scripts\preflight.ps1
driver\windows\scripts\verify-upstream.ps1 -CheckoutRoot .tools\sysvad-upstream
driver\windows\scripts\build-validation.ps1
.tools\cargo-webrtc.cmd build -p mini-aec-sender --release
```

The driver script removes only ignored build output below `driver/windows`, rebuilds x64 Debug, verifies the INF contains `MiniAEC Microphone` and no render category, and writes an ignored package to `driver/windows/out/validation-x64-debug/`. The package contains the INF, SYS, and an unsigned CAT prepared by WDK Inf2Cat. It creates no certificate and makes no driver, device, boot, or certificate-store change.

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

The endpoint is intentionally eligible for selection as the Windows system default input. Windows may assign one or more default input roles to `MiniAEC Microphone` when the package is installed; this Windows-originated change is accepted only when the before/after roles are recorded. Uninstall must verify that Windows restores the saved roles automatically or stop, report the exact difference, and obtain separate approval before restoring them. DevCon restart and package removal can leave a driver restart or service deletion pending until a full Windows reboot; the lifecycle script never performs that reboot itself.

Test signing is only a local development mechanism. A distributable MiniAEC build requires a separately approved production certificate, package/installer design, upgrade policy, Windows compatibility validation, and the applicable Microsoft signing route.
