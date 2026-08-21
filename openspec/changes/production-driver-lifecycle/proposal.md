## Why

MiniAEC has a validated development-only virtual microphone, real-time default-AEC path, normal-user runtime access policy, and long-run stability evidence, but it is not yet a distributable Windows product because production signing, package lifecycle, upgrade behavior, rollback, and compatibility boundaries remain undefined. M5 is needed now to turn the accepted engineering path into an auditable product lifecycle without changing the frozen M131 AEC baseline or reopening the completed M1-M4 audio contracts.

## What Changes

- Define a production release lifecycle for the `MiniAEC Microphone` driver package and the windowless Rust/Tauri runtime, including install, first activation, upgrade, rollback, uninstall, and failure-recovery behavior.
- Establish the production signing and package identity boundary separately from the existing development test-signing workflow.
- Define how package versions, driver/runtime compatibility, protocol compatibility, and interrupted lifecycle operations are detected and reported.
- Define the security boundary for the private `MiniAECTransport` producer interface in a distributable installation, including whether direct interactive access is sufficient or a trusted broker/service boundary is required.
- Preserve the public endpoint name `MiniAEC Microphone`, the fixed PCM/protocol contract, the replaceable sink boundary, and the existing non-elevated runtime behavior unless a documented production security decision requires a scoped change.
- Define a Windows 11 x64 compatibility matrix covering endpoint enumeration, Windows Recorder, target meeting applications, default-input roles, suspend/resume or device restart behavior, upgrade, and complete rollback.
- Keep development validation, test signing, system changes, and operating-system restart actions explicitly separated from the production release process; the agent must not initiate restart, shutdown, sign-out, installation, or rollback actions.
- Do not change WebRTC dependencies, AEC3 configuration, synchronization policy, PCM format, or introduce a settings WebView as part of this change.

## Capabilities

### New Capabilities

- `production-driver-lifecycle`: Production signing, packaging, installation, upgrade, rollback, uninstall, runtime trust boundary, and Windows compatibility acceptance for a distributable MiniAEC product.

### Modified Capabilities

None. The existing `driver-development-lifecycle` capability remains the auditable development-only test-signing and validation contract; this change defines the separate production lifecycle rather than weakening or replacing it.

## Impact

- Affects Windows driver packaging and INF/CAT/signing inputs, the Rust/Tauri distribution boundary, lifecycle and rollback scripts, release documentation, and the compatibility-validation harness.
- May require a new installer or package-management component and a production trust/broker boundary; the selected technologies and external signing route are design decisions for this change.
- Requires explicit Windows 11 x64 acceptance with before/after inventories, package identity evidence, default-input role preservation, ordinary-user runtime checks, target-client compatibility results, and rollback evidence.
- Does not change the active `webrtc-audio-processing 2.1.0` / WebRTC M131 baseline, the M3 bounded synchronizer, the M4 stability analyzer, or private recording policy.
