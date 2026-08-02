## Why

M1 proved the project-owned `MiniAEC Microphone` transport and M2 proved a bounded real-time physical-microphone bypass, but MiniAEC still does not cancel echo on its real-time product path. The next highest-value step is to combine an explicitly selected physical microphone and physical render loopback on a common bounded timeline, run the frozen upstream-default M131 AEC3 baseline, and validate the result through the public virtual microphone endpoint.

## What Changes

### Goals

- Extend the real-time engine configuration and lifecycle to require explicit physical capture and render endpoint identities for an AEC-enabled run while preserving explicit, visible bypass as a separate user-selected mode.
- Capture the selected physical render endpoint through WASAPI loopback, normalize both streams into the existing 48 kHz and 10 ms internal contract, and align them on a bounded QPC-based timeline.
- Add a replaceable project-owned `EchoCanceller` boundary whose initial adapter runs the frozen `webrtc-audio-processing 2.1.0` / WebRTC M131 full echo canceller with upstream-default AEC3 parameters and no NS, AGC or product post-processing.
- Define render-reference shortage, timestamp discontinuity, AEC reset, non-finite output, source invalidation and terminal failure behavior without silently leaking raw microphone audio.
- Extend metadata-only snapshots and validation evidence with render, alignment, AEC timing, reset, underrun and degraded-state diagnostics while keeping PCM and meeting content out of logs.
- Validate far-end-only, near-end-only, double-talk, render-silence and restart scenarios end to end through `MiniAEC Microphone`, including consumption by Windows Recorder and at least one target meeting application.
- Connect the completed engine states to the existing windowless tray control surface without moving PCM or WebRTC work into Tauri.

### Non-goals

- Do not tune AEC3, reintroduce product-facing suppression profiles, expose experimental wrapper configuration or upgrade the frozen WebRTC/FreeDesktop/Rust dependency chain.
- Do not add asynchronous resampling or claim long-run clock-drift closure; M3 records bounded drift evidence and leaves sustained correction and long-run gates to M4.
- Do not add noise suppression, gain control, equalization, dereverberation, voice enhancement or a WebView/settings frontend.
- Do not change the virtual microphone IOCTL protocol, public endpoint name, driver ring behavior, SysVAD baseline, control DACL, test-signing lifecycle, installer or production-signing strategy.
- Do not automatically follow Windows default devices, select fallback endpoints, install or remove a driver, change Windows default audio roles, or commit private recordings.

## Capabilities

### New Capabilities

- `realtime-echo-cancellation`: Defines explicit render-loopback capture, bounded microphone/render synchronization, the project-owned echo-canceller contract, frozen default M131 AEC3 processing, safe degraded behavior, AEC diagnostics and end-to-end acoustic acceptance through `MiniAEC Microphone`.

### Modified Capabilities

- `realtime-audio-engine`: Extends the existing bypass-only engine requirements with an AEC-enabled configuration, additional lifecycle states, two explicit physical endpoints, AEC-aware failure isolation and tray-visible state while retaining explicit bypass semantics and stale-audio prevention.

## Impact

- Primarily affects `crates/mini-aec-engine/` contracts, runtime, test support and Windows WASAPI adapters, plus the headless validation surface in `crates/mini-aec-lab/` and low-frequency state/control integration in `src-tauri/`.
- Adds a project-owned real-time adapter around the already pinned `webrtc-audio-processing 2.1.0` dependency; no dependency version, vendored algorithm source or local vendor patch is expected to change.
- Reuses `mini-aec-transport`, `mini-aec-windows-transport` and the validated driver protocol without changing their public requirements or WDK implementation.
- Automated verification remains synthetic and non-mutating. Approved machine validation requires the existing development driver lifecycle and elevated producer path, stores private recordings only under ignored local paths, and must fully roll back separately authorized driver state.
- Updates README, architecture, AEC-baseline and validation documentation to distinguish completed real-time AEC behavior from deferred M4 drift correction and M5 distribution work.
