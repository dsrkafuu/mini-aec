# Tasks

## 1. Documentation and active contract migration

- [x] 1.1 Rewrite `README.md`, `AGENTS.md`, `docs/technical-plan.md` and `openspec/config.yaml` so the future product route is `physical microphone + physical speaker loopback -> AEC -> CABLE Input -> CABLE Output`, while clearly stating that the current code has not implemented that route; verify `rg` finds no active product claim that MiniAEC will ship, sign or own `MiniAEC Microphone`.
- [x] 1.2 Update `docs/aec-baseline.md`, `docs/realtime-aec-validation.md`, `docs/long-run-audio-stability.md` and related validation guidance so completed SysVAD runs remain explicitly historical while all future acceptance uses `CABLE Output`; verify historical measurements are unchanged and all forward-looking links and endpoint directions are correct.
- [x] 1.3 Remove the active production-signing documents `docs/production-driver-package.md` and `docs/production-driver-lifecycle.md`, replace any still-useful history with concise Git/OpenSpec references and verify no active documentation asks for a production signer, timestamp service, INF/SYS/CAT release or MiniAEC driver installer.
- [x] 1.4 Update repository maps and legacy driver documentation to mark `driver/windows`, its imported SysVAD source and `mini-aec-release` as migration-only legacy implementation pending equivalent VB-CABLE acceptance; verify no document presents those paths as a supported release dependency.
- [x] 1.5 Sync this change's capability deltas into the main OpenSpec specs without archiving `adopt-vb-cable-output`, update changed spec purposes to the VB-CABLE contract where needed and verify `openspec validate --all --strict --no-interactive` passes.
- [x] 1.6 Retire `production-driver-package` and `production-driver-lifecycle` as superseded without syncing their unfinished production-driver deltas, preserve their artifacts in OpenSpec history and verify neither appears in `openspec list --json` while the existing unrelated working-tree changes remain intact.
- [x] 1.7 Run a documentation consistency pass for `MiniAEC Microphone`, `SysVAD`, production signing, `CABLE Input`, `CABLE Output`, official VB-Audio links and installation ownership; verify every remaining old-route occurrence is clearly historical or legacy and `git diff --check` passes.

## 2. VB-CABLE endpoint and output implementation

- [x] 2.1 Generalize the project-owned sink boundary and engine configuration from the private driver protocol to an output-session contract that accepts complete 10 ms 48 kHz mono finite frames; verify engine unit tests compile without WDK or SysVAD types entering engine, tray or AEC interfaces.
- [x] 2.2 Implement read-only Windows endpoint enumeration, explicit ID configuration and deterministic VB-CABLE pair preflight using data-flow role plus corroborating device metadata; verify tests cover one valid pair, missing/inactive/wrong-role endpoints, renamed display labels, ambiguous candidates and no-default fallback.
- [x] 2.3 Reject the selected `CABLE Output` endpoint as the physical microphone and the selected `CABLE Input` endpoint as the physical render-loopback source; verify both feedback configurations fail before any source, AEC or output session opens.
- [x] 2.4 Implement the event-driven shared-mode WASAPI render adapter for `CABLE Input`, including deterministic channel/sample/mix-format adaptation and fresh per-session conversion state; verify synthetic renderer tests preserve sample order, duration and finite values across supported formats.
- [x] 2.5 Connect the adapter through bounded freshest-audio storage, record padding/clock/queue/conversion/underrun/failure metadata and keep file, console and UI waits off real-time workers; verify backpressure tests prove bounded memory and no unbounded latency growth.
- [x] 2.6 Implement stop, endpoint invalidation, write failure and explicit restart behavior that closes render resources and clears partial, queued and converted PCM; verify a new session never submits bytes or samples retained from the prior run.
- [x] 2.7 Update the windowless tray and headless commands to show the selected direction `MiniAEC -> CABLE Input -> CABLE Output`, missing/ambiguous prerequisite errors and output health without adding a WebView; verify state transitions and error text in controller tests.

## 3. Automated verification

- [x] 3.1 Replace private-driver transport fixtures with fake VB-CABLE output and endpoint inventory fixtures while retaining synthetic bypass, synchronization and AEC coverage; verify no automated test requires a driver, administrator token, certificate or boot mutation.
- [x] 3.2 Extend metadata schemas and stability analysis for paired output identities, negotiated format, render progress, conversion, underrun and output failure while maintaining compatibility with retained historical evidence; verify old fixtures still analyze truthfully and new fixtures exercise each output failure disposition.
- [x] 3.3 Run `cargo fmt --all -- --check`, `.tools\cargo-webrtc.cmd test --workspace`, `.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings`, strict OpenSpec validation and `git diff --check`; record any environment-only limitation without marking the associated task complete.

## 4. Authorized Windows runtime acceptance

- [x] 4.1 Document the exact supported VB-CABLE package/version, official download, manual administrator installation and user-performed restart boundary, then obtain separate authorization before any real-machine validation; verify MiniAEC itself performs only read-only endpoint preflight and audio runtime operations.
- [x] 4.2 Validate five-minute bypass by rendering the selected physical microphone to `CABLE Input` and consuming `CABLE Output` with Windows Recorder; verify expected duration, no stale replay or unexplained gap and complete accounting of nonzero output counters.
- [x] 4.3 Validate frozen-default AEC with the existing far-end-only, near-end-only, double-talk and render-silence/recovery matrix while Recorder and one target meeting application consume `CABLE Output`; verify quality claims remain bounded by observed evidence and no raw-microphone fallback occurs.
- [x] 4.4 Validate stop/start, source invalidation and VB-CABLE endpoint invalidation/recovery with explicit MiniAEC restart; verify each run has fresh output state, no automatic endpoint substitution and no stale PCM from an earlier run.
- [x] 4.5 Complete the documented 30-minute K7/Realtek functional-stability gate through `CABLE Input`/`CABLE Output`; verify ordinary-client coverage, metadata-only evidence and a separate functional result under ignored `artifacts/`, and record clock-drift characterization as conclusive only when active-render coverage meets the analyzer threshold, otherwise explicitly `inconclusive` without a drift-compensation claim.

## 5. Legacy driver and release-path cleanup

- [x] 5.1 After sections 2 through 4 pass, remove the obsolete MiniAEC SysVAD transport, driver projects, INF templates and driver build/install/signing scripts; verify no remaining workspace target or documented command can build, sign, install or restart a MiniAEC driver.
- [x] 5.2 Remove the production package/lifecycle implementation and obsolete `mini-aec-release` workspace surface while preserving unrelated user changes and historical Git/OpenSpec evidence; verify workspace metadata and all remaining Rust tests pass.
- [x] 5.3 Audit retained upstream files and notices before removing the imported SysVAD tree and MS-PL materials; verify no retained source derives from or redistributes the pinned SysVAD snapshot before deleting its notice.
- [x] 5.4 Remove obsolete driver-only validation helpers and references, retain only VB-CABLE-compatible audio/stability tooling and verify `rg` finds no non-historical `MiniAECTransport`, `MiniAECProduction`, `MiniAECValidation`, MiniAEC-owned INF/SYS/CAT or production-signing path; retain the user-provided external VB-CABLE package as a separately managed prerequisite, not a MiniAEC release payload.
- [x] 5.5 Measure the cleaned repository and ignored tool/runtime footprint, document optional local cache cleanup separately from source changes and verify no command deletes or modifies private `artifacts/` recordings.

## 6. Final product acceptance and change completion

- [x] 6.1 Re-run the complete build, test, Clippy, strict OpenSpec and documentation checks after legacy cleanup and verify the supported Windows 11 x64 build no longer needs WDK, SysVAD or driver-signing prerequisites.
- [x] 6.2 Review the final product path, external licensing boundary, endpoint UX, failure behavior, evidence and repository tree against proposal, design and specs; verify every requirement has implementation or acceptance evidence and no production-driver work remains active.
- [ ] 6.3 Archive `adopt-vb-cable-output` only after all tasks and acceptance gates pass, preserving archived historical driver changes and verifying the main specs describe only the supported VB-CABLE product route.
