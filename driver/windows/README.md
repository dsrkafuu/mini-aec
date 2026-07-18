# MiniAEC Microphone driver boundary

This directory is reserved for the project-owned Windows virtual audio driver that exposes the public capture endpoint `MiniAEC Microphone`.

No driver source has been imported yet. The repository baseline intentionally stops before choosing or copying an upstream snapshot so that the first driver change can record its specification, exact source, license, build, signing, installation, and rollback contract together.

## Required upstream record

Before importing code, record:

- Microsoft Windows Driver Samples repository URL;
- exact SysVAD commit and sample path;
- WDK and Visual Studio versions used to build it;
- all applicable license and notice files;
- every removed endpoint and every MiniAEC-specific modification;
- test-signing setup and its removal procedure.

Do not copy a moving branch without an exact commit.

## Public device contract

- Public capture endpoint name: `MiniAEC Microphone`.
- Initial format target: mono, 48 kHz PCM suitable for the user-mode 10 ms engine contract.
- User-mode disconnect or underrun produces silence, never repeated stale audio.
- Driver communication uses bounded buffers and the minimum access rights.
- Installation, upgrade, and removal must be reversible.
- A third-party virtual cable is not part of the supported architecture.

## First spike

The first implementation change must compare the smallest credible SysVAD data path options rather than assume one:

1. a private WaveRT render sink forwarded to the capture endpoint;
2. a restricted control-device/shared-ring bridge.

Feed deterministic audio from a small user-mode harness, then verify capture with Windows Recorder and at least one meeting application. Measure continuity, latency, buffer behavior, process restart, driver restart, and uninstall. Do not connect WebRTC AEC3 until this transport works independently.

Test-signed drivers are development artifacts and must not be described as a release path. Production signing and any HLK/attestation requirements remain a separate distribution milestone.
