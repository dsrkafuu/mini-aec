# Offline AEC3 baseline

## Scope

This baseline validates the first half of milestone M2: timestamp alignment and
repeatable offline WebRTC AEC3 processing. It does not yet validate a real-time
audio pipeline, clock-drift correction, double-talk preservation, or a virtual
microphone.

Test hardware:

- Physical microphone: K7
- Physical render endpoint: Sound Blaster X4
- Format: 48 kHz microphone mono plus render-loopback stereo
- Stimulus: the same approximately 40-second video playback for both captures

Two captures are retained locally below the Git-ignored `artifacts/runs/`
directory: one with maximum microphone recording gain and one with normal daily
gain. No private recordings or generated WAV files are committed.

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

## Interpretation and next gate

This result is strong enough to proceed: the reference signal is usable, AEC3
converges on both gain settings, and timestamp alignment is repeatable. It does
not prove Krisp-like call quality because the current stimulus contains no
near-end speech.

The next highest-value recording is controlled double-talk:

1. Play the same far-end video through Sound Blaster X4.
2. Speak at normal volume near K7 during three intervals: near the beginning,
   middle, and end.
3. Include several seconds of far-end-only playback before the first speech so
   AEC3 can converge.
4. Compare aligned microphone and AEC output for residual far-end speech,
   near-end voice damage, pumping, and recovery after double-talk.

After double-talk passes, run a longer capture to measure clock drift and then
move the same framing and processor contract into the real-time pipeline.
