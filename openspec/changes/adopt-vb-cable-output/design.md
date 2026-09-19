# Design

## Context

See [proposal.md](proposal.md) for the product decision. The repository currently treats a pinned SysVAD-derived capture endpoint, private IOCTL transport, production INF/SYS/CAT package and signed driver lifecycle as the path to `MiniAEC Microphone`. The audio engine and frozen WebRTC M131/default-AEC3 adapter are already isolated behind project-owned boundaries, but current specifications, diagnostics, validation commands and documentation assume that the final sink is the project-owned driver.

VB-Audio documents VB-CABLE as a Windows virtual cable that forwards audio sent to its playback side (`CABLE Input`) to its recording side (`CABLE Output`). Its official installation requires an administrator-run vendor setup and may require a user-performed restart. VB-Audio's licensing guidance explicitly presents direct user installation from the official site as the simplest no-bundling integration route. MiniAEC therefore treats VB-CABLE as an external user-owned prerequisite rather than a vendored library or redistributable package. References: [VB-CABLE product page](https://vb-audio.com/Cable/) and [VB-Audio licensing guidance](https://vb-audio.com/Services/licensing.htm).

The migration must not alter the AEC algorithm, add a settings frontend, install a driver, change boot configuration or delete historical validation evidence. The repository has unfinished `production-driver-package` and `production-driver-lifecycle` changes plus implementation work associated with that route; they must be retired coherently without merging their future production-driver requirements into the active specs.

## Goals / Non-Goals

**Goals:**

- Preserve the project-owned tray, engine, WASAPI input, synchronizer and `EchoCanceller` boundaries while replacing only the final output transport and the contracts coupled to it.
- Keep processed audio bounded and low-latency across the additional WASAPI render clock domain.
- Make endpoint identity, signal direction, failure ownership and user actions explicit enough to prevent feedback loops and accidental system mutation.
- Separate the documentation-first contract migration from later code implementation, runtime acceptance and legacy cleanup.

**Non-Goals:**

- Supporting arbitrary virtual cables, Voicemeeter or a pluggable catalog of third-party routing products in this change.
- Shipping both the SysVAD route and VB-CABLE as supported product modes.
- Automating VB-CABLE download, setup, licensing, update, removal, restart or Windows default-role selection.
- Renaming vendor endpoints or hiding the VB-Audio dependency behind MiniAEC branding.
- Revalidating or tuning AEC3 unless the unchanged algorithm fails because of an output-path defect demonstrated with identical input evidence.

## Decisions

### 1. VB-CABLE is the sole supported product output, not a temporary fallback

The supported route becomes:

```text
physical microphone + physical speaker loopback
                    -> bounded synchronization
                    -> frozen default WebRTC M131 AEC3
                    -> MiniAEC VB-CABLE output session
                    -> CABLE Input (playback)
                    -> VB-CABLE driver
                    -> CABLE Output (recording)
                    -> Recorder / meeting application
```

This makes the third-party boundary visible and testable. The existing `VirtualMicrophoneSink` abstraction may be renamed to a neutral output abstraction during implementation, but WebRTC, WASAPI and vendor-specific types remain outside the engine contract.

Alternatives considered: retaining the self-owned driver as an optional production path keeps signing, installer and compatibility work alive and contradicts the product decision; supporting multiple cable vendors expands discovery, support and validation scope before one route is reliable.

### 2. Users install VB-CABLE directly from the official source

MiniAEC documentation links to the official vendor download and explains the donationware/external-license boundary. Product code performs read-only endpoint preflight only. It does not ship vendor binaries, invoke vendor setup, request elevation, accept a license on the user's behalf or initiate a restart.

Alternatives considered: bundling the driver would reintroduce licensing, installer, signing-chain and lifecycle obligations; downloading on demand would create supply-chain, consent and elevation responsibilities that are unnecessary for the first product route.

### 3. Endpoint selection uses exact IDs plus corroborating identity and role checks

Runtime configuration stores the exact Windows endpoint IDs for the selected `CABLE Input` playback side and paired `CABLE Output` recording side. Discovery enumerates active endpoints, verifies render-versus-capture data flow and corroborates VB-CABLE identity from available endpoint/device metadata. Friendly names are displayed for diagnosis but are never the sole selection key. If discovery cannot produce one unambiguous pair, startup fails and the user must select explicit IDs through a headless/tray-compatible flow; no default endpoint is substituted.

The resolver also compares the selected pair with physical input roles. `CABLE Output` cannot be the physical microphone, and `CABLE Input` cannot be the physical render-loopback reference, because either choice would feed processed output back into capture or AEC reference.

Alternatives considered: matching only `CABLE Input`/`CABLE Output` is fragile under localization, renaming and multiple installed cable products; following Windows defaults can silently change the signal topology.

### 4. The output backend is event-driven shared-mode WASAPI render

The engine continues to emit complete 10 ms 48 kHz mono finite frames. A dedicated output adapter opens the selected `CABLE Input` playback endpoint in event-driven shared mode, negotiates the active mix format, and performs deterministic channel/sample-format conversion at the boundary when the endpoint does not accept the engine representation directly. Sample-rate conversion is performed only when required by the negotiated format and is reset with every output session.

The real-time engine does not call COM/WASAPI rendering directly. Capture and AEC workers submit complete frames to a bounded output queue; an output worker responds to render events, writes only available frames and reports clock, padding, underrun, conversion and failure metadata. The existing four-frame freshest-audio queue remains the initial latency bound unless profiling demonstrates that the event period requires a different explicit bound; any change to that bound must remain documented and tested.

Alternatives considered: exclusive mode is unnecessarily disruptive and less compatible with a general virtual cable; relying on implicit undocumented format conversion obscures duration and channel behavior; rendering from the AEC worker couples sink scheduling to algorithm deadlines.

### 5. Output failure is terminal for the current run

Missing or ambiguous endpoints, render initialization failure, endpoint invalidation, unrecoverable write failure or sustained bounded-queue failure closes the current output session, stops source capture, clears partial/queued/converted PCM and enters `Failed`. MiniAEC never falls back to speakers, a default device, raw microphone output or another cable. Recovery requires an explicit start that re-runs pair preflight and creates fresh output/conversion state.

Stopping MiniAEC closes its render stream and guarantees only that MiniAEC submits no more or stale PCM. Silence behavior after the stream closes is owned by the installed VB-CABLE version and is verified observationally during acceptance rather than represented as project-owned driver behavior.

### 6. Product acceptance moves to `CABLE Output`

Synthetic tests verify discovery, role rejection, format conversion, bounded backpressure, stale-state clearing and lifecycle behavior without requiring a driver. Real Windows acceptance requires an already installed vendor pair, renders bypass and AEC output to `CABLE Input`, and consumes `CABLE Output` through Windows Recorder plus at least one target meeting application. Existing acoustic scenarios and the 30-minute stability methodology remain, but evidence adds output endpoint identities, negotiated format and render counters. The 30-minute gate separates functional stability from clock-drift characterization: a complete run with continuous ordinary-client consumption may pass the functional gate through render-silent intervals when all bounded recovery and output counters are explained, while drift remains explicitly inconclusive when active-render coverage is below the analyzer threshold and no clock-drift compensation claim is made.

Validation never installs or removes VB-CABLE and never changes Windows default roles. The user explicitly selects `CABLE Output` in each target application. Raw recordings and machine-specific endpoint identities remain under ignored `artifacts/`.

### 7. Legacy driver material is removed only after equivalent acceptance

The documentation/specification phase immediately removes the self-owned production driver from the active contract and marks the two unfinished production changes as superseded without spec sync. The implementation phase first adds and validates the VB-CABLE backend behind the existing output boundary. Only after bypass, AEC, failure/restart and long-run acceptance pass is the obsolete SysVAD transport, driver build/signing scripts, `mini-aec-release` production package code and associated active documentation removed from the working tree and workspace.

Git history and archived OpenSpec records remain the audit trail. The pinned upstream notices stay until the last redistributed SysVAD-derived source is removed; the cleanup must verify that no retained file still requires the MS-PL notice before deleting it.

Alternatives considered: deleting the legacy implementation before the replacement can be exercised removes useful comparison and rollback evidence; retaining it indefinitely leaves misleading build paths and repository weight.

## Risks / Trade-offs

- [External dependency availability or vendor behavior changes] -> Pin and document the accepted VB-CABLE package/version during implementation, detect endpoint/format assumptions at preflight and require a new compatibility check before claiming support for later versions.
- [Users confuse `CABLE Input` and `CABLE Output`] -> Show the complete direction in docs, tray status and diagnostics: MiniAEC writes `CABLE Input`; applications record `CABLE Output`.
- [Endpoint names are renamed or localized] -> Persist exact endpoint IDs, corroborate role/vendor metadata and fail closed when pairing is ambiguous.
- [Additional render clock introduces underrun or drift symptoms] -> Keep a finite event-driven queue, record render clock/padding/underrun evidence and repeat the existing 30-minute gate before legacy cleanup.
- [VB-CABLE forwards audio from unrelated applications] -> Document that `CABLE Input` is a shared external playback endpoint and recommend reserving the selected pair for MiniAEC while it is running; MiniAEC cannot enforce exclusive ownership in shared mode.
- [No MiniAEC-branded microphone appears in Windows] -> Treat `CABLE Output` naming as an explicit product trade-off and never claim vendor endpoint ownership.
- [Donationware or distribution terms change] -> Link to the official source, do not redistribute, and require a separate reviewed change before any bundling or commercial volume-license workflow.
- [Historical production-driver artifacts are mistaken for current direction] -> Remove them from active docs and changes, label retained evidence historical, and remove implementation/build entry points after replacement acceptance.

## Migration Plan

1. Documentation and specification phase: update active product docs, OpenSpec context and current specs to the VB-CABLE contract; add official prerequisite/licensing links; remove production driver signing from the roadmap; archive `production-driver-package` and `production-driver-lifecycle` as superseded without syncing their unfinished deltas. Make no Rust/C++ or system changes.
2. Output implementation phase: add endpoint enumeration/pair preflight, a bounded event-driven WASAPI render adapter and metadata diagnostics behind the project-owned output boundary; update tray/headless configuration without adding a WebView.
3. Automated verification phase: add synthetic pair-resolution, feedback rejection, conversion, queue, failure and fresh-session tests; keep all automated checks non-mutating.
4. Runtime acceptance phase: after separate user authorization and manual VB-CABLE installation, validate bypass, default AEC, Recorder/meeting-client consumption, stop/restart, invalidation and the separate functional/clock-drift 30-minute gates without modifying driver lifecycle or default roles.
5. Legacy cleanup phase: remove the SysVAD source/import tooling, MiniAEC driver transport, production INF/SYS/CAT/signing/lifecycle code, obsolete release crate paths and active driver docs; verify licenses, workspace membership, references and repository size before completion.
6. OpenSpec completion phase: sync the VB-CABLE deltas only after implementation and acceptance are complete, then archive this change. Historical archived changes and Git history remain untouched.

## Rollback Strategy

Before runtime acceptance, revert only the VB-CABLE implementation changes while retaining the approved documentation decision; the project returns to a non-product development state rather than restoring the abandoned production-driver roadmap. Before legacy cleanup, the old source remains recoverable in the working tree and Git for comparison. After cleanup, any forensic need uses Git history or archived OpenSpec evidence; rollback does not authorize reinstalling the development driver, enabling test mode or restarting Windows.
