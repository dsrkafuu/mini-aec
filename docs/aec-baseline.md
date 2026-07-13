# Offline AEC3 baseline

## Scope

This baseline validates timestamp alignment, repeatable offline WebRTC AEC3
processing, and the first controlled double-talk gate. It does not yet validate
a real-time audio pipeline, clock-drift correction, or a virtual microphone.

Test hardware:

- Physical microphone: K7
- Physical render endpoint: Sound Blaster X4
- Format: 48 kHz microphone mono plus render-loopback stereo
- Stimulus: the same approximately 40-second video playback for both captures

The echo-only baseline uses two captures: one with maximum microphone recording
gain and one with normal daily gain. A third normal-gain capture supplies the
controlled double-talk experiment. All are retained only below the Git-ignored
`artifacts/runs/` directory; no private recordings or generated WAV files are
committed.

## Processing path

For each run, `denoise-lab aec`:

1. Reads each stream's first WASAPI QPC timestamp from `manifest.json`.
2. Places both tracks on a common 48 kHz timeline, padding the later stream at
   the beginning and rounding the final length to complete 10 ms frames.
3. Downmixes the stereo render loopback to the mono reverse stream expected by
   the current processor configuration.
4. Submits the render frame before the corresponding microphone frame to WebRTC
   AEC3.
5. Writes aligned inputs, processed output, and a JSON report for listening and
   metric comparison.

Reproduce automatic-delay processing with:

```powershell
cargo run -p denoise-lab -- aec --run artifacts/runs/<run-id>
```

Compare a fixed acoustic delay hint with:

```powershell
cargo run -p denoise-lab -- aec --run artifacts/runs/<run-id> --stream-delay-ms 60
```

## Result

| Microphone gain | QPC start offset (mic - render) | AEC3 delay estimate | Active reduction, adaptive | Active reduction, 60 ms hint |
| --------------- | ------------------------------: | ------------------: | -------------------------: | ---------------------------: |
| Daily           |                       +19.97 ms |               56 ms |                   32.33 dB |                     32.07 dB |
| Maximum         |                       -18.50 ms |               56 ms |                   20.48 dB |                     20.79 dB |

“Active reduction” compares microphone input and AEC output energy only in 10 ms
frames where the render reference exceeds -50 dBFS. This is a useful far-end
echo-only baseline, not a speech-quality score. The fixed 60 ms hint changes the
result by less than 0.4 dB, so automatic delay estimation remains the default.

The shared 56 ms estimate is also consistent with the approximately 60 ms
physical echo delay previously measured by waveform correlation. QPC alignment
correctly handles either stream starting first; it removes capture-start skew,
while AEC3 estimates the separate speaker-to-microphone acoustic path.

## Controlled double-talk

The first controlled double-talk run used normal daily microphone gain and this
schedule:

| Interval | Content      |
| -------- | ------------ |
| 0-7 s    | Far end only |
| 7-13 s   | Double-talk  |
| 13-19 s  | Far end only |
| 19-26 s  | Double-talk  |
| 26-32 s  | Far end only |
| 32-38 s  | Double-talk  |
| 38-40 s  | Far end only |

The frozen default profile removed the video speech completely and achieved
28.85 dB and 32.85 dB input/output reduction in the two converged far-end-only
intervals. Subjective listening found no metallic sound, but did find audible
word-tail loss, pumping, and near-end volume changes. Therefore far-end echo
cancellation passes while near-end speech preservation does not.

Full-recording energy reduction is intentionally not used as the double-talk
score: preserved near-end speech should dominate the output during those
intervals. The listening gate is residual far-end speech, intelligibility,
word-tail preservation, level stability, and recovery after speech.

## Anonymous profile experiment

The next experiment keeps the frozen default as one candidate and adds two
single-mechanism diagnostic profiles:

- `nearend-stable`: changes only dominant-near-end detector timing, entering
  after 6 AEC3 blocks and holding for 200 blocks. This tests whether rapid state
  switching causes pumping.
- `speech-safe`: changes only near-end suppression gain dynamics, using a 4.0
  maximum increase factor and 0.5 low-frequency decrease factor. This tests
  whether slower gain drops and faster recovery preserve speech.

Both candidates use the same WebRTC M131 AEC3 source, QPC alignment, inputs,
frame order, and delay estimator. AEC3 configuration validation must accept
each profile before processing.

Create a randomized A/B/C listening set from the three speech intervals with:

```powershell
cargo run -p denoise-lab -- blind-aec `
  --run artifacts/runs/<run-id> `
  --segment 7-13 --segment 19-26 --segment 32-38
```

The command writes only anonymous `A.wav`, `B.wav`, and `C.wav` files plus
segment metadata into the listening directory. It stores the answer key
separately below the ignored run's `processed/` directory. Do not inspect the
answer key until the listener has ranked all three files.

Objective safety checking on the same run showed:

| Profile         | Far-only 13-19 s | Far-only 26-32 s |
| --------------- | ---------------: | ---------------: |
| Default         |         28.85 dB |         32.85 dB |
| Near-end stable |         28.85 dB |         32.85 dB |
| Speech safe     |         28.85 dB |         31.71 dB |

The candidates retain the far-end echo result closely enough for blind
listening. Acceptance still requires a subjective improvement without audible
video speech returning.

## Interpretation and next gate

The reference signal is usable, AEC3 converges on both gain settings, timestamp
alignment is repeatable, and the default profile removes far-end speech during
double-talk. The current blocker is near-end speech quality.

The active gate is the anonymous A/B/C comparison:

1. Rank A/B/C for voice naturalness and stable volume.
2. For each file, note word-tail loss, pumping, and any returned video speech.
3. Reveal the answer key only after the ranking is recorded.
4. Keep a candidate only if it improves speech subjectively and retains the
   far-end-only safety result.

After double-talk passes, run a longer capture to measure clock drift and then
move the same framing and processor contract into the real-time pipeline.
