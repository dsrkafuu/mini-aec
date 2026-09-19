use anyhow::{Context, Result};
use serde::Serialize;
use wasapi::{initialize_mta, Device, DeviceEnumerator, Direction, Role};

#[derive(Debug, Serialize)]
struct DeviceReport {
  capture: Vec<DeviceInfo>,
  render: Vec<DeviceInfo>,
}

#[derive(Debug, Serialize)]
struct DeviceInfo {
  id: String,
  name: String,
  interface_name: Option<String>,
  description: Option<String>,
  vb_audio_metadata: bool,
  state: String,
  default_roles: Vec<String>,
  format: Option<FormatInfo>,
  format_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct FormatInfo {
  sample_rate: u32,
  channels: u16,
  bits_per_sample: u16,
  valid_bits_per_sample: u16,
  block_align: u32,
  sample_type: String,
  channel_mask: u32,
}

pub fn list_devices(json: bool) -> Result<()> {
  initialize_mta()
    .ok()
    .context("failed to initialize COM in multithreaded mode")?;

  let enumerator = DeviceEnumerator::new().context("failed to create device enumerator")?;
  let report = DeviceReport {
    capture: collect_devices(&enumerator, Direction::Capture)?,
    render: collect_devices(&enumerator, Direction::Render)?,
  };

  if json {
    println!("{}", serde_json::to_string_pretty(&report)?);
  } else {
    print_group("Capture devices", &report.capture);
    print_group("Render devices", &report.render);
  }

  Ok(())
}

fn collect_devices(enumerator: &DeviceEnumerator, direction: Direction) -> Result<Vec<DeviceInfo>> {
  let default_ids = default_device_ids(enumerator, direction);
  let collection = enumerator
    .get_device_collection(&direction)
    .with_context(|| format!("failed to enumerate {direction:?} devices"))?;

  collection
    .into_iter()
    .map(|device| {
      let device = device.context("failed to access an enumerated audio device")?;
      device_info(&device, &default_ids)
    })
    .collect()
}

fn default_device_ids(enumerator: &DeviceEnumerator, direction: Direction) -> Vec<(Role, String)> {
  [Role::Console, Role::Multimedia, Role::Communications]
    .into_iter()
    .filter_map(|role| {
      enumerator
        .get_default_device_for_role(&direction, &role)
        .ok()
        .and_then(|device| device.get_id().ok())
        .map(|id| (role, id))
    })
    .collect()
}

fn device_info(device: &Device, default_ids: &[(Role, String)]) -> Result<DeviceInfo> {
  let id = device.get_id().context("failed to read device ID")?;
  let name = device
    .get_friendlyname()
    .context("failed to read device friendly name")?;
  let interface_name = device.get_interface_friendlyname().ok();
  let description = device.get_description().ok();
  let vb_audio_metadata = interface_name
    .iter()
    .chain(description.iter())
    .any(|value| value.to_ascii_lowercase().contains("vb-audio"));
  let state = device
    .get_state()
    .context("failed to read device state")?
    .to_string();
  let default_roles = default_ids
    .iter()
    .filter(|(_, default_id)| default_id == &id)
    .map(|(role, _)| format!("{role:?}"))
    .collect();

  let (format, format_error) = match device.get_device_format() {
    Ok(format) => {
      let sample_type = format.get_subformat().map_or_else(
        |error| format!("unknown ({error})"),
        |value| format!("{value:?}"),
      );
      (
        Some(FormatInfo {
          sample_rate: format.get_samplespersec(),
          channels: format.get_nchannels(),
          bits_per_sample: format.get_bitspersample(),
          valid_bits_per_sample: format.get_validbitspersample(),
          block_align: format.get_blockalign(),
          sample_type,
          channel_mask: format.get_dwchannelmask(),
        }),
        None,
      )
    }
    Err(error) => (None, Some(error.to_string())),
  };

  Ok(DeviceInfo {
    id,
    name,
    interface_name,
    description,
    vb_audio_metadata,
    state,
    default_roles,
    format,
    format_error,
  })
}

fn print_group(title: &str, devices: &[DeviceInfo]) {
  println!("{title}:");

  for device in devices {
    let defaults = if device.default_roles.is_empty() {
      String::new()
    } else {
      format!(" [default: {}]", device.default_roles.join(", "))
    };
    println!("- {}{}", device.name, defaults);
    println!("  id: {}", device.id);
    println!(
      "  interface: {}",
      device.interface_name.as_deref().unwrap_or("unavailable")
    );
    println!(
      "  description: {}",
      device.description.as_deref().unwrap_or("unavailable")
    );
    println!("  VB-Audio metadata: {}", device.vb_audio_metadata);
    println!("  state: {}", device.state);

    if let Some(format) = &device.format {
      println!(
        "  format: {} Hz, {} ch, {}-bit {}, block align {}",
        format.sample_rate,
        format.channels,
        format.valid_bits_per_sample,
        format.sample_type,
        format.block_align
      );
    } else if let Some(error) = &device.format_error {
      println!("  format: unavailable ({error})");
    }
  }
}
