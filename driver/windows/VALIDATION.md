# Virtual microphone candidate validation

This protocol is the common comparison surface for the private WaveRT render sink and restricted shared-ring candidates. It defines evidence collection but does not authorize test-mode, certificate, driver installation, device restart, or uninstall actions. Those commands and their rollback plan are presented separately under OpenSpec task 5.1 and require explicit approval before execution.

## Fixed input contract

Both candidates must accept the same project-owned contract:

| Field | Value |
| --- | --- |
| Sample rate | 48,000 Hz |
| Channels | 1 |
| Sample representation | Signed 16-bit little-endian PCM |
| Frame duration | 10 ms |
| Samples per frame | 480 |
| Frame sequence | Starts at 0 and increases by one within each sender session |
| Base signal | Byte-deterministic integer triangle wave |
| Audible marker | Integer square wave mixed into frames 0–19 of every 500-frame period |
| Marker cadence | 200 ms at session start and every 5 seconds |
| Continuous comparison input | Exactly 30,000 frames, or 300 seconds |

The current non-driver smoke test is:

```powershell
cargo run -p mini-aec-sender -- --transport dry-run --duration-seconds 300
```

Candidate adapters will add `wavert` and `shared-ring` transport values without changing signal generation, pacing, session identity, frame logging, or the diagnostic schema. Run logs belong under the ignored `driver/windows/out/validation/<candidate>/<run-id>/` directory. Do not use a physical microphone or files from `artifacts/` as candidate input.

## Endpoint isolation check

Save the present audio endpoint inventory before installing either candidate. After an approved candidate installation, enumerate audio endpoints again and verify all of the following:

- Exactly one new public capture endpoint is named `MiniAEC Microphone`.
- No producer-only render or capture endpoint appears in ordinary Windows Sound settings or Windows Recorder device selection.
- The pre-install default input and output devices remain the defaults.
- Every unrelated physical capture and render endpoint from the baseline remains present with the same enabled state.
- Record the candidate package identity, public endpoint instance ID, driver service identity, and the before/after inventory paths.

The exact read-only inventory command and approved lifecycle commands are added under tasks 5.1 and 5.2 after candidate packages exist. Visual inspection in Windows Sound settings and Windows Recorder remains required because a private WaveRT sink that leaks into an ordinary application fails the WaveRT hard gate even if a lower-level enumeration filter could hide it.

## Five-minute Windows Recorder run

Use the following sequence separately for each installed candidate:

1. Confirm that the previous candidate is fully uninstalled and that the current candidate exposes only `MiniAEC Microphone`.
2. Open Windows Recorder and select `MiniAEC Microphone` using the application's input-device selection without changing the Windows default input device.
3. Start recording while no sender is connected and retain the initial silence as proof that stale PCM is not replayed.
4. Start `mini-aec-sender` for exactly 300 seconds with the candidate transport and save every JSON-line event, including the generated session identity and frame sequence 0 through 29,999.
5. After the sender logs `session_closed`, stop Windows Recorder and save the recording outside version control in that run's ignored evidence directory.
6. Correlate the captured session-start marker and subsequent five-second markers with sender log sequences 0, 500, 1,000, and so on through 29,500.
7. Confirm that diagnostics report 30,000 accepted frames and no unexplained rejected write, underrun, overflow, session reset, or driver restart during the scored five-minute interval.

The recording may contain a short silence before and after the exact 300-second sender interval. The scored interval begins at the session-start marker and ends after frame 29,999; both candidates use the same rule.

## Sender restart run

Keep one Windows Recorder capture open for the complete sequence:

1. Start a first sender session and retain its session identity, frame sequence, markers, and diagnostics.
2. Stop or terminate the sender and record the stop time.
3. Leave Windows Recorder open for at least ten seconds and verify continuing zero-valued silence with no repeated marker or old frame.
4. Start a second sender process, verify a different session identity and frame sequence restarting at 0, and retain its session-start marker and diagnostics.
5. Stop recording and verify that no first-session PCM appears after the second session begins.

## Driver restart run

This scenario is system-changing and must not run until the exact restart and rollback commands have been reviewed and explicitly approved:

1. Save the active sender session, endpoint inventory, diagnostics, driver package, service, and device instance identities.
2. Execute only the approved disable-enable or equivalent driver restart for the current candidate.
3. Confirm that the public endpoint disappears and returns while unrelated endpoints remain unchanged.
4. Reopen Windows Recorder after `MiniAEC Microphone` returns; an existing recording client is not required to survive the driver restart.
5. Start a new sender process and verify a new session identity, frame sequence beginning at 0, a fresh session marker, and no PCM retained from before the restart.
6. Record the driver restart counter and all observed endpoint transition times.

## Evidence record

Create one record per candidate and run with these fields:

| Area | Required evidence |
| --- | --- |
| Source | Repository commit, OpenSpec change name, candidate name, transport protocol version, diagnostics schema version |
| Environment | Timestamp and timezone, Windows edition/build, Visual Studio, x64 MSBuild, MSVC, SDK, WDK, SignTool versions |
| Package | Driver package path and hash, INF identity, service name, device instance ID, public endpoint ID, development-signing identity |
| Isolation | Before/after endpoint inventories, default input/output identities, visible endpoint count, producer-only endpoint visibility result |
| Sender | Command, process ID, session ID, first and last sequence, start/stop times, marker sequences, sender exit code, log path |
| Recording | Windows Recorder version when observable, selected input, recording path, duration, first/last marker times, ordered-marker result |
| Continuity | Accepted frames, rejected writes, underruns, overflows, unexplained gaps, stale segments, observable latency |
| Recovery | Sender stop time, silence interval, new sender session ID, driver restart command/result, endpoint return time, new-session result |
| Safety | Approval reference, pre-change boot/certificate/device state, post-uninstall comparison, unresolved residue |
| Decision | Hard-gate pass/fail for isolation, silence on underrun, sender restart, driver restart, plus complexity and attack-surface notes |

Generated driver packages, certificate private keys, logs, and recordings remain outside version control. Only a redacted decision record containing no private PCM may be committed after both candidates have been evaluated.
