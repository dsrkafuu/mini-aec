use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::mpsc::{self, SyncSender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use hound::{SampleFormat, WavSpec, WavWriter};
use serde::Serialize;
use wasapi::{
  initialize_mta, Device, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat,
};

use crate::platform::CaptureConfig;

const SAMPLE_RATE: u32 = 48_000;
const CHANNEL_CAPACITY: usize = 128;
const EVENT_WAIT_MS: u32 = 100;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Track {
  Microphone,
  RenderReference,
}

impl Track {
  const fn channels(self) -> u16 {
    match self {
      Self::Microphone => 1,
      Self::RenderReference => 2,
    }
  }

  const fn filename(self) -> &'static str {
    match self {
      Self::Microphone => "microphone.wav",
      Self::RenderReference => "render-reference.wav",
    }
  }

  const fn direction(self) -> Direction {
    match self {
      Self::Microphone => Direction::Capture,
      Self::RenderReference => Direction::Render,
    }
  }
}

#[derive(Debug, Serialize)]
struct DeviceDescriptor {
  id: String,
  name: String,
  native_sample_rate: u32,
  native_channels: u16,
  native_bits_per_sample: u16,
}

#[derive(Debug, Serialize)]
struct BufferMetadata {
  device_position: u64,
  qpc_timestamp_100ns: u64,
  data_discontinuity: bool,
  silent: bool,
  timestamp_error: bool,
}

#[derive(Debug)]
enum CaptureMessage {
  Started {
    track: Track,
    device: DeviceDescriptor,
  },
  Packet {
    track: Track,
    frames: u32,
    bytes: Vec<u8>,
    metadata: BufferMetadata,
  },
  Stopped {
    track: Track,
  },
  Failed {
    track: Track,
    error: String,
  },
}

#[derive(Debug, Default, Serialize)]
struct TrackStats {
  packets: u64,
  frames: u64,
  silent_packets: u64,
  discontinuities: u64,
  timestamp_errors: u64,
  first_device_position: Option<u64>,
  last_device_position: Option<u64>,
  first_qpc_timestamp_100ns: Option<u64>,
  last_qpc_timestamp_100ns: Option<u64>,
}

#[derive(Debug, Serialize)]
struct TrackManifest {
  device: DeviceDescriptor,
  output_file: String,
  requested_sample_rate: u32,
  requested_channels: u16,
  stats: TrackStats,
}

#[derive(Debug, Serialize)]
struct RunManifest {
  schema_version: u32,
  created_unix_ms: u128,
  requested_duration_ms: u128,
  tracks: BTreeMap<Track, TrackManifest>,
  failures: BTreeMap<Track, String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum EventRecord<'a> {
  Started {
    track: Track,
    device: &'a DeviceDescriptor,
  },
  Packet {
    track: Track,
    frames: u32,
    metadata: &'a BufferMetadata,
  },
  Stopped {
    track: Track,
  },
  Failed {
    track: Track,
    error: &'a str,
  },
}

struct TrackWriter {
  writer: WavWriter<BufWriter<File>>,
  manifest: TrackManifest,
}

#[allow(
  clippy::too_many_lines,
  reason = "run orchestration is intentionally kept in event order"
)]
pub fn capture(config: CaptureConfig) -> Result<()> {
  if config.duration.is_zero() {
    bail!("capture duration must be greater than zero");
  }

  let created_unix_ms = unix_time_ms()?;
  let run_dir = config.output_root.join(created_unix_ms.to_string());
  fs::create_dir_all(&run_dir)
    .with_context(|| format!("failed to create {}", run_dir.display()))?;

  let (sender, receiver) = mpsc::sync_channel(CHANNEL_CAPACITY);
  let microphone = spawn_capture_thread(
    Track::Microphone,
    config.microphone_selector,
    config.duration,
    sender.clone(),
  )?;
  let render = spawn_capture_thread(
    Track::RenderReference,
    config.render_selector,
    config.duration,
    sender,
  )?;

  let events_path = run_dir.join("events.jsonl");
  let mut events = BufWriter::new(
    File::create(&events_path)
      .with_context(|| format!("failed to create {}", events_path.display()))?,
  );
  let mut writers = BTreeMap::<Track, TrackWriter>::new();
  let mut failures = BTreeMap::<Track, String>::new();
  let mut finished = 0_u8;

  while finished < 2 {
    let message = receiver
      .recv()
      .context("capture threads ended before reporting completion")?;

    match message {
      CaptureMessage::Started { track, device } => {
        write_event(
          &mut events,
          &EventRecord::Started {
            track,
            device: &device,
          },
        )?;
        println!(
          "{track:?}: {} (native {} Hz, {} ch) -> {} Hz, {} ch",
          device.name,
          device.native_sample_rate,
          device.native_channels,
          SAMPLE_RATE,
          track.channels()
        );
        writers.insert(track, create_track_writer(&run_dir, track, device)?);
      }
      CaptureMessage::Packet {
        track,
        frames,
        bytes,
        metadata,
      } => {
        write_event(
          &mut events,
          &EventRecord::Packet {
            track,
            frames,
            metadata: &metadata,
          },
        )?;
        let writer = writers
          .get_mut(&track)
          .ok_or_else(|| anyhow!("{track:?} sent audio before its start event"))?;
        write_packet(writer, frames, &bytes, &metadata)?;
      }
      CaptureMessage::Stopped { track } => {
        write_event(&mut events, &EventRecord::Stopped { track })?;
        finished += 1;
      }
      CaptureMessage::Failed { track, error } => {
        write_event(
          &mut events,
          &EventRecord::Failed {
            track,
            error: &error,
          },
        )?;
        failures.insert(track, error);
        finished += 1;
      }
    }
  }

  microphone
    .join()
    .map_err(|_| anyhow!("microphone capture thread panicked"))?;
  render
    .join()
    .map_err(|_| anyhow!("render capture thread panicked"))?;
  events.flush().context("failed to flush capture events")?;

  let tracks = finalize_writers(writers)?;
  let manifest = RunManifest {
    schema_version: 1,
    created_unix_ms,
    requested_duration_ms: config.duration.as_millis(),
    tracks,
    failures,
  };
  let manifest_path = run_dir.join("manifest.json");
  fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)
    .with_context(|| format!("failed to write {}", manifest_path.display()))?;

  println!("Capture artifacts: {}", run_dir.display());

  if manifest.failures.is_empty() {
    Ok(())
  } else {
    bail!("one or more capture tracks failed; see manifest.json")
  }
}

fn spawn_capture_thread(
  track: Track,
  selector: Option<String>,
  duration: Duration,
  sender: SyncSender<CaptureMessage>,
) -> Result<thread::JoinHandle<()>> {
  thread::Builder::new()
    .name(format!("{track:?}"))
    .spawn(move || {
      if let Err(error) = capture_track(track, selector.as_deref(), duration, &sender) {
        let _ = sender.send(CaptureMessage::Failed {
          track,
          error: format!("{error:#}"),
        });
      }
    })
    .with_context(|| format!("failed to spawn {track:?} capture thread"))
}

#[allow(
  clippy::too_many_lines,
  reason = "WASAPI setup and teardown must remain visibly paired"
)]
fn capture_track(
  track: Track,
  selector: Option<&str>,
  duration: Duration,
  sender: &SyncSender<CaptureMessage>,
) -> Result<()> {
  initialize_mta()
    .ok()
    .context("failed to initialize COM in capture thread")?;

  let enumerator = DeviceEnumerator::new().context("failed to create device enumerator")?;
  let device = select_device(&enumerator, track.direction(), selector)?;
  let native_format = device
    .get_device_format()
    .context("failed to read native device format")?;
  let descriptor = DeviceDescriptor {
    id: device.get_id().context("failed to read device ID")?,
    name: device
      .get_friendlyname()
      .context("failed to read device friendly name")?,
    native_sample_rate: native_format.get_samplespersec(),
    native_channels: native_format.get_nchannels(),
    native_bits_per_sample: native_format.get_validbitspersample(),
  };

  let mut audio_client = device
    .get_iaudioclient()
    .context("failed to create WASAPI audio client")?;
  let requested_format = WaveFormat::new(
    32,
    32,
    &SampleType::Float,
    SAMPLE_RATE as usize,
    track.channels() as usize,
    None,
  );
  let (_, minimum_period) = audio_client
    .get_device_period()
    .context("failed to read WASAPI device period")?;
  let mode = StreamMode::EventsShared {
    autoconvert: true,
    buffer_duration_hns: minimum_period,
  };
  audio_client
    .initialize_client(&requested_format, &Direction::Capture, &mode)
    .context("failed to initialize WASAPI capture stream")?;
  let event = audio_client
    .set_get_eventhandle()
    .context("failed to create WASAPI event handle")?;
  let buffer_frames = audio_client
    .get_buffer_size()
    .context("failed to read WASAPI buffer size")?;
  let capture_client = audio_client
    .get_audiocaptureclient()
    .context("failed to create WASAPI capture client")?;
  let bytes_per_frame = requested_format.get_blockalign() as usize;
  let mut buffer = vec![0_u8; buffer_frames as usize * bytes_per_frame];

  sender
    .send(CaptureMessage::Started {
      track,
      device: descriptor,
    })
    .context("capture writer disconnected")?;

  audio_client
    .start_stream()
    .context("failed to start WASAPI capture stream")?;
  let deadline = Instant::now() + duration;

  while Instant::now() < deadline {
    let _ = event.wait_for_event(EVENT_WAIT_MS);

    loop {
      let Some(packet_frames) = capture_client
        .get_next_packet_size()
        .context("failed to query WASAPI packet size")?
      else {
        break;
      };
      if packet_frames == 0 {
        break;
      }

      let required_bytes = packet_frames as usize * bytes_per_frame;
      if buffer.len() < required_bytes {
        buffer.resize(required_bytes, 0);
      }
      let (frames, info) = capture_client
        .read_from_device(&mut buffer[..required_bytes])
        .context("failed to read WASAPI capture packet")?;
      let bytes = buffer[..frames as usize * bytes_per_frame].to_vec();
      sender
        .send(CaptureMessage::Packet {
          track,
          frames,
          bytes,
          metadata: BufferMetadata {
            device_position: info.index,
            qpc_timestamp_100ns: info.timestamp,
            data_discontinuity: info.flags.data_discontinuity,
            silent: info.flags.silent,
            timestamp_error: info.flags.timestamp_error,
          },
        })
        .context("capture writer disconnected")?;
    }
  }

  audio_client
    .stop_stream()
    .context("failed to stop WASAPI capture stream")?;
  sender
    .send(CaptureMessage::Stopped { track })
    .context("capture writer disconnected")?;
  Ok(())
}

fn select_device(
  enumerator: &DeviceEnumerator,
  direction: Direction,
  selector: Option<&str>,
) -> Result<Device> {
  let Some(selector) = selector else {
    return enumerator
      .get_default_device(&direction)
      .with_context(|| format!("failed to get default {direction:?} device"));
  };

  let normalized = selector.to_lowercase();
  let collection = enumerator
    .get_device_collection(&direction)
    .with_context(|| format!("failed to enumerate {direction:?} devices"))?;
  let mut exact = Vec::new();
  let mut partial = Vec::new();

  for item in &collection {
    let device = item.context("failed to access an enumerated audio device")?;
    let id = device.get_id().context("failed to read device ID")?;
    let name = device
      .get_friendlyname()
      .context("failed to read device friendly name")?;
    if id.eq_ignore_ascii_case(selector) || name.eq_ignore_ascii_case(selector) {
      exact.push((device, name));
    } else if id.to_lowercase().contains(&normalized) || name.to_lowercase().contains(&normalized) {
      partial.push((device, name));
    }
  }

  let mut matches = if exact.is_empty() { partial } else { exact };
  match matches.len() {
    0 => bail!("no {direction:?} device matches {selector:?}"),
    1 => Ok(matches.remove(0).0),
    _ => {
      let names = matches
        .into_iter()
        .map(|(_, name)| name)
        .collect::<Vec<_>>()
        .join(", ");
      bail!("{direction:?} selector {selector:?} is ambiguous: {names}")
    }
  }
}

fn create_track_writer(
  run_dir: &Path,
  track: Track,
  device: DeviceDescriptor,
) -> Result<TrackWriter> {
  let output_path = run_dir.join(track.filename());
  let writer = WavWriter::create(
    &output_path,
    WavSpec {
      channels: track.channels(),
      sample_rate: SAMPLE_RATE,
      bits_per_sample: 32,
      sample_format: SampleFormat::Float,
    },
  )
  .with_context(|| format!("failed to create {}", output_path.display()))?;

  Ok(TrackWriter {
    writer,
    manifest: TrackManifest {
      device,
      output_file: track.filename().to_owned(),
      requested_sample_rate: SAMPLE_RATE,
      requested_channels: track.channels(),
      stats: TrackStats::default(),
    },
  })
}

fn write_packet(
  track_writer: &mut TrackWriter,
  frames: u32,
  bytes: &[u8],
  metadata: &BufferMetadata,
) -> Result<()> {
  let expected_samples = frames as usize * track_writer.manifest.requested_channels as usize;
  if bytes.len() != expected_samples * size_of::<f32>() {
    bail!(
      "capture packet contains {} bytes, expected {}",
      bytes.len(),
      expected_samples * size_of::<f32>()
    );
  }

  if metadata.silent {
    for _ in 0..expected_samples {
      track_writer.writer.write_sample(0.0_f32)?;
    }
  } else {
    let (samples, remainder) = bytes.as_chunks::<{ size_of::<f32>() }>();
    debug_assert!(remainder.is_empty());
    for bytes in samples {
      let sample = f32::from_le_bytes(*bytes);
      track_writer.writer.write_sample(sample)?;
    }
  }

  let stats = &mut track_writer.manifest.stats;
  stats.packets += 1;
  stats.frames += u64::from(frames);
  stats.silent_packets += u64::from(metadata.silent);
  stats.discontinuities += u64::from(metadata.data_discontinuity);
  stats.timestamp_errors += u64::from(metadata.timestamp_error);
  stats
    .first_device_position
    .get_or_insert(metadata.device_position);
  stats.last_device_position = Some(metadata.device_position);
  stats
    .first_qpc_timestamp_100ns
    .get_or_insert(metadata.qpc_timestamp_100ns);
  stats.last_qpc_timestamp_100ns = Some(metadata.qpc_timestamp_100ns);
  Ok(())
}

fn write_event(writer: &mut BufWriter<File>, event: &EventRecord<'_>) -> Result<()> {
  serde_json::to_writer(&mut *writer, event)?;
  writer.write_all(b"\n")?;
  Ok(())
}

fn finalize_writers(
  writers: BTreeMap<Track, TrackWriter>,
) -> Result<BTreeMap<Track, TrackManifest>> {
  writers
    .into_iter()
    .map(|(track, writer)| {
      writer
        .writer
        .finalize()
        .with_context(|| format!("failed to finalize {}", track.filename()))?;
      Ok((track, writer.manifest))
    })
    .collect()
}

fn unix_time_ms() -> Result<u128> {
  Ok(
    SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .context("system clock is before the Unix epoch")?
      .as_millis(),
  )
}
