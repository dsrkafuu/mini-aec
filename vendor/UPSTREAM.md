# WebRTC AEC upstream provenance

This file is the source of truth for the AEC implementation used by MiniAEC. Update it in the same commit as any upstream pin or local patch change.

## Current pin

| Layer | Pin | Source | Purpose |
| --- | --- | --- | --- |
| Rust API | `webrtc-audio-processing 2.1.0` | crates.io | Safe Rust processor API |
| Rust configuration | `webrtc-audio-processing-config 2.1.0` | crates.io | Stable APM configuration types |
| Rust FFI/build | `webrtc-audio-processing-sys 2.1.0` | `vendor/webrtc-audio-processing-sys` | C++ bridge and reproducible Windows build |
| Rust upstream commit | `c14d7af1760baff83e8210fee336a0cae0faaa7d` | published package `.cargo_vcs_info.json` | Wrapper provenance |
| C++ distribution | FreeDesktop `webrtc-audio-processing 2.1` | bundled inside the vendored `-sys` crate | Distribution-oriented APM source and Meson build |
| Google algorithm baseline | WebRTC M131 | FreeDesktop release metadata | APM and AEC3 implementation |

The high-level Rust wrapper is not vendored or locally modified. Cargo locks it to 2.1.0 and patches only `webrtc-audio-processing-sys` to the project-owned Windows build layer.

The published package does not retain the nested FreeDesktop Git metadata, so the exact FreeDesktop and Google WebRTC commits cannot be reconstructed from this snapshot. M131 is the strongest available algorithm pin. A future refresh must record exact commits before integration.

## Source chain

```text
Google WebRTC M131 modules/audio_processing/aec3
  -> FreeDesktop webrtc-audio-processing 2.1 extraction and Meson packaging
  -> tonarino webrtc-audio-processing-sys 2.1.0 C++ bridge
  -> crates.io webrtc-audio-processing 2.1.0 safe Rust API
  -> MiniAEC timestamp alignment and 10 ms adapter
```

The actual echo-cancellation algorithm lives under:

```text
vendor/webrtc-audio-processing-sys/webrtc-audio-processing/
  webrtc/modules/audio_processing/aec3/
```

MiniAEC does not modify those AEC3 algorithm files.

## MiniAEC local changes

Local changes are confined to the vendored `webrtc-audio-processing-sys` build/wrapper layer:

1. Build bundled WebRTC as C++20 with MSVC because the source uses designated initializers rejected by MSVC in C++17 mode.
2. Build the wrapper as C++20 on MSVC and avoid GCC-only warning flags.
3. Define compatibility macros for bindgen/libclang parsing of current Visual Studio headers.
4. Disable archive symbol prefixing on MSVC. LLVM objcopy does not reliably rewrite the wrapper archive's MSVC C++ undefined references, and MiniAEC links only one WebRTC major version.
5. Link the MSVC static archive with Cargo's verbatim, non-bundled native library syntax so the final executable retains it.
6. Copy bundled sources with Rust `fs_extra` instead of requiring Unix `cp` on Windows.
7. Define `WEBRTC_WIN` and `NOMINMAX` for the standalone wrapper and bindgen pass so Windows headers select compatible paths and avoid `min`/`max` macro collisions.

These changes adapt build and linkage only. The former linear AEC getter and high-level experimental configuration changes were removed when the product returned to the upstream-default AEC3 baseline.

## Licenses

- Rust API wrapper: BSD-3-Clause, distributed by crates.io.
- Vendored FFI/build wrapper: BSD-3-Clause; see `vendor/webrtc-audio-processing-sys/COPYING`.
- FreeDesktop package: BSD-style license; see its bundled `COPYING`.
- Google WebRTC: BSD-style license and accompanying `PATENTS` grant under the vendored WebRTC root.
- Third-party components retain their own license files in the source tree.

Do not remove license, patent, authorship, or third-party notice files when refreshing the vendor tree.

## Update policy

Do not update this snapshot merely because Google WebRTC `main` changed. Start an upgrade only when a trigger in `docs/upstream-upgrade-plan.md` applies, and accept it only after the complete identical-input regression gate passes.

Preferred source order:

1. A stable FreeDesktop release with a known WebRTC milestone and exact commit.
2. A matching stable Rust wrapper release.
3. A project-owned Google WebRTC snapshot only when a measured product blocker has an upstream fix unavailable through the stable packaging chain and the added maintenance scope is explicitly approved.
