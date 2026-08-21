use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use mini_aec_release::{preflight_package, LifecycleInventory, ReleaseManifest};

#[derive(Debug, Parser)]
#[command(
  name = "mini-aec-release",
  about = "Validate MiniAEC production release artifacts"
)]
struct Cli {
  #[command(subcommand)]
  command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
  /// Validate a production package directory without changing Windows state.
  Preflight {
    #[arg(long)]
    package: PathBuf,
  },
  /// Validate one release manifest without checking package files.
  ValidateManifest {
    #[arg(long)]
    manifest: PathBuf,
  },
  /// Check whether a candidate manifest accepts an installed manifest.
  CheckCompatibility {
    #[arg(long)]
    candidate: PathBuf,
    #[arg(long)]
    installed: PathBuf,
  },
  /// Compare two metadata-only inventories and fail when they differ.
  CompareInventory {
    #[arg(long)]
    expected: PathBuf,
    #[arg(long)]
    observed: PathBuf,
  },
}

fn main() -> Result<()> {
  match Cli::parse().command {
    Command::Preflight { package } => {
      let result = preflight_package(&package).with_context(|| {
        format!(
          "production package preflight failed for {}",
          package.display()
        )
      })?;
      println!("{}", serde_json::to_string_pretty(&result)?);
    }
    Command::ValidateManifest { manifest } => {
      let value = ReleaseManifest::from_path(&manifest)
        .with_context(|| format!("failed to read {}", manifest.display()))?;
      value
        .validate()
        .with_context(|| format!("manifest validation failed for {}", manifest.display()))?;
      println!("validated {}", manifest.display());
    }
    Command::CheckCompatibility {
      candidate,
      installed,
    } => {
      let candidate = read_manifest(&candidate)?;
      let installed = read_manifest(&installed)?;
      candidate
        .is_compatible_with(&installed)
        .context("candidate is incompatible with the installed release")?;
      println!("candidate is compatible with the installed release");
    }
    Command::CompareInventory { expected, observed } => {
      let expected = read_inventory(&expected)?;
      let observed = read_inventory(&observed)?;
      let comparison = observed.compare_to(&expected);
      println!("{}", serde_json::to_string_pretty(&comparison)?);
      if !comparison.is_clean() {
        anyhow::bail!(
          "inventory comparison found {} difference(s)",
          comparison.differences.len()
        );
      }
    }
  }
  Ok(())
}

fn read_manifest(path: &Path) -> Result<ReleaseManifest> {
  let manifest = ReleaseManifest::from_path(path)
    .with_context(|| format!("failed to read manifest {}", path.display()))?;
  manifest
    .validate()
    .with_context(|| format!("manifest validation failed for {}", path.display()))?;
  Ok(manifest)
}

fn read_inventory(path: &Path) -> Result<LifecycleInventory> {
  let json = std::fs::read_to_string(path)
    .with_context(|| format!("failed to read inventory {}", path.display()))?;
  serde_json::from_str(&json)
    .with_context(|| format!("invalid inventory JSON in {}", path.display()))
}
