mod platform;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "denoise-lab")]
#[command(about = "Open Denoise audio capture and processing diagnostics")]
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
  }
}
