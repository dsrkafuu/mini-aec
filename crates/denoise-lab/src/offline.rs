use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use serde::{Deserialize, Serialize};
use webrtc_audio_processing::config::EchoCanceller;
use webrtc_audio_processing::experimental::EchoCanceller3Config;
use webrtc_audio_processing::{Config, Processor, Stats};

const SAMPLE_RATE: u32 = 48_000;
const QPC_TICKS_PER_SECOND: u64 = 10_000_000;
const FRAME_SAMPLES: usize = 480;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum AecProfile {
  /// Frozen WebRTC M131 defaults.
  #[default]
  Default,
  /// Enter near-end mode sooner and hold it longer to reduce state pumping.
  NearendStable,
  /// Slow gain drops and allow faster recovery while near-end speech is present.
  SpeechSafe,
}

pub struct AecConfig {
  pub run_dir: PathBuf,
  pub stream_delay_ms: Option<u16>,
  pub active_threshold_dbfs: f64,
  pub profile: AecProfile,
}

pub struct BlindAecConfig {
  pub run_dir: PathBuf,
  pub segments: Vec<String>,
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
  profile: AecProfile,
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

struct ProcessOutput {
  output_dir: PathBuf,
  aec_output: Vec<f32>,
}

#[derive(Clone, Debug, Serialize)]
struct ListeningSegment {
  start_seconds: f64,
  end_seconds: f64,
}

#[derive(Debug, Serialize)]
struct BlindManifest {
  schema_version: u32,
  source_run: String,
  experiment_id: String,
  segments: Vec<ListeningSegment>,
  files: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BlindAnswerKey {
  schema_version: u32,
  experiment_id: String,
  assignments: BTreeMap<String, BlindAssignment>,
}

#[derive(Debug, Serialize)]
struct BlindAssignment {
  profile: AecProfile,
  source_report: String,
}

pub fn process(config: &AecConfig) -> Result<()> {
  process_internal(config).map(|_| ())
}

fn process_internal(config: &AecConfig) -> Result<ProcessOutput> {
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
  let output_dir = output_directory(&config.run_dir, config.stream_delay_ms, config.profile);
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

  let processor = create_processor(config.profile)?;
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
    profile: config.profile,
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

  println!("AEC profile: {:?}", config.profile);
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
  Ok(ProcessOutput {
    output_dir,
    aec_output,
  })
}

pub fn build_blind_experiment(config: &BlindAecConfig) -> Result<()> {
  if !config.active_threshold_dbfs.is_finite() {
    bail!("active threshold must be finite");
  }
  let segments = config
    .segments
    .iter()
    .map(|value| parse_segment(value))
    .collect::<Result<Vec<_>>>()?;

  let profiles = [
    AecProfile::Default,
    AecProfile::NearendStable,
    AecProfile::SpeechSafe,
  ];
  let mut outputs = Vec::with_capacity(profiles.len());
  for profile in profiles {
    let output = process_internal(&AecConfig {
      run_dir: config.run_dir.clone(),
      stream_delay_ms: None,
      active_threshold_dbfs: config.active_threshold_dbfs,
      profile,
    })?;
    outputs.push((profile, output));
  }

  let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .context("system clock is before the Unix epoch")?;
  let experiment_id = now.as_nanos().to_string();
  let experiment_dir = config
    .run_dir
    .join("processed")
    .join(format!("blind-aec-{experiment_id}"));
  fs::create_dir_all(&experiment_dir)
    .with_context(|| format!("failed to create {}", experiment_dir.display()))?;

  let order = randomized_profile_order(now);
  let mut assignments = BTreeMap::new();
  let mut files = Vec::with_capacity(order.len());
  for (index, profile) in order.iter().enumerate() {
    let label = char::from(b'A' + u8::try_from(index).expect("three labels fit u8"));
    let file_name = format!("{label}.wav");
    let source = outputs
      .iter()
      .find(|(candidate, _)| candidate == profile)
      .expect("all blind profiles were processed");
    let listening_audio = extract_segments(&source.1.aec_output, &segments)?;
    write_mono_wave(&experiment_dir.join(&file_name), &listening_audio)?;
    assignments.insert(
      label.to_string(),
      BlindAssignment {
        profile: *profile,
        source_report: source
          .1
          .output_dir
          .join("aec-report.json")
          .display()
          .to_string(),
      },
    );
    files.push(file_name);
  }

  let source_run = config.run_dir.file_name().map_or_else(
    || config.run_dir.display().to_string(),
    |name| name.to_string_lossy().into(),
  );
  let manifest = BlindManifest {
    schema_version: 1,
    source_run,
    experiment_id: experiment_id.clone(),
    segments,
    files,
  };
  let manifest_path = experiment_dir.join("manifest.json");
  fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)
    .with_context(|| format!("failed to write {}", manifest_path.display()))?;

  let answer_key = BlindAnswerKey {
    schema_version: 1,
    experiment_id: experiment_id.clone(),
    assignments,
  };
  let answer_key_path = config
    .run_dir
    .join("processed")
    .join(format!(".blind-aec-{experiment_id}-answer-key.json"));
  fs::write(&answer_key_path, serde_json::to_vec_pretty(&answer_key)?)
    .with_context(|| format!("failed to write {}", answer_key_path.display()))?;

  println!("Blind listening directory: {}", experiment_dir.display());
  println!("Listen to A.wav, B.wav, and C.wav without inspecting the answer key.");
  Ok(())
}

fn create_processor(profile: AecProfile) -> Result<Processor> {
  if profile == AecProfile::Default {
    return Processor::new(SAMPLE_RATE).context("failed to create WebRTC processor");
  }

  let mut aec3_config = EchoCanceller3Config::default();
  match profile {
    AecProfile::Default => unreachable!("default profile returned above"),
    AecProfile::NearendStable => {
      let detector = &mut aec3_config.suppressor.dominant_nearend_detection;
      detector.trigger_threshold = 6;
      detector.hold_duration = 200;
    }
    AecProfile::SpeechSafe => {
      let tuning = &mut aec3_config.suppressor.nearend_tuning;
      tuning.max_inc_factor = 4.0;
      tuning.max_dec_factor_lf = 0.5;
    }
  }
  if !aec3_config.validate() {
    bail!("AEC3 rejected the {profile:?} profile");
  }
  Processor::with_aec3_config(SAMPLE_RATE, aec3_config)
    .context("failed to create WebRTC processor with experimental AEC3 config")
}

fn parse_segment(value: &str) -> Result<ListeningSegment> {
  let (start, end) = value
    .split_once('-')
    .with_context(|| format!("invalid segment {value:?}; expected START-END in seconds"))?;
  let start_seconds = start
    .parse::<f64>()
    .with_context(|| format!("invalid segment start in {value:?}"))?;
  let end_seconds = end
    .parse::<f64>()
    .with_context(|| format!("invalid segment end in {value:?}"))?;
  if !start_seconds.is_finite()
    || !end_seconds.is_finite()
    || start_seconds < 0.0
    || end_seconds <= start_seconds
  {
    bail!("invalid segment {value:?}; require finite 0 <= START < END");
  }
  Ok(ListeningSegment {
    start_seconds,
    end_seconds,
  })
}

fn extract_segments(samples: &[f32], segments: &[ListeningSegment]) -> Result<Vec<f32>> {
  let gap_samples = usize::try_from(SAMPLE_RATE / 4).expect("sample rate fits usize");
  let mut output = Vec::new();
  for (index, segment) in segments.iter().enumerate() {
    let start = seconds_to_samples(segment.start_seconds)?;
    let end = seconds_to_samples(segment.end_seconds)?;
    if end > samples.len() {
      bail!(
        "segment {:.3}-{:.3} exceeds the processed recording",
        segment.start_seconds,
        segment.end_seconds
      );
    }
    if index > 0 {
      output.resize(output.len() + gap_samples, 0.0);
    }
    output.extend_from_slice(&samples[start..end]);
  }
  Ok(output)
}

fn seconds_to_samples(seconds: f64) -> Result<usize> {
  let duration = Duration::try_from_secs_f64(seconds)
    .with_context(|| format!("invalid non-negative time {seconds}"))?;
  let samples = u128::from(duration.as_secs()) * u128::from(SAMPLE_RATE)
    + u128::from(duration.subsec_nanos()) * u128::from(SAMPLE_RATE) / 1_000_000_000;
  usize::try_from(samples).context("segment time exceeds addressable audio length")
}

fn randomized_profile_order(now: Duration) -> [AecProfile; 3] {
  let mut order = [
    AecProfile::Default,
    AecProfile::NearendStable,
    AecProfile::SpeechSafe,
  ];
  let rotation = usize::try_from(now.as_nanos() % 3).expect("rotation is below three");
  order.rotate_left(rotation);
  if (now.as_nanos() / 3) % 2 == 1 {
    order.swap(0, 1);
  }
  order
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

fn output_directory(run_dir: &Path, stream_delay_ms: Option<u16>, profile: AecProfile) -> PathBuf {
  let mode = stream_delay_ms.map_or_else(
    || match profile {
      AecProfile::Default => "aec-adaptive".to_owned(),
      AecProfile::NearendStable => "aec-nearend-stable".to_owned(),
      AecProfile::SpeechSafe => "aec-speech-safe".to_owned(),
    },
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
