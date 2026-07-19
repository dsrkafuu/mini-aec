# MiniAEC

MiniAEC is a Windows 11 x64 hands-free acoustic echo canceller. It captures a physical microphone together with the actual speaker render loopback, removes the loudspeaker echo with WebRTC AEC3, and will publish the processed signal as the bundled `MiniAEC Microphone` capture endpoint.

MiniAEC intentionally stops at AEC. Noise suppression, automatic gain control, equalization, and voice enhancement belong after `MiniAEC Microphone`, for example in NVIDIA Broadcast or the selected meeting application.

## Current status

- The application is a windowless Tauri 2 Rust process with a Windows tray menu. The audio-related entries are disabled until the real-time engine is connected.
- `mini-aec-lab` can enumerate Windows endpoints, capture a physical microphone and WASAPI render loopback together, align them by QPC timestamps, and run the frozen default WebRTC AEC3 baseline offline.
- A pinned SysVAD-derived validation driver, private control-interface adapter, and deterministic PCM sender now build without installing anything. The end-to-end `MiniAEC Microphone` path still requires approved test-signing and Windows Recorder validation, and the real-time engine is not implemented.
- OpenSpec is initialized with the `spec-driven` schema and Codex integration. The active `validate-virtual-microphone-transport` change tracks the first driver data-path validation.

See [docs/technical-plan.md](docs/technical-plan.md) for the architecture and delivery gates. AEC provenance and upgrade rules live in [vendor/UPSTREAM.md](vendor/UPSTREAM.md) and [docs/upstream-upgrade-plan.md](docs/upstream-upgrade-plan.md).

## Repository layout

```text
src-tauri/                 windowless Tauri tray host
crates/mini-aec-lab/       capture and offline AEC validation CLI
crates/mini-aec-sender/    deterministic virtual microphone validation sender
crates/mini-aec-windows-transport/ private Windows driver adapter
driver/windows/            MiniAEC Microphone driver boundary and provenance
docs/                      active architecture and validation contracts
vendor/                    pinned Windows build layer for WebRTC AEC3
artifacts/                 ignored private local recordings
```

The future real-time engine belongs in `crates/mini-aec-engine/`. It must remain independent from Tauri and expose project-owned audio and `EchoCanceller` boundaries instead of WebRTC-specific types.

## Tray application

Compile the tray host with:

```powershell
cargo check -p mini-aec
```

Run it with:

```powershell
cargo run -p mini-aec
```

The process creates no WebView or application window. Right-click the tray icon to inspect the scaffolded status and exit the process.

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

## Bundled WebRTC build on Windows

The safe Rust wrapper is pinned to `webrtc-audio-processing 2.1.0` from crates.io. The `webrtc-audio-processing-sys` crate and FreeDesktop M131 source snapshot remain vendored for reproducible MSVC adaptations.

Build from an x64 Visual Studio Developer PowerShell with the C++ build tools, Meson, Ninja, and libclang available. This workspace also keeps a local helper at `.tools/cargo-webrtc.cmd` on configured development machines.

## Checks

```powershell
cargo fmt --all -- --check
.tools\cargo-webrtc.cmd test --workspace
.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings
```

There are no frontend checks or Node/Bun project dependencies.
