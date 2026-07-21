# MiniAEC AEC baseline

## Active baseline

MiniAEC has one active acoustic echo cancellation baseline:

| Layer                      | Active value                                   |
| -------------------------- | ---------------------------------------------- |
| Rust API                   | `webrtc-audio-processing 2.1.0` from crates.io |
| Native build               | vendored `webrtc-audio-processing-sys 2.1.0`   |
| C++ distribution           | FreeDesktop `webrtc-audio-processing 2.1`      |
| Google algorithm milestone | WebRTC M131                                    |
| AEC mode                   | full echo canceller                            |
| AEC3 parameters            | upstream defaults                              |
| Noise suppression          | disabled                                       |
| Gain control               | disabled                                       |
| Product post-processing    | none                                           |

`mini-aec-lab` creates the processor with `Processor::new(48_000)` and enables full echo cancellation through the stable high-level configuration. It does not enable `experimental-aec3-config`, expose a tuning profile, or export the linear pre-suppressor signal.

The exact source chain and Windows build adaptations are recorded in [`vendor/UPSTREAM.md`](../vendor/UPSTREAM.md).

## What remains proven

Earlier work established reusable engineering facts:

- A physical microphone and physical render endpoint can be captured together through WASAPI.
- The retained capture manifest contains first-packet QPC timestamps for both tracks.
- The two tracks can be placed on a common 48 kHz timeline and processed as 10 ms frames.
- The M131 default AEC3 path can converge and remove intelligible far-end video speech in the local K7 microphone and Sound Blaster X4 speaker setup.

These facts justify retaining the capture, alignment, reporting, and default AEC processing code.

## Current product-path status

The M1 `MiniAEC Microphone` transport and M2 real-time physical-microphone bypass are now validated on the approved elevated development path. The driver accepts continuous fixed-format user-mode PCM, ordinary Windows capture clients can consume the public endpoint, and `mini-aec-engine` can carry one explicitly selected physical microphone through bounded normalization, framing, queueing and sink-session lifecycle without stale-frame replay.

Those results close the first two prerequisites below. They do not yet validate real-time echo cancellation: the engine still has one capture input, reports `RunningBypass`, does not capture a render-loopback reference and does not invoke WebRTC AEC3.

## What was reset

The old product context evaluated WAV files directly and led to experiments in near-end detector timing, suppression gain recovery, low/high-frequency near-end masking, blind A/B/C generation, and linear/full output comparison. Those experiments were useful for diagnosis, but their subjective ranking is not accepted as a product baseline after the signal chain changed to:

```text
physical microphone + physical render loopback
  -> MiniAEC default AEC
  -> MiniAEC Microphone
  -> optional downstream noise suppression
```

All product-facing profiles and diagnostic-only wrapper extensions have been removed. Their exact code and listening history remain available in Git history. Active documentation must not route future work back to them.

## Reproducing the offline baseline

List devices:

```powershell
cargo run -p mini-aec-lab -- devices
```

Capture a run:

```powershell
cargo run -p mini-aec-lab -- capture `
  --duration 30 `
  --microphone "K7" `
  --render "Sound Blaster X4"
```

Process it with AEC3 delay estimation:

```powershell
cargo run -p mini-aec-lab -- aec --run artifacts/runs/<run-id>
```

The command writes:

```text
processed/aec-default-adaptive/
├─ aligned-microphone.wav
├─ aligned-render-reference.wav
├─ aec-output.wav
└─ aec-report.json
```

The report schema is version 2 and records `webrtc-audio-processing 2.1 / WebRTC M131` with configuration `upstream-default`.

An explicit delay comparison remains available without changing AEC3 tuning:

```powershell
cargo run -p mini-aec-lab -- aec `
  --run artifacts/runs/<run-id> `
  --stream-delay-ms 60
```

## Real-time product validation gate

Offline WAV output is now a diagnostic, not the final acceptance surface. The default baseline must be judged again only after the following are working:

1. Completed in M1: a bundled `MiniAEC Microphone` endpoint receives continuous user-mode PCM.
2. Completed in M2: the physical microphone passes through that endpoint without AEC on the elevated development path.
3. Pending in M3: the real-time engine supplies an explicitly selected physical render loopback to the frozen default AEC3 adapter and aligns it with microphone frames on a bounded QPC timeline.
4. Pending in M3: Windows Recorder and at least one target meeting application consume the AEC output through `MiniAEC Microphone` without unexplained discontinuities.
5. Pending in M3: the same acoustic scenarios are assessed both directly and after the intended downstream noise suppressor.

The minimum acoustic matrix is:

| Scenario | Primary check |
| --- | --- |
| Far-end only | No intelligible returned video speech after convergence |
| Near-end only | Natural voice, intact starts and endings, stable level |
| Double-talk | Near-end remains understandable without obvious pumping or swallowing |
| Render silence | No unnecessary coloration of the physical microphone |
| Device restart | Bounded silence followed by clean recovery; no stale frames |
| Long run | Stable delay and no growing drift, underruns, or periodic artifacts |

Only after this end-to-end default baseline exposes a repeatable blocker may a new algorithm experiment be proposed. It must process identical inputs, change one mechanism, keep far-end removal as a gate, and evaluate near-end speech preservation before suppression metrics.

## Next action

Do not record another tuning sample or alter the frozen M131 baseline. The next highest-value capability is real-time default AEC3: extend `mini-aec-engine` with an explicit physical render-loopback input, bounded QPC-based synchronization, a project-owned `EchoCanceller` boundary and metadata-only AEC diagnostics, then validate the output end to end through `MiniAEC Microphone`.

M3 should collect timestamp delta, buffer depth, discontinuity, underrun and drift evidence, but it should not pre-emptively add asynchronous resampling. Sustained clock-error correction belongs to M4 after measurements establish its direction and required control range. Normal-user driver access, installation and production signing remain separate M5 concerns.
