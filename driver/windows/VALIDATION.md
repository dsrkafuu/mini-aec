# MiniAEC Microphone transport validation

This protocol validates the selected private control-interface and driver-owned-ring transport. It defines evidence collection but does not authorize test mode, certificate, driver installation, device restart, or uninstall actions. Every such action requires explicit user approval after reviewing the commands and rollback plan below.

## Build and lifecycle preview

Create the ignored unsigned x64 Debug package and Release sender without changing Windows driver state:

```powershell
driver\windows\scripts\build-validation.ps1
.tools\cargo-webrtc.cmd build -p mini-aec-sender --release
```

Preview all planned system mutations and save the pre-change inventory:

```powershell
driver\windows\scripts\validation-lifecycle.ps1 -Action Plan
.tools\cargo-webrtc.cmd build -p mini-aec-lab
driver\windows\scripts\validation-lifecycle.ps1 -Action Inventory
```

The generated package, certificate, inventory, sender logs, and recordings stay below ignored `driver/windows/out/`, `target/`, or private `artifacts/` paths.

## Approved lifecycle commands and rollback

Run the following only from an elevated PowerShell after explicit approval. `PrepareSigning` creates one non-exportable `CN=MiniAEC Validation Test` certificate in LocalMachine My, imports its public certificate into LocalMachine Root and TrustedPublisher, signs only the validation CAT, and runs `bcdedit.exe /set testsigning on`. It does not reboot or install the driver.

```powershell
driver\windows\scripts\validation-lifecycle.ps1 -Action PrepareSigning -ConfirmSystemChanges
```

Record the printed certificate thumbprint. Reboot Windows manually, rerun `-Action Inventory`, and confirm TESTSIGNING before installing. Installation uses the exact WDK x64 DevCon path shown by `-Action Plan`, the package INF, and hardware ID `Root\MiniAECValidation`:

```powershell
driver\windows\scripts\validation-lifecycle.ps1 -Action Install -ConfirmSystemChanges
```

Immediately record the published `oem<number>.inf` associated with original name `MiniAECValidation.inf` from `pnputil.exe /enum-drivers /class Media`. The approved targeted restart is:

```powershell
driver\windows\scripts\validation-lifecycle.ps1 -Action Restart -ConfirmSystemChanges
```

Rollback removes the root device, deletes only the recorded MiniAEC package, removes only the recorded certificate thumbprint from LocalMachine My, Root and TrustedPublisher, and optionally restores test signing to off when the pre-install inventory showed it was off:

```powershell
driver\windows\scripts\validation-lifecycle.ps1 -Action Uninstall -PublishedInf oem<number>.inf -CertificateThumbprint <40-hex-thumbprint> -RestoreTestSigningOff -ConfirmSystemChanges
```

If `-RestoreTestSigningOff` is used, reboot Windows manually and save another inventory. Compare the endpoint/default-role list, PnP list, boot configuration, certificate stores, driver packages, service/device absence, and unrelated physical devices with the pre-install JSON. If signing preparation fails before installation, remove only the exact recorded certificate thumbprint and restore the saved test-signing state manually. Secure Boot can prevent BCDEdit test-signing changes; do not disable Secure Boot automatically or from this script.

## Fixed input contract

The driver and sender use this project-owned contract:

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

The installed driver path uses `--transport driver` without changing signal generation, pacing, session identity, frame logging, or the diagnostic schema. Run logs belong under the ignored `driver/windows/out/validation/driver/<run-id>/` directory. Do not use a physical microphone or files from `artifacts/` as validation input.

## Endpoint isolation check

Save the present audio endpoint inventory before installation. After an approved installation, enumerate audio endpoints again and verify all of the following:

- Exactly one new public capture endpoint is named `MiniAEC Microphone`.
- No producer-only render or capture endpoint appears in ordinary Windows Sound settings or Windows Recorder device selection.
- The pre-install default input and output devices remain the defaults.
- Every unrelated physical capture and render endpoint from the baseline remains present with the same enabled state.
- Record the package identity, public endpoint instance ID, driver service identity, and the before/after inventory paths.

Visual inspection in Windows Sound settings and Windows Recorder remains required. The selected INF must add exactly one public capture endpoint and no producer-facing render endpoint.

## Five-minute Windows Recorder run

Use the following sequence for the installed validation driver:

1. Confirm that the baseline contains no earlier MiniAEC package or device and that the installed validation driver exposes only `MiniAEC Microphone`.
2. Open Windows Recorder and select `MiniAEC Microphone` using the application's input-device selection without changing the Windows default input device.
3. Start recording while no sender is connected and retain the initial silence as proof that stale PCM is not replayed.
4. Start `mini-aec-sender --transport driver --duration-seconds 300` and save every JSON-line event, including the generated session identity and frame sequence 0 through 29,999.
5. After the sender logs `session_closed`, stop Windows Recorder and save the recording outside version control in that run's ignored evidence directory.
6. Correlate the captured session-start marker and subsequent five-second markers with sender log sequences 0, 500, 1,000, and so on through 29,500.
7. Confirm that diagnostics report 30,000 accepted frames and account for every rejected write, underrun, overflow, discarded frame, session reset, current depth, high-water mark, and driver restart during the scored five-minute interval.

The recording may contain a short silence before and after the exact 300-second sender interval. The scored interval begins at the session-start marker and ends after frame 29,999.

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
2. Execute only the approved targeted validation-driver restart.
3. Confirm that the public endpoint disappears and returns while unrelated endpoints remain unchanged.
4. Reopen Windows Recorder after `MiniAEC Microphone` returns; an existing recording client is not required to survive the driver restart.
5. Start a new sender process and verify a new session identity, frame sequence beginning at 0, a fresh session marker, and no PCM retained from before the restart.
6. Record the driver restart counter and all observed endpoint transition times.

## Evidence record

Create one record per run with these fields:

| Area | Required evidence |
| --- | --- |
| Source | Repository commit, OpenSpec change name, transport name, transport protocol version, diagnostics schema version |
| Environment | Timestamp and timezone, Windows edition/build, Visual Studio, x64 MSBuild, MSVC, SDK, WDK, SignTool versions |
| Package | Driver package path and hash, INF identity, service name, device instance ID, public endpoint ID, development-signing identity |
| Isolation | Before/after endpoint inventories, default input/output identities, visible endpoint count, producer-only endpoint visibility result |
| Sender | Command, process ID, session ID, first and last sequence, start/stop times, marker sequences, sender exit code, log path |
| Recording | Windows Recorder version when observable, selected input, recording path, duration, first/last marker times, ordered-marker result |
| Continuity | Accepted frames, rejected writes, underruns, overflows, discarded frames, current depth, high-water mark, unexplained gaps, stale segments, observable latency |
| Recovery | Sender stop time, silence interval, new sender session ID, driver restart command/result, endpoint return time, new-session result |
| Safety | Approval reference, pre-change boot/certificate/device state, post-uninstall comparison, unresolved residue |
| Decision | Hard-gate pass/fail for isolation, silence on underrun, sender restart, driver restart, plus complexity and attack-surface notes |

Generated driver packages, certificate material, logs, and recordings remain outside version control. Only a redacted result record containing no private PCM may be committed after validation is complete.
