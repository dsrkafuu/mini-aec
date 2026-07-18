## 1. Upstream and Toolchain Baseline

- [x] 1.1 Add `driver/windows/UPSTREAM.md` with the fixed `microsoft/Windows-driver-samples` repository, `audio/sysvad` path, commit `2ee527bfeb0aeb6be11f0a8b6dce4011b358ce89`, retrieval date, MS-PL identity, imported file inventory and a local-patch ledger.
- [x] 1.2 Add the applicable MS-PL license text and source notices required for the imported SysVAD files, and verify that no file comes from the unpinned `microsoft/audio` copy or another repository.
- [x] 1.3 Add a read-only Windows driver preflight command that reports Windows, Visual Studio, SDK, WDK, MSBuild and signing-tool versions without changing boot, certificate, driver or device state.
- [x] 1.4 Import the minimum fixed SysVAD source needed by the prototype and record every build-only adaptation outside upstream AEC code in the patch ledger.
- [ ] 1.5 Build the unmodified or minimally adapted x64 Debug SysVAD baseline with the documented toolchain and record the clean-checkout command and result before adding a transport.

## 2. Shared Validation Contract

- [ ] 2.1 Define a project-owned `VirtualMicrophoneSink` boundary for fixed-format session open, PCM write, diagnostics and close operations without exposing WaveRT, IOCTL or SysVAD types to the caller.
- [ ] 2.2 Implement the minimal user-mode sender with a deterministic 48 kHz mono PCM16 signal, periodic audible markers, session identity, monotonic frame sequence logging and explicit error reporting.
- [ ] 2.3 Define one versioned diagnostics schema for session transitions, accepted frames, rejected writes, underruns, overflows and driver restarts that both candidate transports must emit.
- [ ] 2.4 Add unit tests for signal determinism, frame sequencing, session reset and transport adapter error mapping without accessing a physical microphone or private `artifacts/` data.
- [ ] 2.5 Document the identical five-minute input, endpoint-enumeration checks, Windows Recorder steps, sender restart sequence, driver restart sequence and evidence fields used for both candidate transports.

## 3. Private WaveRT Render Sink Spike

- [ ] 3.1 Create an isolated SysVAD-derived spike with one public capture endpoint named `MiniAEC Microphone` and a producer-only WaveRT render sink.
- [ ] 3.2 Implement render-to-capture forwarding with audio-clock-driven capture, zero-valued underrun output, bounded buffering and session reset diagnostics.
- [ ] 3.3 Implement the `VirtualMicrophoneSink` adapter that opens and writes the producer-only render sink through WASAPI.
- [ ] 3.4 Add build-time and user-mode tests for format rejection, bounded-buffer behavior, stale-audio prevention and sender reconnection.
- [ ] 3.5 Produce a separate x64 Debug test-signable driver package for the WaveRT candidate without installing it or modifying the local machine.

## 4. Restricted Shared-Ring Spike

- [ ] 4.1 Create an isolated SysVAD-derived spike with one public capture endpoint named `MiniAEC Microphone` and a fixed-size driver-owned shared-ring control protocol.
- [ ] 4.2 Restrict the prototype control device to SYSTEM and Administrators, validate every version, length, index and session transition, and avoid a globally open named mapping.
- [ ] 4.3 Implement audio-clock-driven ring consumption, zero-valued underrun output, one documented overflow policy, stale-session discard and diagnostics counters.
- [ ] 4.4 Implement the `VirtualMicrophoneSink` adapter for the versioned control protocol and return actionable errors for access denial, driver absence, version mismatch and rejected writes.
- [ ] 4.5 Add driver-boundary and user-mode tests for malformed requests, ring wraparound, overflow, underrun, sender termination and new-session isolation.
- [ ] 4.6 Produce a separate x64 Debug test-signable driver package for the shared-ring candidate without installing it or modifying the local machine.

## 5. Approved Candidate Comparison and Selection

- [ ] 5.1 Present the exact test-mode, certificate, install, device-restart and uninstall commands plus rollback plan, and obtain explicit user approval before the first system-changing validation action.
- [ ] 5.2 Save a pre-install inventory of physical audio endpoints, default devices, relevant driver packages, services, certificates and test-signing state.
- [ ] 5.3 Sequentially install and test each candidate with the same five-minute sender input, endpoint visibility check, Windows Recorder capture, sender stop/restart and diagnostics collection, fully uninstalling one candidate before installing the other.
- [ ] 5.4 Execute the approved driver restart scenario for each candidate, reopen Windows Recorder after endpoint return and verify that only fresh-session PCM is captured.
- [ ] 5.5 Record endpoint isolation, permissions, continuity, observable latency, underrun, overflow, sender recovery, driver recovery, code complexity, attack surface and Rust integration evidence in a comparison decision record.
- [ ] 5.6 Select the WaveRT candidate only if it passes every hard gate; otherwise select the shared-ring candidate if it passes every hard gate, and stop the change with recorded failures if neither qualifies.

## 6. Selected Validation Path

- [ ] 6.1 Promote only the selected transport and adapter into the default validation build, exclude the unselected spike from that build and document why it was not selected.
- [ ] 6.2 Add a read-only-by-default validation entry point that prints planned system changes and requires explicit confirmation before test certificate, driver, restart or uninstall operations.
- [ ] 6.3 Add clean-checkout build and non-system test commands for the sender, selected driver package, boundary unit tests and driver protocol tests.
- [ ] 6.4 Update `driver/windows/README.md` and relevant architecture documentation with the selected transport, fixed PCM contract, diagnostics, development-only signing limits and deferred production concerns.

## 7. Final Verification and Rollback

- [ ] 7.1 From a clean build, verify that approved installation adds exactly one public capture endpoint named `MiniAEC Microphone`, exposes no producer-only public endpoint and does not change the default input device or unrelated physical endpoints.
- [ ] 7.2 Record at least five minutes in Windows Recorder and verify expected duration, ordered signal markers and diagnostics with no unexplained gap, stale segment, underrun, overflow or rejected write.
- [ ] 7.3 While one recording remains open, stop the sender, verify continuing silence without stale replay, restart the sender and verify capture resumes with a new session and no old-session PCM.
- [ ] 7.4 Restart the selected driver using the approved procedure, reopen Windows Recorder after endpoint return and verify a new sender session is captured without reinstalling the package.
- [ ] 7.5 Execute the approved uninstall and rollback procedure, verify that the endpoint, control interfaces, service, staged validation package and targeted test certificate are gone, and compare unrelated physical endpoints and boot state with the saved baseline.
- [ ] 7.6 Record exact commands, environment, evidence paths, pass/fail results and unresolved limitations without committing generated driver packages, certificate private keys or private recordings.
