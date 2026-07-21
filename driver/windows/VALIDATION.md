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
driver\windows\scripts\validation-lifecycle.ps1 -Action Uninstall -PublishedInf oem<number>.inf -CertificateThumbprint <40-hex-thumbprint> -ConfirmSystemChanges

# Use this variant only when restoring a saved TESTSIGNING=off state was separately approved.
driver\windows\scripts\validation-lifecycle.ps1 -Action Uninstall -PublishedInf oem<number>.inf -CertificateThumbprint <40-hex-thumbprint> -RestoreTestSigningOff -ConfirmSystemChanges
```

If `-RestoreTestSigningOff` is used, reboot Windows manually and save another inventory. A targeted DevCon restart or uninstall can also report that a full reboot is required; the script does not reboot Windows and the pending state must be inventoried and approved before rebooting. Compare the endpoint/default-role list, PnP list, boot configuration, certificate stores, driver packages, service/device absence, and unrelated physical devices with the pre-install JSON. If signing preparation fails before installation, remove only the exact recorded certificate thumbprint and restore the saved test-signing state manually. Secure Boot can prevent BCDEdit test-signing changes; do not disable Secure Boot automatically or from this script.

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
- Windows system sound settings allow `MiniAEC Microphone` to be selected as the default input device.
- Record the saved and installed Console, Multimedia, and Communications default input roles. Windows-originated role changes during installation are accepted when they are explicit evidence, and uninstall must restore the saved roles automatically or pause for separately approved restoration.
- The pre-install default output roles remain unchanged.
- Every unrelated physical capture and render endpoint from the baseline remains present with the same enabled state.
- Record the package identity, public endpoint instance ID, driver service identity, and the before/after inventory paths.

Visual inspection in Windows Sound settings and Windows Recorder remains required. The selected INF must add exactly one public capture endpoint and no producer-facing render endpoint.

## Five-minute Windows Recorder run

Use the following sequence for the installed validation driver:

1. Confirm that the baseline contains no earlier MiniAEC package or device and that the installed validation driver exposes only `MiniAEC Microphone`.
2. Open Windows Recorder and select `MiniAEC Microphone`. Record whether Windows already assigned it a default input role rather than forcing the saved default to remain unchanged.
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

## 2026-07-21 validation result

The development transport passed the endpoint-isolation, five-minute continuity, sender-restart, driver-restart and rollback gates on the working tree based on `main` commit `759614e`. The OpenSpec change was `validate-virtual-microphone-transport`, the control protocol was version 1, and diagnostics used schema 2. This result validates the development transport only; it is not production signing, installer, upgrade, compatibility or real-time AEC acceptance.

The clean-checkout reproduction commands for the resulting source revision are:

```powershell
git status --short
driver\windows\scripts\preflight.ps1 -Json
driver\windows\scripts\verify-upstream.ps1 -CheckoutRoot .tools\sysvad-upstream
driver\windows\scripts\build-validation.ps1
.tools\cargo-webrtc.cmd build -p mini-aec-sender --release
cargo fmt --all -- --check
.tools\cargo-webrtc.cmd test --workspace
.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings
```

The validated environment was Windows 11 Pro for Workstations 25H2 build 26200.8875, Visual Studio Build Tools 2026 18.8.12009.203, x64 MSBuild 18.8.2, MSVC 14.51.36231, Windows SDK/WDK 10.0.28000.0, and SignTool 10.0.28000.2114. Secure Boot was off. TESTSIGNING was already on at the saved rollback baseline and was intentionally left on with explicit approval.

Final source checks passed: `cargo fmt --all -- --check`, `.tools\cargo-webrtc.cmd test --workspace`, `.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings`, `driver\windows\scripts\build-validation.ps1`, and `.tools\cargo-webrtc.cmd build -p mini-aec-sender --release`. The first test invocation encountered a stale generated Tauri permission path from the repository's former `D:\GitHub\open-denoise` location; removing only the regenerable `tauri` Cargo cache fixed the environment issue, after which the complete test suite passed. `git diff --check` passed, and the Git index contained no generated package, certificate file, private recording, `driver/windows/out/`, `target/`, or `artifacts/` content.

The package installed as `oem53.inf`, service `MiniAECValidation`, device `ROOT\MEDIA\0000`, and public endpoint `{0.0.1.00000000}.{48c79171-915d-46ba-81e4-606f4be171e8}`. Development certificate thumbprint `05F44E8C76BCA800C2980B2978FC47A27EFE2BC3` was used only in LocalMachine My, Root and TrustedPublisher and was removed during rollback. Windows automatically changed the Console, Multimedia and Communications default input roles from endpoint `{0.0.1.00000000}.{6a144e18-ca31-4794-b41e-ffe95fb9aba4}` (`Krisp Microphone`) to `MiniAEC Microphone` during installation; this established system-default selectability. Uninstall automatically restored all three roles to Krisp without a manual default-device mutation. Default render roles remained on `{0.0.0.00000000}.{116f2831-3426-43b9-a9ae-740dde724709}` (`Sound Blaster X4`).

| Stage | Evidence | Result |
| --- | --- | --- |
| Rollback baseline | `driver/windows/out/validation/inventory-20260721-183254.json` | No MiniAEC device, package or certificate; Krisp owned all default input roles; TESTSIGNING on |
| Installed endpoint | `driver/windows/out/validation/inventory-20260721-184123.json` | One active MiniAEC capture endpoint, no added public render endpoint, MiniAEC owned all default input roles |
| Five-minute and sender restart | `driver/windows/out/validation/driver/20260721-191056/` and private `C:\Users\<user>\Documents\录音\录音 (3).m4a` | First session sent 30,000 ordered frames with zero rejected writes, underruns, overflows or discards in its delta; after 10.23 seconds of silence, a distinct session sent 1,000 frames from sequence 0 with no stale replay |
| Before driver restart | `driver/windows/out/validation/inventory-20260721-192334.json` | Device, endpoint, package, certificate and default roles saved before the approved restart |
| After driver reboot | `driver/windows/out/validation/inventory-20260721-193518.json` | Same device, endpoint ID, package and default roles returned without reinstalling |
| Fresh post-restart session | `driver/windows/out/validation/driver/20260721-193653/` and private `C:\Users\<user>\Documents\录音\录音 (4).m4a` | New session `00005adc0000000018c44aff75dd1728` sent sequences 0-2999; 3,000 accepted frames and zero rejected writes, underruns, overflows, discards or resets; 41.30-second recording contained 5.45 seconds initial silence, 30 seconds of new signal with six ordered five-second markers, and 5.85 seconds trailing silence |
| After uninstall | `driver/windows/out/validation/inventory-20260721-194106.json` | Endpoint, PnP device, package and certificate gone; defaults restored; running disabled service remained marked `DriverDelete=1` and `DeleteFlag=1` pending reboot |
| Final rollback | `driver/windows/out/validation/inventory-20260721-194543.json` | Service and registry entry gone after the separately approved reboot; package, device, endpoint, control interface and certificate absent; saved input/output roles and boot state restored |

The targeted DevCon restart required a full Windows reboot instead of completing dynamically. Uninstall also required a full reboot to unload the disabled kernel service and finish deleting its marked service entry; the lifecycle script now reports both pending-reboot conditions without rebooting automatically. The final inventory differed from the saved baseline only in the unrelated `Realtek Bluetooth LE Audio Driver` PnP row, which was `Present=False` with `CM_PROB_PHANTOM` after reboot; its package remained installed, no Bluetooth audio endpoint was added or removed, and all visible endpoint identities and default roles matched the baseline. Windows Recorder version was not captured. Private recordings, generated packages, logs and certificate material remain ignored and must not be committed.
