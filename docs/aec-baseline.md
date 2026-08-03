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

Both the offline lab path and real-time `DefaultEchoCanceller` adapter create the processor with `Processor::new(48_000)` and enable full echo cancellation through the stable high-level configuration. They do not enable `experimental-aec3-config`, expose a tuning profile, or export the linear pre-suppressor signal. The real-time adapter leaves stream delay unset, reuses adapter-owned channel buffers, submits render before capture, and rejects non-finite output behind the project-owned `EchoCanceller` boundary.

The exact source chain and Windows build adaptations are recorded in [`vendor/UPSTREAM.md`](../vendor/UPSTREAM.md).

## What remains proven

Earlier work established reusable engineering facts:

- A physical microphone and physical render endpoint can be captured together through WASAPI.
- The retained capture manifest contains first-packet QPC timestamps for both tracks.
- The two tracks can be placed on a common 48 kHz timeline and processed as 10 ms frames.
- The M131 default AEC3 path can converge and remove intelligible far-end video speech in the local K7 microphone and Sound Blaster X4 speaker setup.

These facts justify retaining the capture, alignment, reporting, and default AEC processing code.

## Current product-path status

The M1 `MiniAEC Microphone` transport and M2 real-time physical-microphone bypass are validated on the approved elevated development path. The driver accepts continuous fixed-format user-mode PCM, ordinary Windows capture clients can consume the public endpoint, and `mini-aec-engine` can carry one explicitly selected physical microphone through bounded normalization, framing, queueing and sink-session lifecycle without stale-frame replay.

The active M3 change implements two explicitly role-checked WASAPI inputs, capture-paced QPC pairing, the frozen default real-time adapter, `RunningAec`/`Degraded`/`Failed` behavior, metadata-only JSONL evidence, a headless `realtime-aec` command, and tray AEC/bypass control. Synthetic engine, adapter, CLI, and tray tests validate the repository behavior without installing a driver. A separately approved elevated run exercised the installed transport through Windows Recorder and Discord. Far-end removal remained effective at the tested louder playback level, near-end-only speech was natural, and render silence/recovery had no audible stale replay or discontinuity. Double-talk remained understandable but had obvious near-end swallowing, so the desired double-talk quality target did not pass; M3 records that frozen-default limitation while accepting the separately verified functional path. Final read-only inventory after the user-performed restart verified complete rollback of the validation device, endpoint, package, certificates, service, default roles and TESTSIGNING state.

The implemented synchronization policy uses two eight-frame latest-wins queues, 5 ms pairing tolerance, a 100 ms maximum skew observation, 50 consecutive timestamp-bearing unpairable intervals before terminal synchronization failure, and ten healthy pairs before degraded recovery. An active render endpoint may legally provide no loopback packets while playback is silent; those intervals use counted silent references and remain visibly degraded without terminating or switching to bypass. Invalid AEC frames are silenced and the adapter is reconstructed; three consecutive processing failures terminate the run. This is bounded startup/recovery behavior, not long-run hardware-clock drift correction.

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
3. Implemented and automated in M3: the real-time engine supplies an explicitly selected physical render loopback to the frozen default AEC3 adapter and aligns it with microphone frames on a bounded QPC timeline.
4. Completed in M3 validation: Windows Recorder and Discord consumed the AEC output through `MiniAEC Microphone`; the scored client intervals had no reported unexplained discontinuity or stale replay.
5. Completed in M3 quality characterization: far-end-only, near-end-only and render silence/recovery met their listening targets; double-talk remained understandable but had obvious near-end swallowing and is recorded as a deferred default-algorithm quality limitation.

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

Keep the frozen M131 default configuration for the completed M3 path. Double-talk near-end swallowing is a known quality limitation rather than unfinished M3 implementation; any future algorithm experiment requires a separate approved change, identical-input old/new evidence, and preservation of the demonstrated far-end removal.

M3 metadata collects timestamp delta, buffer depth, discontinuity, underrun and bounded processing evidence, but it does not claim asynchronous resampling or sustained clock-error correction. That belongs to M4 after measurements establish its direction and required control range. Normal-user driver access, installation and production signing remain separate M5 concerns.
