# MiniAEC

MiniAEC is a Windows 11 x64 tray app for hands-free acoustic echo cancellation.

[简体中文](README.zh.md)

## What it does

MiniAEC takes a physical microphone and a physical speaker/render reference, processes the microphone signal with AEC3, and writes the result to VB-CABLE:

```text
Physical microphone + speaker reference
                 -> MiniAEC AEC
                 -> CABLE Input -> CABLE Output -> recording or meeting app
```

The app runs from the Windows tray. Device choices and AEC state are applied automatically; there is no settings window or manual save step.

## Before you start

- Windows 11 x64.
- VB-CABLE installed and managed by you from the [official VB-Audio source](https://vb-audio.com/Cable/).
- Your recording or meeting app configured to read `CABLE Output`.

MiniAEC does not ship, download, install, update, remove, authorize, or rename VB-CABLE, and it never requests a system restart. The release package does not contain the VB-CABLE installer or Windows driver files. If the required pair is unavailable, MiniAEC stays offline or reports an error instead of silently switching to another device.

## Quick start

1. Install VB-CABLE yourself and complete any Windows steps it requires.
2. Start MiniAEC and choose the physical microphone, physical output reference, and VB-CABLE pair from the tray menu. The first valid/default entries are available automatically.
3. Leave AEC disabled for bypass, or enable AEC3 when a physical output reference is available.
4. Select `CABLE Output` in the recording or meeting app.

For source builds and release packages, see [Windows release and development notes](docs/windows-release.md) and the [technical plan](docs/technical-plan.md).

## Scope and limits

- AEC only: no noise suppression, automatic gain control, equalization, de-reverberation, or voice enhancement.
- User-mode Rust/Tauri tray host; no MiniAEC-owned Windows audio driver.
- The current AEC3 baseline is frozen for reproducible development.
- The project does not provide a production code-signing certificate; release artifacts may be unsigned and Windows may show a warning.

## Documentation

- [AEC baseline](docs/aec-baseline.md)
- [Real-time validation](docs/realtime-aec-validation.md)
- [Long-run stability](docs/long-run-audio-stability.md)
- [Upstream upgrade plan](docs/upstream-upgrade-plan.md)
- [OpenSpec changes and product contracts](openspec/README.md)

Detailed WASAPI, endpoint identity, diagnostic, toolchain, and validation rules live in `docs/` rather than in this project overview.

## Development

The repository is a Rust workspace with a windowless Tauri 2 tray host. Build on Windows 11 x64 with Rust, Meson, and Ninja from mise, plus Visual Studio Build Tools with C++ desktop tools and LLVM/Clang. From the repository root, run `mise exec -- .\.tools\cargo-webrtc.cmd build --release`. See [Windows release and development notes](docs/windows-release.md) for other checks.
