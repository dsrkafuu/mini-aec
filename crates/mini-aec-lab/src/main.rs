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
  /// Bypass one explicit physical microphone into `MiniAEC Microphone` in real time.
  Bypass(BypassArgs),
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

#[derive(Debug, Args)]
struct BypassArgs {
  /// Exact physical capture endpoint ID. Friendly names and defaults are not accepted.
  #[arg(long)]
  microphone_id: String,

  /// Run duration in seconds.
  #[arg(long)]
  duration: u64,

  /// Parent directory for metadata-only validation runs.
  #[arg(long, default_value = "driver/windows/out/validation/engine")]
  output: PathBuf,
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
    Command::Bypass(args) => platform::bypass(platform::BypassConfig {
      duration: Duration::from_secs(args.duration),
      output_root: args.output,
      microphone_endpoint_id: args.microphone_id,
    }),
  }
}

#[cfg(test)]
mod tests {
  use clap::Parser;

  use super::{Cli, Command};

  #[test]
  fn existing_command_names_and_core_arguments_remain_available() {
    assert!(matches!(
      Cli::try_parse_from(["mini-aec-lab", "devices", "--json"])
        .expect("devices arguments parse")
        .command,
      Command::Devices { json: true }
    ));
    assert!(matches!(
      Cli::try_parse_from([
        "mini-aec-lab",
        "capture",
        "--duration",
        "1",
        "--microphone",
        "mic",
        "--render",
        "speaker"
      ])
      .expect("capture arguments parse")
      .command,
      Command::Capture(_)
    ));
    assert!(matches!(
      Cli::try_parse_from(["mini-aec-lab", "aec", "--run", "artifacts/runs/example"])
        .expect("aec arguments parse")
        .command,
      Command::Aec(_)
    ));
  }

  #[test]
  fn bypass_requires_an_exact_id_and_duration() {
    assert!(Cli::try_parse_from(["mini-aec-lab", "bypass"]).is_err());
    assert!(
      Cli::try_parse_from(["mini-aec-lab", "bypass", "--microphone-id", "physical-id"]).is_err()
    );
    assert!(matches!(
      Cli::try_parse_from([
        "mini-aec-lab",
        "bypass",
        "--microphone-id",
        "physical-id",
        "--duration",
        "300"
      ])
      .expect("complete bypass arguments parse")
      .command,
      Command::Bypass(_)
    ));
  }
}
