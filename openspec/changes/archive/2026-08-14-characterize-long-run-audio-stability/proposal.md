## Why

MiniAEC has passed minute-scale real-time AEC acceptance, but it has not established whether the independently clocked physical microphone and render device remain synchronized during sustained use. Before adding speculative drift correction or preparing distribution, the project needs repeatable long-run evidence that distinguishes stable operation from persistent clock drift, growing latency, periodic frame loss, or bounded-synchronizer failure.

## What Changes

- Add a long-run audio stability capability that runs the existing real-time `MiniAEC Microphone` product path for a defined measurement interval and produces reviewable metadata-only stability evidence.
- Add analysis and acceptance criteria for per-input device-position/QPC rate, relative clock drift in parts per million, synchronization-delta trend, queue pressure, discards, silent references, resets, processing deadlines, virtual transport continuity, and terminal failures.
- Establish a 30-minute acceptance gate using the K7 microphone and the current active Realtek speakers render device as the final real-device duration requirement for this change.
- Record an evidence-based outcome: either the current bounded synchronizer is sufficient for the observed hardware pair, or persistent drift justifies a separate clock-drift compensation change.
- Preserve the metadata-only stability logging and analysis contract so early product versions can retain actionable diagnostics and longer validation can be reconsidered only when field evidence identifies a need.
- Keep private recordings, raw machine metadata, and generated reports under ignored `artifacts/`; retain only redistributable procedure, schema, synthetic fixtures, and summarized acceptance conclusions in the repository.
- Update active technical and validation documentation to describe the long-run procedure, measurements, acceptance result, and follow-up trigger.
- Goal: determine from the 30-minute gate whether sustained real-time AEC remains continuous, bounded, and explainable, and make the need for drift compensation or later extended validation an evidence-based decision.
- Non-goal: require a two-hour pre-release gate, implement asynchronous resampling, alter frame pairing or recovery policy, tune or upgrade AEC3, add device selection or persistence, change the Windows driver protocol or access policy, or define production installation and signing.

## Capabilities

### New Capabilities

- `long-run-audio-stability`: Defines metadata-only long-run measurement, drift analysis, acceptance gates, privacy boundaries, and the decision trigger for a separate compensation change.

### Modified Capabilities

None.

## Impact

- Affected areas: the headless validation/reporting path, reusable analysis code and synthetic tests, long-run validation documentation, and milestone status documentation.
- The real-time engine snapshot may gain project-owned timing summaries only if the existing one-second metadata stream cannot support a reliable rate estimate; PCM flow and existing lifecycle behavior remain unchanged.
- No WebRTC dependency, AEC3 configuration, vendored source, virtual microphone protocol, driver ACL, driver package, certificate, boot configuration, installer, or public endpoint behavior changes.
- Approved real-device acceptance may exercise the already documented driver lifecycle, but this change does not itself authorize driver installation, update, removal, test-signing changes, or a system restart.
- Longer-duration validation is deferred rather than prohibited; a future change may define it when early-version diagnostics demonstrate a concrete risk that the accepted 30-minute gate does not cover.
