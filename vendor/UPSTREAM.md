# WebRTC AEC upstream provenance

This file is the source of truth for the AEC implementation vendored by Open
Denoise. Update it in the same commit as any upstream or local patch change.

## Current pin

| Layer                     | Pin                                        | Source                                              | Purpose                                          |
| ------------------------- | ------------------------------------------ | --------------------------------------------------- | ------------------------------------------------ |
| Rust API                  | `webrtc-audio-processing 2.1.0`            | crates.io / `tonarino/webrtc-audio-processing`      | Safe Rust processor API                          |
| Rust configuration        | `webrtc-audio-processing-config 2.1.0`     | crates.io / same repository                         | APM configuration types                          |
| Rust FFI/build            | `webrtc-audio-processing-sys 2.1.0`        | vendored below `vendor/webrtc-audio-processing-sys` | C++ bridge and bundled build                     |
| Rust upstream commit      | `c14d7af1760baff83e8210fee336a0cae0faaa7d` | package `.cargo_vcs_info.json`                      | Published wrapper source provenance              |
| C++ distribution          | FreeDesktop `webrtc-audio-processing 2.1`  | bundled inside the `-sys` crate                     | Distribution-oriented APM source and Meson build |
| Google algorithm baseline | WebRTC M131                                | recorded by FreeDesktop release notes               | APM and AEC3 implementation                      |

The published package records the Rust repository commit but does not retain
the nested FreeDesktop Git metadata. Therefore an exact FreeDesktop commit and
Google WebRTC commit cannot be recovered from this package alone. M131 is the
strongest available algorithm pin for the current snapshot. The next upstream
refresh must record both exact commit IDs here before integration.

Cargo uses a tilde requirement for the high-level wrapper and locks it to
2.1.0. The workspace patches only `webrtc-audio-processing-sys` to the local
vendored directory so Windows build adaptations are reproducible.

## Source chain

```text
Google WebRTC M131 modules/audio_processing/aec3
  -> FreeDesktop webrtc-audio-processing 2.1 extraction and Meson packaging
  -> tonarino Rust wrapper 2.1.0 and C++ bridge
  -> Open Denoise timestamp alignment and 10 ms frame adapter
```

The actual echo cancellation algorithm lives under:

```text
vendor/webrtc-audio-processing-sys/webrtc-audio-processing/
  webrtc/modules/audio_processing/aec3/
```

Open Denoise does not currently modify those AEC3 algorithm files.

## Open Denoise local changes

Local changes are confined to
`vendor/webrtc-audio-processing-sys/build.rs` unless this file says otherwise:

1. Build bundled WebRTC as C++20 with MSVC because the source uses designated
   initializers rejected by MSVC in C++17 mode.
2. Build the wrapper itself as C++20 on MSVC and avoid GCC-only warning flags.
3. Define compatibility macros for bindgen/libclang parsing of current Visual
   Studio headers.
4. Disable archive symbol prefixing on MSVC. LLVM objcopy does not reliably
   rewrite the wrapper archive's MSVC C++ undefined references, and Open Denoise
   links only one WebRTC major version.
5. Link the MSVC static WebRTC archive with Cargo's verbatim, non-bundled native
   library syntax so the final executable retains it.
6. Copy bundled sources with Rust `fs_extra` rather than requiring Unix `cp` on
   Windows.

These are build and linkage adaptations, not AEC behavior changes.

## Licenses

- Rust wrapper: BSD-3-Clause; see
  `vendor/webrtc-audio-processing-sys/COPYING`.
- FreeDesktop package: BSD-style license; see its `COPYING` file.
- Google WebRTC: BSD-style license plus the accompanying `PATENTS` grant; see
  the files below the vendored WebRTC root.
- Third-party components retain their own license files in the source tree.

Do not remove upstream license, patent, authorship, or third-party notice files
when refreshing the vendor tree.

## Update policy

Do not update this snapshot merely because Google WebRTC `main` changed. Start
an upgrade only when one of the triggers in
`docs/upstream-upgrade-plan.md` applies, and accept it only after the complete
old/new regression gate passes.

Preferred source order:

1. A stable FreeDesktop `webrtc-audio-processing` release with a known WebRTC
   milestone and exact commits.
2. A matching stable Rust wrapper release.
3. A project-owned Google WebRTC snapshot only when a measured product blocker
   has an upstream fix unavailable through the stable packaging chain and the
   additional maintenance scope is explicitly approved.
