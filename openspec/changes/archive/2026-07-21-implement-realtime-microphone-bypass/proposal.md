## Why

The virtual microphone transport is validated, but MiniAEC still has no real-time product engine: the existing lab captures into files and the tray host has no audio path. A physical-microphone bypass is the smallest end-to-end step that can validate device capture, format normalization, 10 ms scheduling, virtual-microphone output and recovery before render loopback and AEC3 add synchronization and algorithm risk.

## What Changes

### Goals

- Add a reusable Rust `mini-aec-engine` crate that owns real-time audio lifecycle independently from Tauri.
- Capture one explicitly selected physical Windows microphone through WASAPI and prevent selecting `MiniAEC Microphone` as its own source.
- Normalize the source into finite 48 kHz mono samples, assemble exact 10 ms frames, convert to PCM16 and submit them through the existing project-owned `VirtualMicrophoneSink` boundary.
- Define bounded real-time behavior for startup, steady-state capture, source underrun, backpressure, driver absence, sender contention, device invalidation, stop and restart.
- Expose project-owned engine commands and snapshots suitable for a headless validation harness and later tray integration.
- Validate continuous bypass recording through `MiniAEC Microphone`, sender isolation, source or sink loss, restart and stale-audio prevention without committing private recordings.
- Update top-level status documentation to record the completed virtual-microphone spike and the new M2 boundary.

### Non-goals

- Do not capture render loopback, align two device clocks, compensate drift or run WebRTC AEC3.
- Do not add noise suppression, automatic gain control, EQ, dereverberation or voice enhancement.
- Do not connect the engine to Tauri tray controls, add a WebView or implement startup-at-login behavior.
- Do not change the driver control protocol, public endpoint model, SysVAD baseline, test-signing workflow, installer or production signing strategy.
- Do not add a silent raw-microphone fallback for future AEC failures; this change is an explicitly identified bypass milestone rather than an AEC degradation policy.

## Capabilities

### New Capabilities

- `realtime-audio-engine`: Defines the independent engine lifecycle, physical microphone selection and capture, bounded normalization and framing, virtual microphone bypass output, status reporting and recovery behavior.

### Modified Capabilities

None. The existing `virtual-microphone-transport` and `driver-development-lifecycle` contracts remain unchanged and are consumed as validated dependencies.

## Impact

- Adds `crates/mini-aec-engine/` and may extract reusable Windows WASAPI device and capture adapters from `mini-aec-lab` without turning the lab's file-writing orchestration into a real-time dependency.
- Adds a headless engine validation entry point and tests for lifecycle, framing, bounded queues, error mapping, restart and stale-frame isolation.
- Uses the existing `mini-aec-transport` contract and Windows driver adapter; no WDK source, INF, public endpoint, IOCTL layout or driver signing behavior changes are planned.
- End-to-end validation will require the already documented, separately approved test-signed driver lifecycle, but ordinary unit and integration tests must not install a driver or change BCD, certificates, devices or default audio policy.
- Keeps private audio under ignored local evidence paths and commits only synthetic or metadata-only validation material.
