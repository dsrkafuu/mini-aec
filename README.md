# Open Denoise

Open Denoise is an experimental desktop application for real-time microphone
echo cancellation and speech noise suppression.

The current repository contains the Tauri 2, Rust, React, and TypeScript
application shell. Audio-engine development will start with a Windows proof of
concept that records a physical microphone, a WASAPI render-loopback reference,
and processed output for comparison.

See [docs/technical-plan.md](docs/technical-plan.md) for the architecture,
validation plan, delivery phases, and engineering constraints.

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
