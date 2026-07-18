mod offline;
mod platform;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "mini-aec-lab")]
#[command(about = "MiniAEC audio capture and processing diagnostics")]
struct Cli {
  #[command(subcommand)]
  command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
  /// List Windows audio endpoints and their shared-mode formats.
  Devices {
    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
  },
  /// Capture a physical microphone and render-loopback reference together.
  Capture(CaptureArgs),
  /// Align a diagnostic run by QPC timestamp and process it through WebRTC AEC3.
  Aec(AecArgs),
  /// Build a randomized A/B/C listening set from several AEC3 profiles.
  BlindAec(BlindAecArgs),
}

#[derive(Debug, Args)]
struct CaptureArgs {
  /// Capture duration in seconds.
  #[arg(long, default_value_t = 10)]
  duration: u64,

  /// Parent directory for timestamped run artifacts.
  #[arg(long, default_value = "artifacts/runs")]
  output: PathBuf,

  /// Capture endpoint ID or an unambiguous part of its friendly name.
  #[arg(long)]
  microphone: Option<String>,

  /// Render endpoint ID or an unambiguous part of its friendly name.
  #[arg(long)]
  render: Option<String>,
}

#[derive(Debug, Args)]
struct AecArgs {
  /// Diagnostic run directory containing manifest.json and both source WAV files.
  #[arg(long)]
  run: PathBuf,

  /// Optional fixed acoustic stream-delay hint. Omit to use AEC3 delay estimation.
  #[arg(long)]
  stream_delay_ms: Option<u16>,

  /// Render level above which frames count toward active echo-reduction metrics.
  #[arg(long, default_value_t = -50.0)]
  active_threshold_dbfs: f64,

  /// Export WebRTC's 16 kHz linear AEC signal for mechanism-isolation diagnostics.
  #[arg(long)]
  export_linear: bool,

  /// AEC3 tuning profile. The default profile is the frozen baseline.
  #[arg(long, value_enum, default_value_t)]
  profile: offline::AecProfile,
}

#[derive(Debug, Args)]
struct BlindAecArgs {
  /// Diagnostic run directory containing manifest.json and both source WAV files.
  #[arg(long)]
  run: PathBuf,

  /// Listening interval in seconds, for example 7-13. Repeat for multiple intervals.
  #[arg(long, required = true)]
  segment: Vec<String>,

  /// AEC profile to include. Repeat exactly three times; omit for the original profile set.
  #[arg(long, value_enum)]
  profile: Vec<offline::AecProfile>,

  /// Render level above which frames count toward active echo-reduction metrics.
  #[arg(long, default_value_t = -50.0)]
  active_threshold_dbfs: f64,
}

fn main() -> Result<()> {
  let cli = Cli::parse();

  match cli.command {
    Command::Devices { json } => platform::list_devices(json),
    Command::Capture(args) => platform::capture(platform::CaptureConfig {
      duration: Duration::from_secs(args.duration),
      output_root: args.output,
      microphone_selector: args.microphone,
      render_selector: args.render,
    }),
    Command::Aec(args) => offline::process(&offline::AecConfig {
      run_dir: args.run,
      stream_delay_ms: args.stream_delay_ms,
      active_threshold_dbfs: args.active_threshold_dbfs,
      profile: args.profile,
      export_linear: args.export_linear,
    }),
    Command::BlindAec(args) => offline::build_blind_experiment(&offline::BlindAecConfig {
      run_dir: args.run,
      segments: args.segment,
      profiles: args.profile,
      active_threshold_dbfs: args.active_threshold_dbfs,
    }),
  }
}
