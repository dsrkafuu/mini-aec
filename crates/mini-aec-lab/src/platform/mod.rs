use std::path::PathBuf;
use std::time::Duration;

pub struct CaptureConfig {
  pub duration: Duration,
  pub output_root: PathBuf,
  pub microphone_selector: Option<String>,
  pub render_selector: Option<String>,
}

pub struct BypassConfig {
  pub duration: Duration,
  pub output_root: PathBuf,
  pub microphone_endpoint_id: String,
}

pub struct RealtimeAecConfig {
  pub duration: Duration,
  pub output_root: PathBuf,
  pub microphone_endpoint_id: String,
  pub render_endpoint_id: String,
}

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{bypass, capture, list_devices, realtime_aec};

#[cfg(not(windows))]
pub fn list_devices(_json: bool) -> anyhow::Result<()> {
  anyhow::bail!("mini-aec-lab currently supports Windows only")
}

#[cfg(not(windows))]
pub fn capture(_config: CaptureConfig) -> anyhow::Result<()> {
  anyhow::bail!("mini-aec-lab currently supports Windows only")
}

#[cfg(not(windows))]
pub fn bypass(_config: BypassConfig) -> anyhow::Result<()> {
  anyhow::bail!("mini-aec-lab bypass currently supports Windows only")
}

#[cfg(not(windows))]
pub fn realtime_aec(_config: RealtimeAecConfig) -> anyhow::Result<()> {
  anyhow::bail!("mini-aec-lab real-time AEC currently supports Windows only")
}
