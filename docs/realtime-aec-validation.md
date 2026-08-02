# Real-time default AEC validation

Status: implementation and automated verification are complete under OpenSpec change `implement-realtime-default-aec`. The separately approved elevated acceptance run completed acoustic listening plus Windows Recorder and Discord consumption. Far-end removal, near-end-only preservation and render silence/recovery met their checks, but double-talk had obvious near-end swallowing and fails the acceptance requirement. Final read-only inventory after the user-performed restart verified complete rollback of the validation device, endpoint, package, certificates, service, default roles and TESTSIGNING state.

## Scope

The M3 validation target is the real-time product path `physical microphone + physical render loopback -> bounded QPC synchronizer -> frozen upstream-default M131 AEC3 -> MiniAEC Microphone`. It does not validate tuning profiles, long-run drift correction, normal-user driver access, installer architecture, production signing, upgrade, or distribution.

The real-time adapter constructs `Processor::new(48_000)`, enables full echo cancellation, leaves the upstream AEC3 configuration at its default, and does not enable noise suppression, gain control, experimental configuration, equalization, dereverberation, or post-processing. Each paired render frame is submitted before its capture frame. WebRTC types remain inside the adapter behind the project-owned `EchoCanceller` boundary. The dependency and vendor pins in `vendor/UPSTREAM.md` are unchanged.

## Implemented bounds

- Audio contract: 48 kHz, mono, 10 ms, 480-sample frames.
- Microphone and render synchronization queues: eight frames each, latest-wins under pressure.
- Pairing tolerance: 5 ms on the shared QPC timeline.
- Maximum observed pairing skew before a frame is treated as unavailable: 100 ms.
- Render silence: an active loopback endpoint may provide no packets while playback is silent; capture continues with counted silent references in visible `Degraded` state without switching to bypass or terminating solely because no render timestamp exists.
- Sustained-skew failure window: 50 consecutive capture frames, approximately 500 ms, with timestamp-bearing render data that remains unpairable.
- Recovery gate: ten consecutive healthy paired frames.
- AEC reconstruction policy: silence the invalid output frame, reconstruct the adapter, and fail the run after three consecutive processing failures.
- Processing deadline: 10 ms, recorded as a degradation counter rather than hidden.

M3 does not claim sustained hardware-clock drift correction. If the bounded pairing policy cannot recover, the run enters `Failed`; asynchronous resampling or another measured long-run controller belongs to M4.

## Read-only preparation

Endpoint enumeration is read-only and can be run without installing a driver:

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- devices --json
```

Record the exact ID and reported shared-mode format for one physical capture endpoint and one physical render endpoint. Friendly-name matching, default-device fallback, and device auto-follow are intentionally unavailable in the real-time AEC command.

Before any machine-changing acceptance command, follow `driver/windows/README.md`: record the current machine inventory and rollback targets, run the lifecycle `Plan` action, identify the exact ignored validation package, then obtain explicit user approval for the build/install/restart/record/uninstall sequence. Ordinary `devices`, `realtime-aec`, `bypass`, tray, test, and lint commands never install, update, remove, enable, disable, or select a driver and never change Windows default audio roles.

## Headless command

After a separately approved development driver installation, run from the required elevated shell:

```powershell
.tools\cargo-webrtc.cmd run -p mini-aec-lab -- realtime-aec `
  --microphone-id "<exact-physical-capture-endpoint-id>" `
  --render-id "<exact-physical-render-endpoint-id>" `
  --duration 300
```

The current development driver control DACL permits SYSTEM and Administrators, so the product path remains an elevated validation path. Failure to open the driver as a normal user is a known M5 limitation, not proof that the AEC engine selected bypass.

The command creates `driver/windows/out/validation/engine-aec/<unix-ms>/engine.jsonl`, an ignored metadata-only file. It writes `started`, one-second `periodic`, `failed`, and `final` events outside the real-time workers. The snapshot includes explicit microphone and render descriptors, processing mode and lifecycle state, run/session/AEC instance identities, packet and frame counts, queue depth/high-water/overflow/discard counts, QPC origin and delta, maximum skew, paired/silent-reference/stale counts, synchronization epochs and resets, AEC processed/reset/rebuild/invalid-output counts, processing-time summaries and deadline misses, sink diagnostics, degradation reason, and the last actionable error. It contains no PCM or meeting content; endpoint IDs and friendly names are machine metadata and must not be committed.

## Acoustic scenarios

Use the same physical microphone, render endpoint, room geometry, loudspeaker level, and application routing for the complete run. Consume `MiniAEC Microphone` through Windows Recorder and at least one target meeting application. Keep all recordings under ignored local paths.

1. Far-end only: play speech through the selected render endpoint with no near-end speech; assess residual echo and convergence.
2. Near-end only: keep render silent and speak near the microphone; assess voice preservation and ensure the bounded silent-reference behavior is accounted for.
3. Double-talk: play far-end speech while speaking near the microphone; assess near-end preservation as well as far-end removal.
4. Render silence and recovery: interrupt and restore the reference within the bounded recovery window; confirm `Degraded` and recovery rather than bypass.
5. Failure and lifecycle: execute the approved stop/start, input restart, AEC reconstruction, sender-contention, and sink-failure cases; confirm every gap, reset, discard, underrun, overflow, rejected write, or terminal failure is explained by metadata.

Do not compare different algorithms or settings with different acoustic input recordings. Any future algorithm change requires old/new processing of identical inputs and separate evidence for far-end removal, convergence, double-talk voice preservation, runtime, and failure behavior.

## Interpretation and acceptance

`RunningAec` means both explicit inputs established the bounded timeline and the default adapter is producing frames. `Degraded` means output remains AEC-owned while a documented bounded recovery condition is active. `RunningBypass` can only result from explicit bypass selection. `Failed` is terminal for the current run and must never expose raw microphone audio as a fallback.

M3 acceptance requires the approved driver lifecycle to complete with verified rollback plus reviewable metadata and listening evidence for every scenario. Automated tests, endpoint enumeration, offline WAV output, or a successful headless process alone are insufficient. Until that acceptance is completed, documentation must continue to state that normal-user access, production signing, installer architecture, and long-run drift correction are unvalidated.

## Validation record

The separately approved 2026-07-24 elevated smoke run used the development validation package and the explicit physical K7 microphone and Sound Blaster X4 render roles. All generated JSONL, capture manifests and private WAV files remain under ignored `driver/windows/out/validation/` paths; endpoint IDs and recordings are not copied into this document.

The smoke evidence establishes the following engineering behavior:

- An independent WASAPI capture client consumed 20 seconds from `MiniAEC Microphone` while a 15-second AEC run completed. The engine accepted 1,502 output frames, including 1,484 timestamp-paired frames and 18 counted silent render references, with zero input-queue overflow, invalid AEC output, processing deadline miss, sink failure or terminal error.
- A render-silence run completed 1,500 AEC-owned output frames using 1,500 counted silent references, with no raw bypass transition, invalid AEC output, deadline miss or sink failure.
- A render interruption and recovery run completed 2,002 output frames, including 1,685 paired frames, 317 silent references, 19 stale render discards and eight accounted alignment/AEC resets. Maximum observed absolute skew was 19.595 ms; processing P99 was 250 µs and maximum processing time was 714 µs, with no invalid AEC output, deadline miss, sink failure or terminal error.
- A concurrent sender was rejected as `Busy` while the engine owned the transport, and two explicit five-second AEC runs completed with distinct run, AEC and sink-session identities and sequence restart behavior. The surrounding independent capture remained continuous for the planned stop/start interval.

This earlier run is structural and metadata evidence only. Its missing listening and target-client evidence was supplied by the later run below.

### 2026-08-02 acoustic, client and rollback run

The resumed approved run used the physical K7 microphone and Realtek speakers. Exact endpoint IDs, private Windows Recorder content, crash diagnostics and JSONL evidence remain only below ignored `driver/windows/out/validation/m3-acoustic-20260802/`; no PCM, endpoint ID or meeting content is copied into this document.

- A continuous Windows Recorder capture covered far-end-only, near-end-only, double-talk, render silence, render recovery and a fresh AEC session transition. The first 300-second engine session produced 30,003 frames and accepted 29,975 into the sink. It recorded 19,231 paired frames, 10,744 counted silent references, one stale render frame, three alignment/AEC resets, 26 microphone-queue latest-wins discards, 16 driver underruns and 4,680 driver latest-wins discards, with zero invalid AEC output, deadline miss, rejected write, sink failure or terminal error. Processing P99 was 250 microseconds and maximum was 694 microseconds.
- The following 90-second fresh recovery session used distinct run, session and AEC identities, produced 9,002 frames, accepted 8,999, and added no microphone-queue or driver overflow/discard. The cumulative driver underrun count increased by 11, with no invalid AEC output, deadline miss, rejected write, sink failure, stale replay or terminal error. The user reported no audible gap or stale segment across silence, recovery or the session transition.
- Listening found clear far-end removal after convergence, and a separate louder-playback check in Discord remained effective. Near-end-only speech was natural with intact starts and endings. Render silence/recovery and the AEC session transition were unobjectionable. Double-talk remained understandable but had obvious near-end swallowing, so it does not satisfy the requirement that near-end speech remain understandable without obvious swallowing or pumping. Task 9.3 and M3 acceptance remain open on this reproducible quality failure.
- Windows Recorder version 11.2605.1.0 crashed during initial device-selection/start attempts. One attempt coincided with Discord Mic Test holding the endpoint exclusively and produced `AUDCLNT_E_DEVICE_IN_USE`; another crash occurred after that test stopped, while a K7 control recording worked. The later clean, non-debug Recorder run completed and its full scenario recording was reported continuous on listening. This intermittent startup behavior remains a client-compatibility limitation even though the scored Recorder capture succeeded.
- Discord consumed `MiniAEC Microphone` during the final 134.9 seconds of a 300-second AEC run. Before Discord began consuming, cumulative latest-wins and underrun counters grew as expected for an unconsumed public endpoint; all microphone-queue overflow, driver overflow and driver underrun counters stopped changing at 165.1 seconds and remained stable through the end. The run produced 30,003 frames, accepted 28,399, recorded 13,961 paired frames, 14,438 silent references, 18 stale render frames and three AEC resets, with zero invalid AEC output, deadline miss, rejected write, sink failure or terminal error. Processing P99 was 250 microseconds and maximum was 861 microseconds. The user reported normal continuous audio without gaps or stale replay, apart from the same double-talk quality defect.
- The approved rollback first scheduled the validation device for removal on reboot. After the first user-performed reboot, read-only inventory found no validation device. The resumed cleanup deleted `oem53.inf`, removed the recorded certificate thumbprint from LocalMachine My, Root and TrustedPublisher, and configured TESTSIGNING off. Final read-only inventory after the second user-performed reboot found zero MiniAEC devices and endpoints, zero matching certificates, no matching package, no `MiniAECValidation` service or service registry key, and no enabled TESTSIGNING entry. K7 held all default capture roles and Realtek speakers held all default render roles. This matches the recorded rollback targets, including the user-declared output-device change, and completes task 9.6.

Normal-user access, production signing, installer architecture and long-run drift correction remain unvalidated. Agents must never initiate, schedule or invoke a restart, shutdown or sign-out; any required restart is performed manually by the user after saving work.
