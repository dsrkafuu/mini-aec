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
    }),
  }
}
