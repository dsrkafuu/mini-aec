# WebRTC AEC upstream upgrade plan

## Decision

The current WebRTC M131 AEC3 implementation is a frozen product baseline, not a
rolling dependency. MiniAEC monitors upstream development but upgrades
only through a measured candidate process.

This separation is necessary because Google WebRTC `main` is a Chromium
development branch, FreeDesktop maintains a distribution-oriented source
extraction and build system, and the Rust wrapper warns that minor releases
within the same major version may contain API-breaking changes.

## Upgrade triggers

Open an upgrade evaluation when at least one condition applies:

1. A measured failure in the current baseline has a specific upstream fix,
   such as double-talk voice loss, residual nonlinear echo, unstable delay,
   slow echo-path recovery, or clock-drift failure.
2. A stable FreeDesktop release moves to a newer WebRTC milestone and exposes a
   relevant AEC3 improvement.
3. A matching Rust wrapper release makes that source buildable and supportable.
4. A supported Windows/MSVC toolchain can no longer build the pinned version.
5. A security, correctness, or licensing issue requires replacement.
6. The project is preparing a Beta or major release and schedules a dependency
   refresh.

New files or experiments on Google WebRTC `main`, including neural residual
echo estimation, are signals to investigate rather than automatic upgrade
triggers.

## Monitoring cadence

Perform a lightweight read-only review every one or two months and before each
major release:

- Google WebRTC AEC3 commit log and Chromium milestone notes.
- FreeDesktop `webrtc-audio-processing` releases and Windows patches.
- `tonarino/webrtc-audio-processing` releases, issues, and build changes.
- MSVC, Meson, Ninja, bindgen, and libclang compatibility relevant to the
  bundled Windows build.

Record only actionable findings. Routine monitoring must not rewrite the
vendor tree or update `Cargo.lock`.

## Candidate preparation

1. Create an isolated candidate branch from a clean baseline.
2. Record before editing:
   - Google WebRTC milestone and exact commit.
   - FreeDesktop version and exact commit.
   - Rust wrapper version and exact commit/tag.
   - Release notes or upstream fixes motivating the candidate.
3. Import the stable source snapshot without carrying build outputs or Git
   metadata.
4. Reapply every local patch listed in `vendor/UPSTREAM.md` individually.
5. Remove obsolete patches and explain why; never silently drop one.
6. Update `vendor/UPSTREAM.md`, Cargo pins, checksums, licenses, and notices in
   the same candidate commit.
7. Compile and run unit tests before producing any acoustic comparison.

Prefer keeping WebRTC-specific code behind the project-owned `EchoCanceller`
adapter. Until side-by-side engines exist in one binary, run the baseline commit
and candidate commit separately against byte-identical input tracks and use
distinct output directories.

## Regression corpus

Each candidate must process the same inputs as the frozen baseline:

| Scenario         | Required variation                                | Primary risk                    |
| ---------------- | ------------------------------------------------- | ------------------------------- |
| Far-end only     | Low, normal, and high speaker level               | Residual echo and convergence   |
| Near-end only    | Quiet and normal local speech                     | Unwanted voice coloration       |
| Double-talk      | Speech at startup, middle, and after convergence  | Swallowed syllables and pumping |
| Echo-path change | Move or rotate microphone/speaker during playback | Reset and recovery time         |
| Nonlinear path   | High speaker level and controlled mic clipping    | Residual distorted echo         |
| Delay change     | Buffer/device disturbance where reproducible      | Loss of alignment               |
| Long run         | At least 30 minutes, later the two-hour gate      | Clock drift and stability       |

Private room recordings stay under ignored `artifacts/`. Redistributable
automated regression material belongs under `testdata/` only when its source and
license are recorded. A high-quality double-talk corpus should include an
isolated near-end reference when possible so voice preservation can be measured
instead of judged only by output energy.

## Measurements

For both baseline and candidate record:

- Active far-end echo reduction, ERL/ERLE, residual echo likelihood, delay
  estimate, and convergence time.
- Near-end level and spectral change during near-end-only and double-talk
  regions.
- Audible residual echo, pumping, metallic artifacts, clipped word starts or
  endings, and recovery after an echo-path change.
- CPU time per 10 ms frame, P50/P95/P99 processing time, peak memory, and output
  latency.
- Discontinuities, underruns, overruns, resets, non-finite output, crashes, and
  build/package failures.

Objective suppression metrics do not override near-end speech quality. A
candidate that removes more echo by damaging local speech fails.

## Acceptance gates

A candidate can replace the baseline only when all conditions pass:

1. Far-end-only performance does not materially regress on any retained
   baseline and improves the scenario that motivated the upgrade.
2. Double-talk and near-end-only listening show no new persistent swallowing,
   pumping, metallic artifacts, or clipped syllable endings.
3. Delay acquisition, echo-path recovery, and long-run drift are no worse than
   the baseline.
4. The real-time processing budget remains within the gates in
   `docs/technical-plan.md`.
5. Windows x64 build, tests, strict Clippy, application build, and packaging all
   pass from documented prerequisites.
6. Source pins, local patches, licenses, and comparison reports are complete
   and reviewable.

Use approximately 1 dB as an investigation threshold for far-end ERLE changes,
not as an automatic pass/fail rule. Room recordings vary, so repeated runs and
listening remain required.

## Rollout and rollback

Land an accepted upgrade as its own bounded commit. Do not combine it with
capture, synchronization, tray, virtual-driver, or audio-format changes. Preserve the prior
pin and benchmark report in Git history and document the command needed to
reproduce the comparison.

During product rollout, keep the previous AEC adapter available until the new
version passes extended real-world use. Any crash, non-finite output, severe
double-talk regression, or repeatable loss of echo cancellation is a rollback
condition.
