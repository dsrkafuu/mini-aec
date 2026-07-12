# Open Denoise agent guide

## Required reading

Before changing audio capture, synchronization, AEC, dependencies, or validation code, read:

1. `docs/technical-plan.md`
2. `docs/aec-baseline.md`
3. `vendor/UPSTREAM.md`
4. `docs/upstream-upgrade-plan.md`

## Current project state

- Windows 11 x64, Rust, Tauri 2, React, and TypeScript are the initial target.
- M1 dual WASAPI capture is complete.
- M2 QPC-aligned offline WebRTC AEC3 is complete.
- The current AEC baseline is FreeDesktop `webrtc-audio-processing 2.1`, based on
  WebRTC M131, through Rust `webrtc-audio-processing 2.1.0`.
- The next validation gate is controlled double-talk, followed by a longer run
  that measures microphone/render clock drift.

## AEC dependency rules

- Treat the current M131 AEC3 snapshot as frozen until an upgrade task is
  explicitly approved.
- Do not track or copy Google WebRTC `main` directly during ordinary feature or
  bug-fix work.
- Do not update the Rust wrapper, FreeDesktop source snapshot, or vendored AEC3
  files without following `docs/upstream-upgrade-plan.md`.
- Keep Open Denoise changes out of `webrtc/modules/audio_processing/aec3/` when
  possible. Maintain Windows build adaptations in the wrapper/build layer and
  record every local patch in `vendor/UPSTREAM.md`.
- Product code must depend on a replaceable project-owned `EchoCanceller`
  boundary rather than exposing WebRTC-specific types outside the adapter.

## Validation and privacy

- Algorithm changes require old/new processing of the same inputs. Compare
  far-end echo reduction, convergence, double-talk voice preservation, runtime,
  and failure behavior; a higher suppression number alone is not sufficient.
- `artifacts/` contains private local recordings. Never stage, commit, upload,
  or delete them unless the user explicitly requests that action.
- Commit only redistributable synthetic or public test material under
  `testdata/`, with source and license recorded.
- Keep AEC, independent noise suppression, and gain control isolated while
  establishing baselines.

## Checks

- Rust: `cargo fmt --all -- --check`, workspace tests, and strict Clippy.
- The bundled WebRTC build on Windows requires the x64 Visual Studio C++
  environment plus Meson, Ninja, and libclang; see `README.md`.
- Frontend: `bun run format:check`, `bun run lint`, and `bun run build`.
