# MiniAEC

MiniAEC is a Windows 11 x64 hands-free acoustic echo canceller. It captures an explicitly selected physical microphone and physical-speaker loopback, processes them with the frozen default WebRTC M131 AEC3 baseline, and renders the processed stream into a separately installed VB-CABLE pair.

```text
physical microphone + physical speaker loopback -> MiniAEC AEC -> CABLE Input -> CABLE Output -> Recorder or meeting application
```

MiniAEC intentionally stops at AEC. Noise suppression, automatic gain control, equalization, dereverberation, and voice enhancement remain downstream responsibilities.

## Current status

- The application is a windowless Tauri 2 Rust process with a Windows tray menu. The engine, tray and headless commands now require exact VB-CABLE playback/recording IDs beside the physical microphone and render IDs; no endpoint is selected from Windows defaults.
- `mini-aec-lab` can enumerate Windows endpoints, capture a physical microphone and WASAPI render loopback together, align them by QPC timestamps, and run the frozen default WebRTC AEC3 baseline offline.
- Historical M1–M4 SysVAD runs validated bounded transport, bypass, default AEC, non-elevated runtime access, ordinary-client consumption, 30-minute stability, and complete rollback. Those measurements remain historical evidence and do not validate the new VB-CABLE output clock or product route.
- `production-driver-package` and `production-driver-lifecycle` are archived as superseded without syncing their unfinished production-driver requirements.
- OpenSpec change `adopt-vb-cable-output` owns the new user-mode WASAPI renderer, endpoint preflight, validation, and legacy cleanup. Until its Windows acceptance is complete, the repository remains an implementation in progress rather than a distributable product.
- Double-talk remains understandable but has known near-end swallowing under the frozen default algorithm; this migration does not tune AEC3.

VB-CABLE is an external user-owned prerequisite. Obtain and manage it from the [official VB-CABLE page](https://vb-audio.com/Cable/). MiniAEC does not bundle, redistribute, download, install, update, uninstall, or license it, and never initiates a Windows restart.

The initial Windows 11 x64 compatibility target is the official `VBCABLE_Driver_Pack45.zip` package dated October 2024. Its Windows 10/11 x64 INF (`vbMmeCable64_win10.inf`) identifies driver version `3.3.1.7`, dated 2024-10-07; the installed driver must be signed by Microsoft Windows Hardware Compatibility Publisher. The `1.0.3.5` value present in the package's legacy Windows INF files is not the Windows 11 x64 PnP driver version. This target is not a compatibility claim until the OpenSpec runtime matrix has passed on that exact package; later packages require a new compatibility check.

See [docs/technical-plan.md](docs/technical-plan.md), [docs/aec-baseline.md](docs/aec-baseline.md), [docs/realtime-aec-validation.md](docs/realtime-aec-validation.md), and [docs/long-run-audio-stability.md](docs/long-run-audio-stability.md). AEC provenance and upgrade rules live in [vendor/UPSTREAM.md](vendor/UPSTREAM.md) and [docs/upstream-upgrade-plan.md](docs/upstream-upgrade-plan.md).

## Repository layout

```text
src-tauri/                 windowless Tauri tray host
crates/mini-aec-lab/       capture and offline AEC validation CLI
crates/mini-aec-engine/    Tauri-independent real-time engine and Windows capture adapter
crates/mini-aec-output/    project-owned output-session contract
crates/mini-aec-windows-output/ VB-CABLE discovery and WASAPI renderer
docs/                      active architecture and validation contracts
vendor/                    pinned Windows build layer for WebRTC AEC3
artifacts/                 ignored private local evidence; never delete implicitly
```

The real-time engine remains independent from Tauri and exposes project-owned audio-input, render-input, synchronizer, echo-canceller, and output boundaries instead of CLI, WASAPI, VB-CABLE, or WebRTC types. The frozen WebRTC adapter remains replaceable behind `EchoCanceller`.

## Tray application

Compile the tray host with:

```powershell
cargo check -p mini-aec
```

For an explicitly configured AEC run, set the exact physical endpoint IDs and start the tray host:

```powershell
$env:MINI_AEC_MICROPHONE_ID = "<exact-physical-capture-endpoint-id>"
$env:MINI_AEC_RENDER_ID = "<exact-physical-render-endpoint-id>"
$env:MINI_AEC_CABLE_INPUT_ID = "<exact-vb-cable-playback-endpoint-id>"
$env:MINI_AEC_CABLE_OUTPUT_ID = "<exact-vb-cable-recording-endpoint-id>"
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

After the user has manually installed VB-CABLE from its official source, run the headless path with exact physical and VB-CABLE endpoint IDs. MiniAEC performs only endpoint preflight and ordinary audio runtime work; it does not invoke a driver installer or change system policy:

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- realtime-aec `
  --microphone-id "<exact-physical-capture-endpoint-id>" `
  --render-id "<exact-physical-render-endpoint-id>" `
  --cable-input-id "<exact-vb-cable-playback-endpoint-id>" `
  --cable-output-id "<exact-vb-cable-recording-endpoint-id>" `
  --duration 300
```

The command records metadata-only evidence below ignored `artifacts/`; it contains endpoint identity and format metadata, QPC alignment, queue, AEC, processing-time, output, state, degradation, and failure summaries but no PCM. It accepts no AEC tuning parameters and exits nonzero on terminal failure.

Explicit bypass remains available with the same VB-CABLE pair:

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- bypass `
  --microphone-id "<exact-physical-capture-endpoint-id>" `
  --cable-input-id "<exact-vb-cable-playback-endpoint-id>" `
  --cable-output-id "<exact-vb-cable-recording-endpoint-id>" `
  --duration 300
```

Both commands reject `CABLE Output` as the physical microphone and `CABLE Input` as the physical render reference. They do not change BCD, certificates, drivers, devices, or Windows default audio roles.

Analyze a completed schema version 3 VB-CABLE run without opening audio devices or changing Windows state. Retained schema v1/v2 SysVAD evidence remains readable as historical, non-authoritative input for the current gate:

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- stability-report `
  --events "artifacts\normal-user-access\<run>\engine-aec\<timestamp>\engine.jsonl" `
  --operator-observations "artifacts\normal-user-access\<run>\operator-observations.json" `
  --software-revision "<git-revision>"
```

The command keeps `stability-report.json` beside the private event stream by default. Missing operator observations produce an inconclusive functional result rather than a pass.

Bypass is an explicit mode, not a fallback for an AEC failure. Invalid AEC output is silenced while bounded reconstruction is attempted; exhausted recovery, sustained synchronization failure, input invalidation, or sink failure closes the run. Private Windows Recorder files remain outside version control under ignored local paths. The offline `aec` command is a diagnostic baseline and is not product-path acceptance.

## Bundled WebRTC build on Windows

The safe Rust wrapper is pinned to `webrtc-audio-processing 2.1.0` from crates.io. The `webrtc-audio-processing-sys` crate and FreeDesktop M131 source snapshot remain vendored for reproducible MSVC adaptations.

Build from an x64 Visual Studio Developer PowerShell with the C++ build tools, Meson, Ninja, and libclang available. This workspace keeps a repository-relative helper at `.tools/cargo-webrtc.cmd`; Cargo/Meson builds can be resumed with `.tools/resume-ninja.cmd` without depending on a stale build-directory hash.

VS Code uses the tracked `.vscode/settings.json` and `.tools/rust-analyzer.cmd` wrapper so rust-analyzer build-script loading and on-save checks inherit the same x64 Visual Studio, repository-local Meson/Ninja, and libclang environment. After cloning or changing this configuration, reload the VS Code window; the first native WebRTC analysis build may take several minutes. Only the project-owned root `.tools/*.cmd` entry points are tracked—downloaded tool runtimes below `.tools/` remain ignored.

## Checks

```powershell
cargo fmt --all -- --check
.tools\cargo-webrtc.cmd test --workspace
.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings
```

There are no frontend checks or Node/Bun project dependencies.
