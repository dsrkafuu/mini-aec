## 1. Validation schema and fixtures

- [x] 1.1 Extend the real-time validation event schema with schema version 2 monotonic elapsed time and requested-duration metadata while preserving wall-clock correlation and metadata-only snapshots.
- [x] 1.2 Add parser models that accept usable schema version 1 evidence for diagnostics but require schema version 2 fields for authoritative long-run gates, with actionable errors for incompatible, truncated, mixed-run, or non-monotonic input.
- [x] 1.3 Add redistributable synthetic metadata fixtures or generators for stable clocks, positive and negative drift, inconsistent windows, discontinuity and epoch boundaries, render silence, insufficient coverage, recurring synchronization maintenance, terminal failure, and functional counter failures.

## 2. Clock and coverage analysis

- [x] 2.1 Implement clean-segment construction across endpoint identity, run identity, synchronization epoch, discontinuity, timestamp error, event-gap, QPC monotonicity, device-position monotonicity, and inactive-render boundaries.
- [x] 2.2 Implement five-minute paired-window least-squares effective-rate estimates for microphone and render roles, the documented signed relative-ppm calculation, residual uncertainty, run median, median absolute deviation, and conservative phase accumulation.
- [x] 2.3 Implement duration and data-quality summaries covering requested and observed duration, expected and observed events, periodic-event coverage, usable duration, longest clean segment, excluded intervals, and explicit exclusion reasons.
- [x] 2.4 Verify clock and coverage analysis with deterministic unit tests for exact rates, known ppm offsets, numerical uncertainty, segmentation, sparse data, direction changes, and boundary thresholds.

## 3. Stability classification and report

- [x] 3.1 Implement the three-state drift disposition using persistent window direction, conservative 5 ms threshold-crossing time, time-localized synchronization counters, render-silence exclusions, and terminal synchronization failure evidence.
- [x] 3.2 Implement the independent functional-stability disposition using duration, lifecycle, queue, discard, discontinuity, reset, AEC, processing-deadline, sink, driver, and required operator-observation evidence.
- [x] 3.3 Define and serialize a versioned stability report containing source/run identities, software revision when available, analysis method and thresholds, clean windows, rate and uncertainty results, counter deltas, excluded evidence, drift disposition, functional disposition, 30-minute acceptance status, evidence-triggered follow-up guidance, and actionable reasons.
- [x] 3.4 Add a platform-independent `mini-aec-lab` stability-report command that reads one JSONL event stream plus explicit operator observations and writes only within the existing approved ignored evidence roots.
- [x] 3.5 Verify parsing, classification, report serialization, output-path containment, failed-run analysis, and operator-observation requirements with synthetic automated tests that perform no Windows system mutation and use no private recordings.

## 4. Real-time evidence integration

- [x] 4.1 Update the headless real-time AEC writer to emit started, periodic, failed, and final schema version 2 events with zero-based monotonic elapsed time and consistent requested-duration metadata outside real-time workers.
- [x] 4.2 Add only the bounded project-owned timing summaries proven necessary by synthetic analyzer validation; otherwise retain the current one-second snapshot surface unchanged.
- [x] 4.3 Verify synthetic engine start, periodic sampling, normal completion, early failure, stop cleanup, distinct restart identities, and metadata-only serialization without changing synchronization, AEC3, PCM, or driver behavior.

## 5. Documentation and repository verification

- [x] 5.1 Document the stability-report schema, signed ppm convention, clean-segment rules, coverage thresholds, classification rules, operator observations, private evidence boundary, and interpretation of `bounded-synchronizer-sufficient`, `clock-drift-compensation-required`, and `inconclusive`.
- [x] 5.2 Document the 30-minute K7/Realtek speakers capture and ordinary-client procedure as the final duration gate for this change, the evidence-triggered extended-validation policy, the separate approval boundary for driver lifecycle operations, rollback, and the prohibition on agent-initiated restart.
- [x] 5.3 Update `README.md`, `docs/technical-plan.md`, and relevant validation documentation to correct the active-change status and describe M4 as characterization first, compensation only on evidence.
- [x] 5.4 Run `cargo fmt --all -- --check`, `.tools\cargo-webrtc.cmd test --workspace`, and `.tools\cargo-webrtc.cmd clippy --workspace --all-targets -- -D warnings`; resolve all failures before any real-device acceptance.

## 6. Thirty-minute characterization acceptance

- [x] 6.1 Review the exact driver lifecycle plan and rollback path, obtain separate explicit approval for any required signing, installation, activation, restart, removal, or rollback operation, and stop for the user to perform any required system restart manually.
- [x] 6.2 From an ordinary non-elevated interactive session, capture at least 30 minutes of schema version 2 evidence using the exact K7 microphone and current active Realtek speakers render endpoint while an ordinary client continuously consumes `MiniAEC Microphone` and intentional render activity covers the drift-scored interval.
- [x] 6.3 Record the required operator observations, generate `stability-report.json`, review excluded intervals and every nonzero counter, and classify drift and functional stability independently; repeat the run rather than infer a result if the report is `inconclusive`.
- [x] 6.4 Record the summarized 30-minute conclusion in project documentation without committing raw endpoint identities, JSONL, generated private reports, or recordings, state that no mandatory two-hour gate remains, and state whether a separate `compensate-audio-clock-drift` proposal or evidence-triggered extended validation is required.
- [x] 6.5 Complete the approved driver rollback, obtain the user's manual restart if required, and verify by read-only inventory that device, endpoint, package, certificate, service, default-role, and TESTSIGNING state match the recorded baseline before marking characterization acceptance complete.
