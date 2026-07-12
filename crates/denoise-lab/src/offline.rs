use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use serde::{Deserialize, Serialize};
use webrtc_audio_processing::config::EchoCanceller;
use webrtc_audio_processing::{Config, Processor, Stats};

const SAMPLE_RATE: u32 = 48_000;
const QPC_TICKS_PER_SECOND: u64 = 10_000_000;
const FRAME_SAMPLES: usize = 480;

pub struct AecConfig {
  pub run_dir: PathBuf,
  pub stream_delay_ms: Option<u16>,
  pub active_threshold_dbfs: f64,
}

#[derive(Debug, Deserialize)]
struct CaptureManifest {
  tracks: BTreeMap<String, CaptureTrack>,
}

#[derive(Debug, Deserialize)]
struct CaptureTrack {
  output_file: String,
  requested_sample_rate: u32,
  requested_channels: u16,
  stats: CaptureStats,
}

#[derive(Debug, Deserialize)]
struct CaptureStats {
  first_qpc_timestamp_100ns: Option<u64>,
}

#[derive(Debug, Serialize)]
struct AecReport {
  schema_version: u32,
  source_run: String,
  sample_rate: u32,
  frame_samples: usize,
  frames_processed: usize,
  common_timeline_samples: usize,
  microphone_offset_samples: usize,
  render_offset_samples: usize,
  microphone_minus_render_start_ms: f64,
  stream_delay_ms: Option<u16>,
  active_threshold_dbfs: f64,
  metrics: AecMetrics,
  active_frame_stats: Option<AecStats>,
  final_stats: AecStats,
}

#[derive(Debug, Serialize)]
struct AecMetrics {
  microphone_input_rms_dbfs: f64,
  microphone_input_peak_dbfs: f64,
  aec_output_rms_dbfs: f64,
  aec_output_peak_dbfs: f64,
  total_reduction_db: f64,
  render_active_frames: usize,
  active_input_rms_dbfs: f64,
  active_output_rms_dbfs: f64,
  active_reduction_db: f64,
}

#[derive(Clone, Debug, Serialize)]
struct AecStats {
  echo_return_loss_db: Option<f64>,
  echo_return_loss_enhancement_db: Option<f64>,
  residual_echo_likelihood: Option<f64>,
  residual_echo_likelihood_recent_max: Option<f64>,
  delay_ms: Option<u32>,
}

struct AlignedAudio {
  microphone: Vec<f32>,
  render: Vec<f32>,
  microphone_offset_samples: usize,
  render_offset_samples: usize,
  microphone_minus_render_start_ms: f64,
}

pub fn process(config: &AecConfig) -> Result<()> {
  if !config.active_threshold_dbfs.is_finite() {
    bail!("active threshold must be finite");
  }

  let manifest = read_manifest(&config.run_dir)?;
  let microphone_track = manifest_track(&manifest, "microphone")?;
  let render_track = manifest_track(&manifest, "render-reference")?;
  validate_track(microphone_track, 1, "microphone")?;
  validate_track(render_track, 2, "render-reference")?;

  let microphone = read_float_wave(
    &config.run_dir.join(&microphone_track.output_file),
    microphone_track.requested_channels,
  )?;
  let render_channels = read_float_wave(
    &config.run_dir.join(&render_track.output_file),
    render_track.requested_channels,
  )?;
  let render = downmix(&render_channels);

  let microphone_qpc = required_qpc(microphone_track, "microphone")?;
  let render_qpc = required_qpc(render_track, "render-reference")?;
  let aligned = align_tracks(&microphone[0], &render, microphone_qpc, render_qpc);
  let output_dir = output_directory(&config.run_dir, config.stream_delay_ms);
  fs::create_dir_all(&output_dir)
    .with_context(|| format!("failed to create {}", output_dir.display()))?;

  write_mono_wave(
    &output_dir.join("aligned-microphone.wav"),
    &aligned.microphone,
  )?;
  write_mono_wave(
    &output_dir.join("aligned-render-reference.wav"),
    &aligned.render,
  )?;

  let processor = Processor::new(SAMPLE_RATE).context("failed to create WebRTC processor")?;
  processor.set_config(Config {
    echo_canceller: Some(EchoCanceller::Full {
      stream_delay_ms: config.stream_delay_ms,
    }),
    ..Config::default()
  });
  let (aec_output, active_frame_stats) = run_aec(
    &processor,
    &aligned.microphone,
    &aligned.render,
    config.active_threshold_dbfs,
  )?;
  write_mono_wave(&output_dir.join("aec-output.wav"), &aec_output)?;

  let report = AecReport {
    schema_version: 1,
    source_run: config.run_dir.file_name().map_or_else(
      || config.run_dir.display().to_string(),
      |name| name.to_string_lossy().into(),
    ),
    sample_rate: SAMPLE_RATE,
    frame_samples: FRAME_SAMPLES,
    frames_processed: aec_output.len() / FRAME_SAMPLES,
    common_timeline_samples: aec_output.len(),
    microphone_offset_samples: aligned.microphone_offset_samples,
    render_offset_samples: aligned.render_offset_samples,
    microphone_minus_render_start_ms: aligned.microphone_minus_render_start_ms,
    stream_delay_ms: config.stream_delay_ms,
    active_threshold_dbfs: config.active_threshold_dbfs,
    metrics: calculate_metrics(
      &aligned.microphone,
      &aec_output,
      &aligned.render,
      config.active_threshold_dbfs,
    ),
    active_frame_stats,
    final_stats: processor.get_stats().into(),
  };
  let report_path = output_dir.join("aec-report.json");
  fs::write(&report_path, serde_json::to_vec_pretty(&report)?)
    .with_context(|| format!("failed to write {}", report_path.display()))?;

  println!(
    "Aligned microphone offset: {} samples",
    report.microphone_offset_samples
  );
  println!(
    "Aligned render offset: {} samples",
    report.render_offset_samples
  );
  println!(
    "Active echo reduction: {:.2} dB",
    report.metrics.active_reduction_db
  );
  println!(
    "AEC output: {}",
    output_dir.join("aec-output.wav").display()
  );
  println!("Report: {}", report_path.display());
  Ok(())
}

fn read_manifest(run_dir: &Path) -> Result<CaptureManifest> {
  let path = run_dir.join("manifest.json");
  let bytes = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
  serde_json::from_slice(&bytes).with_context(|| format!("failed to parse {}", path.display()))
}

fn manifest_track<'a>(manifest: &'a CaptureManifest, name: &str) -> Result<&'a CaptureTrack> {
  manifest
    .tracks
    .get(name)
    .with_context(|| format!("manifest does not contain {name:?} track"))
}

fn validate_track(track: &CaptureTrack, channels: u16, name: &str) -> Result<()> {
  if track.requested_sample_rate != SAMPLE_RATE {
    bail!(
      "{name} sample rate is {}, expected {SAMPLE_RATE}",
      track.requested_sample_rate
    );
  }
  if track.requested_channels != channels {
    bail!(
      "{name} has {} channels, expected {channels}",
      track.requested_channels
    );
  }
  Ok(())
}

fn required_qpc(track: &CaptureTrack, name: &str) -> Result<u64> {
  track
    .stats
    .first_qpc_timestamp_100ns
    .with_context(|| format!("{name} has no first QPC timestamp"))
}

fn read_float_wave(path: &Path, expected_channels: u16) -> Result<Vec<Vec<f32>>> {
  let mut reader =
    WavReader::open(path).with_context(|| format!("failed to open {}", path.display()))?;
  let spec = reader.spec();
  if spec.sample_rate != SAMPLE_RATE
    || spec.channels != expected_channels
    || spec.bits_per_sample != 32
    || spec.sample_format != SampleFormat::Float
  {
    bail!(
      "{} has unsupported format: {:?}; expected {SAMPLE_RATE} Hz, {expected_channels} ch, f32",
      path.display(),
      spec
    );
  }

  let mut channels = vec![Vec::new(); expected_channels as usize];
  for (index, sample) in reader.samples::<f32>().enumerate() {
    channels[index % expected_channels as usize]
      .push(sample.with_context(|| format!("failed reading {}", path.display()))?);
  }
  Ok(channels)
}

fn downmix(channels: &[Vec<f32>]) -> Vec<f32> {
  let sample_count = channels.iter().map(Vec::len).min().unwrap_or(0);
  let channel_count =
    u16::try_from(channels.len()).expect("audio channel count fits in a 16-bit WAV field");
  (0..sample_count)
    .map(|index| {
      channels.iter().map(|channel| channel[index]).sum::<f32>() / f32::from(channel_count)
    })
    .collect()
}

fn align_tracks(
  microphone: &[f32],
  render: &[f32],
  microphone_qpc: u64,
  render_qpc: u64,
) -> AlignedAudio {
  let origin = microphone_qpc.min(render_qpc);
  let microphone_offset_samples = qpc_delta_to_samples(microphone_qpc - origin);
  let render_offset_samples = qpc_delta_to_samples(render_qpc - origin);
  let common_samples = (microphone_offset_samples + microphone.len())
    .max(render_offset_samples + render.len())
    .div_ceil(FRAME_SAMPLES)
    * FRAME_SAMPLES;
  let mut aligned_microphone = vec![0.0; common_samples];
  let mut aligned_render = vec![0.0; common_samples];
  aligned_microphone[microphone_offset_samples..microphone_offset_samples + microphone.len()]
    .copy_from_slice(microphone);
  aligned_render[render_offset_samples..render_offset_samples + render.len()]
    .copy_from_slice(render);

  AlignedAudio {
    microphone: aligned_microphone,
    render: aligned_render,
    microphone_offset_samples,
    render_offset_samples,
    microphone_minus_render_start_ms: signed_qpc_delta_ms(microphone_qpc, render_qpc),
  }
}

fn qpc_delta_to_samples(qpc_delta: u64) -> usize {
  let numerator =
    u128::from(qpc_delta) * u128::from(SAMPLE_RATE) + u128::from(QPC_TICKS_PER_SECOND / 2);
  usize::try_from(numerator / u128::from(QPC_TICKS_PER_SECOND))
    .expect("aligned recording length fits usize")
}

fn signed_qpc_delta_ms(left: u64, right: u64) -> f64 {
  if left >= right {
    qpc_delta_ms(left - right)
  } else {
    -qpc_delta_ms(right - left)
  }
}

fn qpc_delta_ms(delta: u64) -> f64 {
  let whole_seconds =
    u32::try_from(delta / QPC_TICKS_PER_SECOND).expect("QPC track difference fits u32 seconds");
  let subsecond_ticks =
    u32::try_from(delta % QPC_TICKS_PER_SECOND).expect("QPC subsecond tick count fits u32");
  f64::from(whole_seconds) * 1_000.0
    + f64::from(subsecond_ticks) * 1_000.0
      / f64::from(u32::try_from(QPC_TICKS_PER_SECOND).expect("QPC frequency fits u32"))
}

fn output_directory(run_dir: &Path, stream_delay_ms: Option<u16>) -> PathBuf {
  let mode = stream_delay_ms.map_or_else(
    || "aec-adaptive".to_owned(),
    |delay| format!("aec-delay-{delay}ms"),
  );
  run_dir.join("processed").join(mode)
}

fn run_aec(
  processor: &Processor,
  microphone: &[f32],
  render: &[f32],
  active_threshold_dbfs: f64,
) -> Result<(Vec<f32>, Option<AecStats>)> {
  let threshold_power = 10_f64.powf(active_threshold_dbfs / 10.0);
  let mut output = Vec::with_capacity(microphone.len());
  let mut active_stats = None;

  for (microphone_frame, render_frame) in microphone
    .chunks_exact(FRAME_SAMPLES)
    .zip(render.chunks_exact(FRAME_SAMPLES))
  {
    let mut render_channels = vec![render_frame.to_vec()];
    processor
      .process_render_frame(&mut render_channels)
      .context("WebRTC failed to process render frame")?;

    let mut capture_channels = vec![microphone_frame.to_vec()];
    processor
      .process_capture_frame(&mut capture_channels)
      .context("WebRTC failed to process capture frame")?;
    output.extend_from_slice(&capture_channels[0]);

    if mean_power(render_frame) > threshold_power {
      active_stats = Some(processor.get_stats().into());
    }
  }
  Ok((output, active_stats))
}

fn calculate_metrics(
  input: &[f32],
  output: &[f32],
  render: &[f32],
  active_threshold_dbfs: f64,
) -> AecMetrics {
  let threshold_power = 10_f64.powf(active_threshold_dbfs / 10.0);
  let mut active_input_energy = 0.0;
  let mut active_output_energy = 0.0;
  let mut active_samples = 0;
  let mut active_frames = 0;

  for ((input_frame, output_frame), render_frame) in input
    .chunks_exact(FRAME_SAMPLES)
    .zip(output.chunks_exact(FRAME_SAMPLES))
    .zip(render.chunks_exact(FRAME_SAMPLES))
  {
    if mean_power(render_frame) > threshold_power {
      active_input_energy += energy(input_frame);
      active_output_energy += energy(output_frame);
      active_samples += FRAME_SAMPLES;
      active_frames += 1;
    }
  }

  let input_energy = energy(input);
  let output_energy = energy(output);
  AecMetrics {
    microphone_input_rms_dbfs: rms_dbfs(input_energy, input.len()),
    microphone_input_peak_dbfs: peak_dbfs(input),
    aec_output_rms_dbfs: rms_dbfs(output_energy, output.len()),
    aec_output_peak_dbfs: peak_dbfs(output),
    total_reduction_db: reduction_db(input_energy, output_energy),
    render_active_frames: active_frames,
    active_input_rms_dbfs: rms_dbfs(active_input_energy, active_samples),
    active_output_rms_dbfs: rms_dbfs(active_output_energy, active_samples),
    active_reduction_db: reduction_db(active_input_energy, active_output_energy),
  }
}

fn mean_power(samples: &[f32]) -> f64 {
  energy(samples) / sample_count_f64(samples.len())
}

fn energy(samples: &[f32]) -> f64 {
  samples
    .iter()
    .map(|sample| f64::from(*sample).powi(2))
    .sum()
}

fn rms_dbfs(energy: f64, samples: usize) -> f64 {
  10.0 * (energy / sample_count_f64(samples)).max(1e-24).log10()
}

fn sample_count_f64(samples: usize) -> f64 {
  f64::from(u32::try_from(samples.max(1)).expect("audio buffer length fits u32 samples"))
}

fn peak_dbfs(samples: &[f32]) -> f64 {
  20.0
    * samples
      .iter()
      .map(|sample| f64::from(sample.abs()))
      .fold(0.0, f64::max)
      .max(1e-12)
      .log10()
}

fn reduction_db(input_energy: f64, output_energy: f64) -> f64 {
  10.0 * (input_energy / output_energy.max(1e-24)).max(1e-24).log10()
}

fn write_mono_wave(path: &Path, samples: &[f32]) -> Result<()> {
  let mut writer = WavWriter::create(
    path,
    WavSpec {
      channels: 1,
      sample_rate: SAMPLE_RATE,
      bits_per_sample: 32,
      sample_format: SampleFormat::Float,
    },
  )
  .with_context(|| format!("failed to create {}", path.display()))?;
  for sample in samples {
    writer.write_sample(*sample)?;
  }
  writer
    .finalize()
    .with_context(|| format!("failed to finalize {}", path.display()))
}

impl From<Stats> for AecStats {
  fn from(stats: Stats) -> Self {
    Self {
      echo_return_loss_db: stats.echo_return_loss,
      echo_return_loss_enhancement_db: stats.echo_return_loss_enhancement,
      residual_echo_likelihood: stats.residual_echo_likelihood,
      residual_echo_likelihood_recent_max: stats.residual_echo_likelihood_recent_max,
      delay_ms: stats.delay_ms,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn qpc_delta_rounds_to_nearest_sample() {
    assert_eq!(qpc_delta_to_samples(100_000), 480);
    assert_eq!(qpc_delta_to_samples(50_000), 240);
  }

  #[test]
  fn alignment_uses_the_earliest_qpc_as_origin() {
    let aligned = align_tracks(&[1.0, 2.0], &[3.0, 4.0], 100_100, 100_000);
    assert_eq!(aligned.render_offset_samples, 0);
    assert_eq!(aligned.microphone_offset_samples, 0);

    let aligned = align_tracks(&[1.0], &[2.0], 200_000, 100_000);
    assert_eq!(aligned.render_offset_samples, 0);
    assert_eq!(aligned.microphone_offset_samples, 480);
    assert!((aligned.microphone[480] - 1.0).abs() < f32::EPSILON);
    assert!((aligned.render[0] - 2.0).abs() < f32::EPSILON);
  }
}
