# Proposal

## Why

MiniAEC will not invest in a project-owned production Windows audio driver or production driver signing for the foreseeable future, so the current release path cannot become a distributable product. The product needs a complete output-path pivot to a user-installed VB-CABLE dependency while preserving the existing AEC scope, frozen WebRTC baseline and windowless Rust architecture.

## What Changes

- **BREAKING** Replace the bundled `MiniAEC Microphone` capture endpoint with VB-CABLE: MiniAEC renders processed PCM to the VB-CABLE playback endpoint (`CABLE Input`), and target applications capture the forwarded stream from the VB-CABLE recording endpoint (`CABLE Output`).
- **BREAKING** Remove the project-owned SysVAD driver, production INF/SYS/CAT packaging, signing, installation, upgrade, rollback and uninstall route from the active product contract and roadmap.
- Require users to obtain, install and manage VB-CABLE directly from the official VB-Audio source as an external prerequisite; MiniAEC will not bundle, redistribute, silently install or uninstall it under this change.
- Add deterministic discovery, selection, preflight and failure behavior for the paired VB-CABLE endpoints, including rejection of missing, ambiguous or feedback-producing configurations.
- Preserve the existing bounded real-time engine, AEC-only scope, frozen WebRTC M131/default-AEC3 baseline, metadata-only diagnostics and user-controlled system-restart boundary.
- Rebase end-to-end and long-run acceptance on ordinary clients consuming `CABLE Output`, and retain historical SysVAD validation records as historical evidence rather than active product requirements.
- Retire the unfinished `production-driver-package` and `production-driver-lifecycle` changes as superseded without syncing their production-driver requirements into the main specifications.

## Goals

- Establish VB-CABLE as MiniAEC's sole supported virtual-audio output dependency for the foreseeable product roadmap.
- Make the supported audio route and its ownership boundaries unambiguous across active documentation and specifications.
- Define a future implementation and verification plan that does not require MiniAEC to own, sign or distribute a Windows kernel driver.

## Non-goals

- Implementing the VB-CABLE sink, endpoint discovery or repository cleanup in this planning step.
- Bundling, redistributing, licensing, installing, updating or uninstalling VB-CABLE on the user's behalf.
- Renaming VB-CABLE endpoints or presenting `CABLE Output` as a project-owned `MiniAEC Microphone` device.
- Changing the AEC algorithm, adding noise suppression, gain control, equalization, dereverberation or voice enhancement.
- Deleting archived OpenSpec changes or rewriting historical development-driver validation evidence.

## Capabilities

### New Capabilities

- `vb-cable-output`: Defines the external prerequisite, paired endpoint selection, WASAPI render contract, bounded output behavior, lifecycle ownership and ordinary-client acceptance for VB-CABLE.

### Modified Capabilities

- `virtual-microphone-transport`: Removes the project-owned SysVAD endpoint, private producer protocol, driver ring and driver restart requirements because that transport is retired from the product.
- `driver-development-lifecycle`: Removes the development-driver build, test-signing, installation and rollback capability from the active product specification while preserving its completed history in archived changes and Git.
- `realtime-audio-engine`: Replaces the private driver sink assumptions with an explicit VB-CABLE render sink and rejects VB-CABLE endpoints as physical source roles where they would create a feedback loop.
- `realtime-echo-cancellation`: Moves end-to-end AEC acceptance from `MiniAEC Microphone` to client consumption of `CABLE Output` without changing the frozen algorithm baseline.
- `long-run-audio-stability`: Moves the long-run output and client-consumption evidence from the project-owned endpoint to the VB-CABLE route and separates external prerequisite health from engine stability.

## Impact

- Active product documentation: `README.md`, `AGENTS.md`, `docs/technical-plan.md`, AEC and long-run validation documents, OpenSpec context and the current main specifications.
- Future Rust implementation: the `VirtualMicrophoneSink` backend, endpoint discovery/configuration, WASAPI render output, diagnostics, tray status and validation commands.
- Legacy repository surface: `driver/windows`, production driver build/signing scripts and `mini-aec-release` become migration cleanup targets after equivalent VB-CABLE product validation exists.
- Distribution: VB-CABLE remains separately installed and owned by the user; MiniAEC provides an official-source prerequisite link and actionable preflight only. Any future bundling or redistribution requires a separate licensing and distribution decision.
- Compatibility: target applications must select `CABLE Output` as their microphone, and MiniAEC must not claim to provide a capture endpoint named `MiniAEC Microphone`.
