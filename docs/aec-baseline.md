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

## Anonymous profile experiment: round 1

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

The revealed listening result was `speech-safe > default > nearend-stable`.
`speech-safe` and the default were difficult to distinguish, while
`nearend-stable` caused clearly worse swallowed speech. This rejects the
earlier/longer dominant-near-end state direction. The combined `speech-safe`
change is not yet a meaningful win, but it is the only direction worth
decomposing.

## Anonymous profile experiment: round 2

Round 2 uses the same source recording and splits `speech-safe` into two
strictly single-variable profiles:

- `recovery-fast` changes only near-end `max_inc_factor` from 2.0 to 4.0.
- `drop-smooth` changes only near-end `max_dec_factor_lf` from 0.25 to 0.5.

The default remains the third candidate. Generate this exact round with:

```powershell
cargo run -p denoise-lab -- blind-aec `
  --run artifacts/runs/<run-id> `
  --segment 7-13 --segment 19-26 --segment 32-38 `
  --profile default --profile recovery-fast --profile drop-smooth
```

The blind command requires exactly three unique profiles when `--profile` is
used. Omitting all profile arguments preserves the round 1 candidate set.

Objective safety checking before listening showed:

| Profile       | Far-only 13-19 s | Far-only 26-32 s |
| ------------- | ---------------: | ---------------: |
| Default       |         28.85 dB |         32.85 dB |
| Recovery fast |         28.85 dB |         31.78 dB |
| Drop smooth   |         28.85 dB |         32.71 dB |

Both candidates remain eligible for blind listening. The round 2 answer key
must remain sealed until the listener records the ranking and swallowing,
pumping, level-stability, and returned-video-speech observations.

The revealed round 2 result was
`recovery-fast > drop-smooth > default`. The default had the most severe
swallowing, while none of the three returned meaningful video speech. This
identifies the near-end gain increase limit, rather than the decrease limit, as
the primary useful variable. `recovery-fast` becomes the leading candidate;
`drop-smooth` is not combined with it because the round 1 combination was
difficult to distinguish from the default.

## Anonymous profile experiment: round 3

Round 3 is a dose-response test of the winning variable using the same source:

- `default`: near-end `max_inc_factor = 2.0`.
- `recovery-fast`: near-end `max_inc_factor = 4.0`.
- `recovery-faster`: near-end `max_inc_factor = 8.0`.

Generate the round with:

```powershell
cargo run -p denoise-lab -- blind-aec `
  --run artifacts/runs/<run-id> `
  --segment 7-13 --segment 19-26 --segment 32-38 `
  --profile default --profile recovery-fast --profile recovery-faster
```

Objective safety checking before listening showed:

| Profile         | Far-only 13-19 s | Far-only 26-32 s |
| --------------- | ---------------: | ---------------: |
| Default         |         28.85 dB |         32.85 dB |
| Recovery fast   |         28.85 dB |         31.78 dB |
| Recovery faster |         28.85 dB |         31.36 dB |

The 8.0 candidate costs another 0.42 dB in the later far-end-only interval
relative to 4.0, but remains above 31 dB and is eligible for blind listening.
The acceptance question is whether the additional speech preservation is
audible without returned video speech or new pumping.

The anonymous result was that the three files were too similar to rank
reliably. The earlier 4.0 preference therefore did not reproduce as a clear
dose response. Keep the frozen default and stop tuning `max_inc_factor` until a
specific processing stage is identified.

## Linear AEC mechanism isolation

The fixed M131 wrapper now exposes upstream
`AudioProcessing::GetLinearAecOutput` as an opt-in diagnostic. Run it against
the same captured input with:

```powershell
cargo run -p denoise-lab -- aec `
  --run artifacts/runs/<run-id> `
  --export-linear
```

The diagnostic writes:

- `linear-aec-output-16khz.wav`: the mono 16 kHz output after linear echo
  cancellation and before the residual echo suppressor.
- `full-aec-output-16khz.wav`: the complete AEC output downsampled with a fixed
  low-pass FIR for sample-rate-matched listening.
- `aec-output.wav`: the unchanged complete 48 kHz output.

On the controlled 40 s run, all 4,000 linear frames were available. The 48 kHz
complete output was byte-identical to the existing frozen default output, so
enabling diagnostic export did not alter the baseline. Segment RMS comparison
showed the complete suppressor was 20.23 dB and 23.45 dB below the linear output
in the two converged far-end-only intervals. Across the three double-talk
intervals it was only 0.71-0.98 dB lower overall; listening is still required
because brief word-tail suppression can be hidden by interval RMS.

## Interpretation and next gate

The reference signal is usable, AEC3 converges on both gain settings, timestamp
alignment is repeatable, and the default profile removes far-end speech during
double-talk. The current blocker is near-end speech quality.

The active gate is the linear/full mechanism-isolation comparison:

1. Compare the same double-talk intervals in the two 16 kHz files.
2. Note which file preserves word tails, avoids pumping, and keeps near-end
   volume stable.
3. Separately note whether the linear output returns audible video speech.
4. If linear preserves near-end speech better, investigate residual echo
   suppressor controls. If it does not, investigate the linear canceller,
   double-talk detection, and alignment before any more suppressor tuning.

After double-talk passes, run a longer capture to measure clock drift and then
move the same framing and processor contract into the real-time pipeline.
