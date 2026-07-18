use std::path::PathBuf;
use std::time::Duration;

pub struct CaptureConfig {
  pub duration: Duration,
  pub output_root: PathBuf,
  pub microphone_selector: Option<String>,
  pub render_selector: Option<String>,
}

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{capture, list_devices};

#[cfg(not(windows))]
pub fn list_devices(_json: bool) -> anyhow::Result<()> {
  anyhow::bail!("mini-aec-lab currently supports Windows only")
}

#[cfg(not(windows))]
pub fn capture(_config: CaptureConfig) -> anyhow::Result<()> {
  anyhow::bail!("mini-aec-lab currently supports Windows only")
}
