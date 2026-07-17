# Open Denoise

Open Denoise is an experimental desktop application for real-time microphone
echo cancellation and speech noise suppression.

The current repository contains the Tauri 2, Rust, React, and TypeScript
application shell. Audio-engine development will start with a Windows proof of
concept that records a physical microphone, a WASAPI render-loopback reference,
and processed output for comparison.

See [docs/technical-plan.md](docs/technical-plan.md) for the architecture,
validation plan, delivery phases, and engineering constraints.

AEC source pins and local patches are recorded in
[vendor/UPSTREAM.md](vendor/UPSTREAM.md). Future WebRTC upgrades must follow
[docs/upstream-upgrade-plan.md](docs/upstream-upgrade-plan.md).

## Development

```bash
bun install
bun run tauri dev
```

Run the frontend checks with:

```bash
bun run format:check
bun run lint
bun run build
```

Inspect the Windows audio endpoints and their shared-mode formats with:

```powershell
cargo run -p denoise-lab -- devices
```

Record a timestamped microphone and render-loopback diagnostic run with:

```powershell
cargo run -p denoise-lab -- capture --duration 30 --microphone "<physical microphone name or endpoint ID>" --render "<physical speaker name or endpoint ID>"
```

Capture artifacts are written below `artifacts/runs/` and are ignored by Git.
When a virtual microphone such as Krisp is the Windows default, select the
physical microphone explicitly to avoid measuring another processor instead of
the raw device.

Align a captured run using its WASAPI/QPC timestamps and process it through
WebRTC AEC3 with automatic delay estimation:

```powershell
cargo run -p denoise-lab -- aec --run artifacts/runs/<run-id>
```

To compare against a known acoustic-delay hint, add (for example)
`--stream-delay-ms 60`. Each mode writes aligned source tracks,
`aec-output.wav`, and `aec-report.json` below the run's `processed/` directory.

For mechanism-isolation diagnostics, export WebRTC's pre-suppressor linear AEC
signal alongside the complete AEC output:

```powershell
cargo run -p denoise-lab -- aec --run artifacts/runs/<run-id> --export-linear
```

This additionally writes `linear-aec-output-16khz.wav` and a sample-rate-matched
`full-aec-output-16khz.wav`. Linear export is opt-in and does not enable noise
suppression or change the ordinary frozen baseline path.

### Bundled WebRTC build on Windows

The repository pins and patches `webrtc-audio-processing` and its `-sys` crate
so the diagnostic API and bundled WebRTC AEC3 source build reproducibly with
MSVC. Build from an x64 Visual Studio Developer PowerShell with the C++ build
tools installed. The native build also requires:

- Python with `meson`, `ninja`, and `libclang` packages available.
- `LIBCLANG_PATH` pointing to the directory containing `libclang.dll` when it
  cannot be discovered automatically.

The first AEC build compiles the bundled WebRTC C++ sources and is substantially
slower than subsequent Cargo builds.

See [docs/aec-baseline.md](docs/aec-baseline.md) for the current offline
validation result and remaining acceptance tests.
