# MiniAEC

MiniAEC is a Windows 11 x64 hands-free acoustic echo canceller. Its product path captures a physical microphone together with the actual speaker render loopback, removes loudspeaker echo with WebRTC AEC3, and publishes the result through the bundled `MiniAEC Microphone` capture endpoint.

MiniAEC intentionally stops at AEC. Noise suppression, automatic gain control, equalization, and voice enhancement belong after `MiniAEC Microphone`, for example in NVIDIA Broadcast or the selected meeting application.

## Current status

- The application is a windowless Tauri 2 Rust process with a Windows tray menu. When exact endpoint IDs are supplied through `MINI_AEC_MICROPHONE_ID` and `MINI_AEC_RENDER_ID`, the tray can explicitly start default AEC, switch to bypass, restart, stop, and display stopped, starting, running, degraded, bypass, or failed state without moving PCM or WebRTC work into Tauri.
- `mini-aec-lab` can enumerate Windows endpoints, capture a physical microphone and WASAPI render loopback together, align them by QPC timestamps, and run the frozen default WebRTC AEC3 baseline offline.
- The M1 virtual-microphone transport is validated end to end: the pinned SysVAD-derived development driver exposes one selectable `MiniAEC Microphone`, accepts fixed 10 ms PCM16 frames through the private adapter, isolates sender sessions, survives the validated restart cases, and rolls back cleanly.
- M2 real-time bypass is validated on the elevated development path. The Tauri-independent `mini-aec-engine` captures one explicit physical microphone through event-driven WASAPI, normalizes and frames it, and sends it to `MiniAEC Microphone` through a bounded four-frame queue. The accepted run covered five-minute continuity, stop/start isolation, sender contention, device restart, and complete rollback.
- The active M3 change implements a real-time default-AEC path with exact physical microphone and render-loopback IDs, bounded QPC alignment, a project-owned `EchoCanceller` boundary around the frozen default M131 adapter, AEC-specific lifecycle states, metadata-only diagnostics, a headless command, and tray control. Automated synthetic tests pass. A separately approved elevated run validated Windows Recorder and Discord consumption, effective far-end removal including louder playback, natural near-end-only speech, render-silence recovery, sender contention, and stop/start isolation, but double-talk had obvious near-end swallowing and therefore fails the acceptance requirement. Final read-only inventory after the user-performed restart verified complete rollback of the validation device, endpoint, package, certificates, service, default roles and TESTSIGNING state. M3 is not accepted as a usable product milestone because the double-talk requirement remains unmet.
- Drift compensation remains M4 work and must be driven by measured long-run clock error. Normal-user driver access, installation, production signing, upgrade and uninstall remain M5 work.
- OpenSpec uses the `spec-driven` schema. The accepted M1 and M2 changes are archived and their capabilities are synced to the main specs. The active change is `implement-realtime-default-aec`; its separately approved Windows and acoustic acceptance group remains open.

See [docs/technical-plan.md](docs/technical-plan.md) for the architecture and delivery gates, [docs/aec-baseline.md](docs/aec-baseline.md) for the frozen algorithm baseline, [docs/realtime-aec-validation.md](docs/realtime-aec-validation.md) for the M3 validation procedure, and [driver/windows/README.md](driver/windows/README.md) for the development driver boundary. AEC provenance and upgrade rules live in [vendor/UPSTREAM.md](vendor/UPSTREAM.md) and [docs/upstream-upgrade-plan.md](docs/upstream-upgrade-plan.md).

## Repository layout

```text
src-tauri/                 windowless Tauri tray host
crates/mini-aec-lab/       capture and offline AEC validation CLI
crates/mini-aec-engine/    Tauri-independent real-time engine and Windows capture adapter
crates/mini-aec-sender/    deterministic virtual microphone validation sender
crates/mini-aec-windows-transport/ private Windows driver adapter
driver/windows/            MiniAEC Microphone driver boundary and provenance
docs/                      active architecture and validation contracts
vendor/                    pinned Windows build layer for WebRTC AEC3
artifacts/                 ignored private local recordings
```

The real-time engine remains independent from Tauri and exposes project-owned audio-input, render-input, synchronizer, echo-canceller, and virtual-sink boundaries instead of CLI, WASAPI, driver, or WebRTC types. The frozen WebRTC adapter is replaceable behind `EchoCanceller`; its concrete types do not enter engine configuration, snapshots, or tray code.

## Tray application

Compile the tray host with:

```powershell
cargo check -p mini-aec
```

For an explicitly configured AEC run, set the exact physical endpoint IDs and start the tray host:

```powershell
$env:MINI_AEC_MICROPHONE_ID = "<exact-physical-capture-endpoint-id>"
$env:MINI_AEC_RENDER_ID = "<exact-physical-render-endpoint-id>"
cargo run -p mini-aec
```

The process creates no WebView or application window. Right-click the tray icon to select AEC or bypass, restart, inspect state, stop audio, or exit. These environment variables are a development configuration surface, not settings persistence or device auto-follow.

## Diagnostic CLI

List Windows audio endpoints and shared-mode formats:

```powershell
cargo run -p mini-aec-lab -- devices
```

Capture a physical microphone and render-loopback reference together:

```powershell
cargo run -p mini-aec-lab -- capture --duration 30 --microphone "<physical microphone name or endpoint ID>" --render "<physical speaker name or endpoint ID>"
```

Capture artifacts are written below `artifacts/runs/` and ignored by Git. When another virtual microphone is the Windows default, select the physical device explicitly so the source is not preprocessed.

Align a capture by WASAPI/QPC timestamps and process it through the default WebRTC M131 AEC3 configuration:

```powershell
cargo run -p mini-aec-lab -- aec --run artifacts/runs/<run-id>
```

To compare a fixed acoustic delay hint, add `--stream-delay-ms 60`. Output is written below `processed/aec-default-adaptive/` or `processed/aec-default-delay-<N>ms/`.

## Real-time validation

First list endpoints and copy the exact physical microphone and render IDs; real-time commands do not follow Windows defaults or accept friendly-name selectors:

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- devices --json
```

With an already installed and separately approved development validation driver, run default AEC from an Administrator PowerShell:

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- realtime-aec `
  --microphone-id "<exact-physical-capture-endpoint-id>" `
  --render-id "<exact-physical-render-endpoint-id>" `
  --duration 300
```

The command records one metadata-only `engine.jsonl` below ignored `driver/windows/out/validation/engine-aec/`. It contains endpoint identity and format metadata, QPC alignment, queue, AEC, processing-time, sink, state, degradation, and failure summaries; it contains no PCM. The command accepts no AEC tuning parameters and exits nonzero on terminal engine failure.

Explicit M2 bypass remains available:

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- bypass `
  --microphone-id "<exact-physical-capture-endpoint-id>" `
  --duration 300
```

Neither command changes BCD, certificates, drivers, devices, or Windows default audio roles. Both reject `MiniAEC Microphone` as their own capture source. The current driver control DACL makes this an elevated development-only validation path; it is not the later normal-user tray or production installation design.

Bypass is an explicit mode, not a fallback for an AEC failure. Invalid AEC output is silenced while bounded reconstruction is attempted; exhausted recovery, sustained synchronization failure, input invalidation, or sink failure closes the run. Private Windows Recorder files remain outside version control under ignored local paths. The offline `aec` command is a diagnostic baseline and is not product-path acceptance.

## Bundled WebRTC build on Windows

The safe Rust wrapper is pinned to `webrtc-audio-processing 2.1.0` from crates.io. The `webrtc-audio-processing-sys` crate and FreeDesktop M131 source snapshot remain vendored for reproducible MSVC adaptations.

Build from an x64 Visual Studio Developer PowerShell with the C++ build tools, Meson, Ninja, and libclang available. This workspace keeps a repository-relative helper at `.tools/cargo-webrtc.cmd`; Cargo/Meson builds can be resumed with `.tools/resume-ninja.cmd` without depending on a stale build-directory hash.

## Checks

```powershell
cargo fmt --all -- --check
.tools\cargo-webrtc.cmd test --workspace
.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings
```

There are no frontend checks or Node/Bun project dependencies.
