## Context

See `proposal.md` for motivation and `specs/long-run-audio-stability/spec.md` for the behavior contract. The current real-time command writes a versioned JSONL event every second outside real-time workers. Each snapshot already exposes the latest microphone and render device positions and QPC timestamps, current synchronization delta, maximum skew, synchronization epoch, discontinuity and timestamp-error counters, queue/discard counters, AEC counters, processing latency, and sink diagnostics. Events use wall-clock `unix_ms`, which is useful for identifying evidence but is unsuitable as the independent variable for clock-rate estimation because wall time can be adjusted.

The M3 synchronizer pairs 10 ms frames within 5 ms, treats observations beyond 100 ms as unavailable, and fails after 50 consecutive timestamp-bearing misses. It provides bounded short-term behavior but does not correct sustained hardware-clock drift. The frozen M131 AEC3 configuration, audio format, synchronizer policy, and virtual microphone protocol must remain unchanged during characterization so the result measures the accepted baseline.

## Goals / Non-Goals

**Goals:**

- Produce deterministic, machine-readable analysis from retained metadata without replaying or inspecting PCM.
- Estimate the effective microphone and render rates against QPC, define the sign and uncertainty of relative drift, and relate that drift to the synchronizer's existing 5 ms pairing tolerance.
- Separate data-quality disposition, drift disposition, and functional-stability acceptance so a clean clock estimate cannot hide a transport or continuity failure.
- Reuse the normal-user real-time AEC path and existing project-owned snapshots, adding only the minimum timing fields required for sound analysis.
- Make the 30-minute K7/Realtek speakers result sufficient to decide whether a separate drift-compensation proposal is warranted.
- Complete the current long-run acceptance decision at 30 minutes while preserving a diagnostic surface that later versions can use to identify rare long-tail failures.

**Non-Goals:**

- Change worker scheduling, queue capacity, frame selection, silent-reference behavior, recovery thresholds, WASAPI formats, AEC processing, or driver transport.
- Automatically tune, resample, restart, select another endpoint, or convert AEC failure into bypass.
- Treat render-silent intervals as clock observations or interpret acoustic suppression quality from metadata.
- Automate Windows driver lifecycle changes, client UI operation, system restart, or the user's listening assessment.
- Require a fixed two-hour gate before the current path can be accepted.

## Decisions

### 1. Extend validation events with monotonic run time and declared run metadata

Validation event schema version 2 will add monotonic elapsed time since the engine run started and declared requested duration. The started event will retain the resolved endpoint descriptors and run identities already present in the snapshot. `unix_ms` remains for human correlation, but the analyzer will use QPC for per-device rate estimation and monotonic elapsed time for coverage, event ordering, counter-rate windows, and requested-duration validation.

The event writer remains in `mini-aec-lab`, outside real-time workers. Existing engine snapshots will remain project-owned and serializable; no PCM, per-frame trace, or WebRTC type will enter the schema. If validation shows that one-second last-observation samples are insufficient, the engine may add bounded first/latest observation summaries per role, but it will not add an unbounded trace or diagnostic I/O to capture workers.

Alternative considered: derive duration from `unix_ms`. This is rejected because wall-clock corrections can create false gaps or rate changes. Alternative considered: log every audio packet. This is rejected because it enlarges private evidence, increases diagnostic load, and is unnecessary for long-run trend estimation.

### 2. Add an offline stability-report command over one completed event stream

`mini-aec-lab` will expose a platform-independent analysis command that accepts an `engine.jsonl` path and writes a versioned `stability-report.json` beside it or below another validated ignored evidence path. Parsing, segmentation, estimation, classification, and report serialization will live in reusable Rust code that can be tested with synthetic metadata on any development platform.

The command will require exactly one coherent run identity and compatible schema. It will reject mixed runs, truncated JSON, regressing monotonic elapsed time, missing terminal disposition, and output paths outside the existing approved ignored roots. A failed real-time run remains analyzable: its report describes the failure rather than pretending the configured duration completed.

Alternative considered: analyze only through an ad hoc PowerShell or notebook. This is rejected because acceptance logic would be difficult to test and reproduce. No new numerical-analysis dependency is planned; the data volume and calculations are small enough for explicit checked Rust arithmetic and `f64` regression.

### 3. Segment observations before estimating clock rates

For each role, an observation consists of monotonic elapsed time, device position, device QPC timestamp, and the cumulative discontinuity/timestamp-error state. Clean segments will end whenever the synchronization epoch changes, either role's discontinuity or timestamp-error counter increases, device position or QPC fails to increase, the event gap exceeds the documented coverage bound, or endpoint/run identity changes. Render observations will additionally exclude intervals in which no new render position is observed; render silence is valid engine behavior but provides no render-clock evidence.

Only segments with adequate duration and observation count will contribute to drift. The report will state total duration, usable duration, longest clean segment, expected and observed periodic-event counts, excluded intervals, and exclusion reasons. The 30-minute gate requires at least 25 minutes of usable observations overall, at least one clean segment of 10 minutes, and periodic-event coverage of at least 95 percent; otherwise drift disposition is `inconclusive`. These thresholds tolerate bounded recovery while preventing a short clean fragment from representing the whole run.

Alternative considered: use only the first and final positions. This is rejected because one discontinuity or timestamp anomaly can dominate the estimate and conceal changing behavior.

### 4. Estimate relative rate in fixed windows and expose uncertainty

Each eligible clean segment will be divided into non-overlapping five-minute windows. Within a window, ordinary least-squares slope of device position against device QPC time gives the role's effective native frames per second. Because endpoints may use different native sample rates, each effective rate is divided by that endpoint's declared nominal native rate before comparison. Relative drift is defined as `((render_rate / render_nominal_rate) / (microphone_rate / microphone_nominal_rate) - 1) * 1_000_000` ppm, so positive values mean the render clock advances faster than the microphone clock relative to its nominal rate. A paired window is eligible only when both role estimates cover the same time span and each has sufficient observations.

The report will include every eligible window, per-role rate, relative ppm, residual error, window duration, and observation count. Its run-level estimate will use the median eligible-window ppm; median absolute deviation will describe between-window variation. A direction is persistent only when at least three eligible windows exist, at least 80 percent have the same nonzero sign after their residual uncertainty is applied, and the median magnitude exceeds both the residual-derived uncertainty and a documented 1 ppm numerical floor. Predicted phase accumulation is computed from the conservative drift magnitude remaining after uncertainty, not the raw point estimate.

Alternative considered: one regression over the entire run. Fixed windows make direction changes, thermal settling, bad segments, and inconsistency visible instead of compressing them into one attractive number. Alternative considered: use `current_delta_100ns` alone. That value is `render_qpc - microphone_qpc`, so a negative trend corroborates render-faster rate evidence and a positive trend corroborates render-slower evidence, but it also reflects discrete frame selection and silence/recovery and therefore remains corroborating synchronization evidence rather than an independent clock-rate estimate.

### 5. Classify drift from rate evidence plus observable synchronizer consequences

The drift disposition is independent from functional stability:

- `bounded-synchronizer-sufficient`: data quality passes, no persistent conservative rate estimate predicts at least 5 ms accumulated phase during the 30-minute gate, and there is no recurring drift-correlated synchronization maintenance or failure.
- `clock-drift-compensation-required`: data quality passes and either conservative persistent drift predicts at least 5 ms phase accumulation within the gate, a synchronization failure is preceded by directional clock/delta evidence, or stale-render/silent-reference counter increments recur in at least three separated eligible windows with the direction predicted by the rate estimate and without a render-silence, discontinuity, or timestamp-error explanation.
- `inconclusive`: coverage is insufficient, window direction is inconsistent, uncertainty overlaps the decision boundary, or rate and synchronizer evidence conflict.

The report will retain both the predicted threshold-crossing time and all counter deltas used for classification. It will not infer drift from a lone stale frame, render-silent interval, startup/recovery event, or cumulative counter without time-localized increments.

Alternative considered: use a fixed ppm pass/fail threshold. The operational risk depends on run duration and the synchronizer's phase tolerance, so converting the conservative ppm estimate into predicted phase error is more directly tied to product behavior.

### 6. Evaluate functional stability as a separate gate

Functional acceptance will check requested and observed duration, terminal state, run/session continuity, queue depth and high-water behavior, local and driver discards, sink rejection/failure, discontinuities and resets, AEC invalid output/rebuild, processing deadline misses, and client-consumption notes. The structured report will classify conditions deterministically from metadata and include a required operator checklist for facts not observable from the engine: the public endpoint was continuously consumed during the scored interval, the selected render endpoint had intentional active playback for drift scoring, and listening found no stale replay, periodic gap, or unexplained interruption.

The report generator will leave the functional gate `inconclusive` until the required operator observations are supplied through an explicit metadata sidecar or command arguments recorded in the report. It will never silently assume that an open capture client or audible continuity existed. Nonzero counters do not automatically fail when the contract permits bounded recovery, but every increment must be time-localized and explained; terminal failure, rejected writes, sink failure, invalid AEC output, deadline miss, growing queue depth, or unexplained discontinuity/reset fails the gate.

Alternative considered: make the JSONL alone the acceptance surface. That would repeat the mistake of treating a healthy process as proof that `MiniAEC Microphone` was continuously consumed and audible.

### 7. Keep real-device execution behind existing safety approvals

Repository implementation and synthetic analyzer tests require no driver installation. The documented 30-minute gate reuses the existing non-elevated runtime validation command only after an administrator has separately installed and activated an approved development package through the reviewed lifecycle. The change adds no install, signing, device restart, rollback, or reboot automation. If rollback requires a restart, the workflow stops and the user performs it manually.

The real-device gate may use runtime-only unattended orchestration to loop a local synthetic far-end file through the selected physical render endpoint, continuously consume `MiniAEC Microphone` through an ordinary FFmpeg DirectShow client, retain a private FLAC for later listening review, invoke the unchanged non-elevated AEC harness, and stop only its owned child processes. Early playback or client exit fails the orchestration, while schema version 2 evidence remains authoritative for render activity and counters. Process coverage cannot establish audible continuity, so the orchestration does not synthesize a passing operator sidecar and the functional gate remains inconclusive until a truthful listening observation is supplied.

Raw JSONL, generated reports, endpoint IDs, operator sidecars, and recordings remain ignored private artifacts. Repository documentation may record summarized numerical conclusions and anonymized failure categories, but not private recordings or raw machine identifiers.

### 8. Complete current acceptance at 30 minutes and defer extended duration to evidence

Implementation first validates the analyzer with synthetic constant-rate, positive/negative drift, discontinuity, render-silence, insufficient-coverage, counter-correlation, and failure fixtures. After repository checks pass, a separately approved 30-minute K7/Realtek speakers run is captured and analyzed. If it returns `clock-drift-compensation-required`, this change records the baseline and stops so a new change can design and compare compensation. If it returns `bounded-synchronizer-sufficient` and functional stability passes, the current long-run gate is complete without a mandatory two-hour run. An `inconclusive` result requires improved evidence or a repeat run, not implementation of compensation.

The versioned snapshots, event stream, analyzer, thresholds, and privacy boundary remain the diagnostic baseline for early product versions. A release-specific change may connect this bounded metadata surface to local retention after defining storage, rotation, user disclosure, and support export; this change does not introduce background upload, unbounded retention, or PCM logging. Recurring synchronization maintenance, growing queue pressure, unexplained discontinuity, or another sustained field signal can justify a separate change that selects an evidence-driven duration rather than inheriting a fixed two-hour requirement.

## Risks / Trade-offs

- [One-second snapshots alias short timing excursions] → Use them for long-run rate and counter trends, retain maximum-skew and cumulative counters for short events, and add only bounded observation summaries if synthetic validation proves necessary.
- [Device positions may reset or jump during driver recovery] → Segment at all discontinuity, timestamp-error, epoch, identity, and monotonicity boundaries and never regress across them.
- [Render silence can resemble missing clock data] → Exclude unchanged render-position intervals from drift estimation while preserving them as valid degraded engine behavior and reporting lost coverage.
- [Scheduling jitter can inflate ppm estimates] → Regress many position/QPC observations in five-minute windows, report residual uncertainty, use a median across windows, and require consistent direction.
- [A numeric pass can conceal client or audible failure] → Keep functional acceptance separate and require explicit ordinary-client and listening observations.
- [The target hardware pair does not represent every Windows device combination] → Scope the first conclusion to the K7/Realtek speakers pair and preserve the procedure for later compatibility-matrix runs.
- [A 30-minute gate may miss a rare long-tail stability failure] → Preserve versioned metadata diagnostics for early product versions and require a new evidence-scoped change if field logs show a recurring risk.
- [A real-device run requires privileged lifecycle preparation] → Keep mutation outside automated analysis, require separate approval and rollback, and never initiate a restart.

## Migration Plan

1. Add backward-aware parsing for validation schema version 1 where possible, but require schema version 2 monotonic metadata for an authoritative gate.
2. Introduce the analyzer and synthetic fixtures without changing the real-time audio path, then run formatting, workspace tests, and strict Clippy.
3. Document a read-only analysis workflow and the separately approved real-device capture procedure.
4. Capture and assess the 30-minute target-hardware evidence; update active documentation with the summarized result and next-decision state.
5. Complete the current stability decision from the accepted 30-minute result and carry the privacy-bounded metadata contract into release-specific logging work for evidence-triggered reassessment.

Rollback consists of removing the validation schema additions, analyzer command, fixtures, and documentation while leaving the accepted M3 engine, AEC adapter, synchronizer, transport, and driver unchanged. Real-device package rollback continues to follow `driver/windows/VALIDATION.md`; no change-specific system state is introduced.
