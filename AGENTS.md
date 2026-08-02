# MiniAEC agent guide

## Required reading

Before changing audio capture, synchronization, AEC, dependencies, the virtual driver, or validation code, read:

1. `docs/technical-plan.md`
2. `docs/aec-baseline.md`
3. `vendor/UPSTREAM.md`
4. `docs/upstream-upgrade-plan.md`
5. `driver/windows/README.md` for driver work
6. `openspec/config.yaml` and the applicable generated OpenSpec skill for SDD work

## Product contract

- The product name is `MiniAEC`; Rust/package identifiers use `mini-aec`.
- The supported platform is Windows 11 x64.
- Tauri 2 is a windowless Rust system-tray host. Do not add a WebView, React, TypeScript, Vite, Bun, or another settings frontend unless explicitly approved.
- MiniAEC performs AEC only. Do not add noise suppression, automatic gain control, equalization, dereverberation, or voice enhancement to its signal path.
- The first usable product milestone must ship a project-owned virtual capture endpoint named `MiniAEC Microphone`. A third-party virtual cable is not a supported product dependency.
- User-space product code is Rust. The Windows audio driver may use the C/C++ required by the WDK and Microsoft samples.
- The tray shell alone is an engineering baseline, not a usable release.

## Current project state

- The React/Bun/WebView scaffold has been removed.
- The Tauri tray shell compiles and creates no application window.
- Dual WASAPI capture and QPC-aligned offline AEC are retained in `mini-aec-lab`.
- The active AEC baseline is FreeDesktop `webrtc-audio-processing 2.1`, based on WebRTC M131, through Rust `webrtc-audio-processing 2.1.0`.
- The active configuration is the upstream AEC3 default. Previous suppression tuning profiles, blind comparison generation, and linear-output diagnostic bridge were removed during the product pivot. Their history is available in Git but they are not current directions.
- The virtual microphone, real-time engine, installer, and driver signing flow are not implemented.
- OpenSpec is initialized with the `spec-driven` schema and Codex integration. Use its generated workflow for future changes, but do not create a change unless the user asks to start or specify one.

## AEC dependency rules

- Treat the current M131 AEC3 snapshot as frozen until an upgrade task is explicitly approved.
- Do not track or copy Google WebRTC `main` during ordinary feature work.
- Do not update the Rust wrapper, FreeDesktop source snapshot, or vendored build layer without following `docs/upstream-upgrade-plan.md`.
- Keep MiniAEC changes out of `webrtc/modules/audio_processing/aec3/` whenever possible. Record every local vendor patch in `vendor/UPSTREAM.md`.
- Product code must depend on a replaceable project-owned `EchoCanceller` boundary. WebRTC-specific types stay inside its adapter.
- Do not reintroduce product-facing AEC tuning profiles until the real-time `MiniAEC Microphone` path is stable and identical-input evidence establishes a specific default-baseline failure.

## Driver rules

- Start from a pinned Microsoft SysVAD snapshot with exact commit, source URL, licenses, and local changes recorded before importing code.
- Keep the public capture endpoint name fixed as `MiniAEC Microphone`.
- Test signing is development-only. A distributable build requires an approved production signing and installer strategy.
- Never install, update, or remove a driver without explicit user approval and a rollback path.

## System restart safety

- Never initiate, schedule, or invoke an operating-system restart, shutdown, or sign-out command. If a workflow requires one, explain why and stop so the user can save their work and perform the action manually.
- Prior approval for a broader validation, installation, rollback, or recovery plan does not authorize an agent-initiated restart, shutdown, or sign-out. Only the user performs these actions, even when they are required to continue the approved plan.

## Validation and privacy

- Algorithm changes require old/new processing of identical inputs. Compare far-end removal, convergence, double-talk voice preservation, runtime, and failure behavior; suppression alone is insufficient.
- Validate the product path end-to-end through `MiniAEC Microphone`, not only a WAV produced by the lab.
- `artifacts/` contains private local recordings. Never stage, commit, upload, or delete it unless the user explicitly requests that exact action.
- Commit only redistributable synthetic or public material under `testdata/`, with source and license recorded.

## Documentation style

- Do not hard-wrap prose to a fixed column width. Keep each paragraph, list item, requirement description, scenario step, and task on one source line unless Markdown or YAML syntax requires a break.
- Use line breaks only for semantic paragraph boundaries, headings, lists, tables, code blocks, and other structural markup.
- This rule applies to project documentation and OpenSpec artifacts. Do not reformat generated OpenSpec skills or third-party vendored documentation.

## Checks

- Format: `cargo fmt --all -- --check`
- Tests: `.tools\cargo-webrtc.cmd test --workspace`
- Lints: `.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings`
- The bundled WebRTC build requires an x64 Visual Studio C++ environment plus Meson, Ninja, and libclang; see `README.md`.
- There are no frontend checks.
