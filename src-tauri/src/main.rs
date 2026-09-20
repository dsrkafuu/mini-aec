#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use audio::{ConfigurationSource, TrayEngine};
use config::ConfigStore;
use mini_aec_engine::{EngineSnapshot, EngineState};
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{image::Image, Manager, Runtime};

const STATUS_MENU_ID: &str = "status";
const ENABLE_AEC_MENU_ID: &str = "enable-aec3";
const INPUT_MICROPHONE_SUBMENU_ID: &str = "input-microphone";
const OUTPUT_REFERENCE_SUBMENU_ID: &str = "output-reference";
const CABLE_PAIRS_SUBMENU_ID: &str = "cable-pairs";
const DEFAULT_MICROPHONE_MENU_ID: &str = "select-microphone-default";
const DEFAULT_RENDER_MENU_ID: &str = "select-render-default";
const SELECT_MICROPHONE_PREFIX: &str = "select-microphone:";
const SELECT_RENDER_PREFIX: &str = "select-render:";
const SELECT_CABLE_PAIR_PREFIX: &str = "select-cable-pair:";
const QUIT_MENU_ID: &str = "quit";

#[allow(
  clippy::too_many_lines,
  reason = "the windowless tray setup keeps menu actions, polling and orderly shutdown in one host boundary"
)]
fn main() {
  tauri::Builder::default()
    .setup(|app| {
      let config_path = app.path().app_config_dir()?.join("config.json");
      let controller = Arc::new(TrayEngine::new(ConfigStore::new(config_path)));
      let _ = controller.refresh_inventory();
      let _ = controller.apply_current();

      let status = MenuItem::with_id(
        app,
        STATUS_MENU_ID,
        controller_status_text(&controller),
        false,
        None::<&str>,
      )?;
      let enable_aec = CheckMenuItem::with_id(
        app,
        ENABLE_AEC_MENU_ID,
        "Enable AEC3",
        true,
        controller.aec_enabled(),
        None::<&str>,
      )?;
      let input_microphone =
        Submenu::with_id(app, INPUT_MICROPHONE_SUBMENU_ID, "Input Microphone", true)?;
      let output_reference =
        Submenu::with_id(app, OUTPUT_REFERENCE_SUBMENU_ID, "Output Reference", true)?;
      let cable_pairs = Submenu::with_id(app, CABLE_PAIRS_SUBMENU_ID, "VB-CABLE Pairs", true)?;
      let separator_1 = PredefinedMenuItem::separator(app)?;
      let separator_2 = PredefinedMenuItem::separator(app)?;
      let quit = MenuItem::with_id(app, QUIT_MENU_ID, "Quit MiniAEC", true, None::<&str>)?;
      let menu = Menu::with_items(
        app,
        &[
          &status,
          &enable_aec,
          &separator_1,
          &input_microphone,
          &output_reference,
          &cable_pairs,
          &separator_2,
          &quit,
        ],
      )?;

      rebuild_device_submenus(
        &controller,
        &input_microphone,
        &output_reference,
        &cable_pairs,
      )?;
      update_menu_state(
        &controller,
        &status,
        &enable_aec,
        &input_microphone,
        &output_reference,
        &cable_pairs,
      );

      let tray_icon = Image::from_bytes(include_bytes!("../icons/tray-icon.png"))?;
      let tray = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .icon(tray_icon)
        .tooltip("MiniAEC");

      let menu_controller = controller.clone();
      let menu_status = status.clone();
      let menu_enable_aec = enable_aec.clone();
      let menu_input_microphone = input_microphone.clone();
      let menu_output_reference = output_reference.clone();
      let menu_cable_pairs = cable_pairs.clone();
      tray
        .on_menu_event(move |app, event| {
          let id = event.id().as_ref();
          match id {
            QUIT_MENU_ID => {
              let _ = menu_controller.stop();
              app.exit(0);
            }
            ENABLE_AEC_MENU_ID => {
              let requested = menu_enable_aec
                .is_checked()
                .unwrap_or_else(|_| menu_controller.aec_enabled());
              let _ = menu_controller.set_aec_enabled(requested);
            }
            DEFAULT_MICROPHONE_MENU_ID => {
              let _ = menu_controller.select_default_microphone();
            }
            DEFAULT_RENDER_MENU_ID => {
              let _ = menu_controller.select_default_render();
            }
            _ if id.strip_prefix(SELECT_MICROPHONE_PREFIX).is_some() => {
              if let Some(index) = id
                .strip_prefix(SELECT_MICROPHONE_PREFIX)
                .and_then(|index| index.parse().ok())
              {
                let _ = menu_controller.select_microphone(index);
              }
            }
            _ if id.strip_prefix(SELECT_RENDER_PREFIX).is_some() => {
              if let Some(index) = id
                .strip_prefix(SELECT_RENDER_PREFIX)
                .and_then(|index| index.parse().ok())
              {
                let _ = menu_controller.select_render(index);
              }
            }
            _ if id.strip_prefix(SELECT_CABLE_PAIR_PREFIX).is_some() => {
              if let Some(index) = id
                .strip_prefix(SELECT_CABLE_PAIR_PREFIX)
                .and_then(|index| index.parse().ok())
              {
                let _ = menu_controller.select_cable_pair(index);
              }
            }
            _ => {}
          }
          let _ = rebuild_device_submenus(
            &menu_controller,
            &menu_input_microphone,
            &menu_output_reference,
            &menu_cable_pairs,
          );
          update_menu_state(
            &menu_controller,
            &menu_status,
            &menu_enable_aec,
            &menu_input_microphone,
            &menu_output_reference,
            &menu_cable_pairs,
          );
        })
        .build(app)?;

      let polling_controller = controller.clone();
      let polling_status = status.clone();
      let polling_enable_aec = enable_aec.clone();
      let polling_input_microphone = input_microphone.clone();
      let polling_output_reference = output_reference.clone();
      let polling_cable_pairs = cable_pairs.clone();
      thread::Builder::new()
        .name("mini-aec-tray-status".to_owned())
        .spawn(move || loop {
          thread::sleep(Duration::from_secs(2));
          if let Ok(changed) = polling_controller.refresh_inventory() {
            if changed {
              let _ = polling_controller.apply_current();
              let _ = rebuild_device_submenus(
                &polling_controller,
                &polling_input_microphone,
                &polling_output_reference,
                &polling_cable_pairs,
              );
            }
          }
          update_menu_state(
            &polling_controller,
            &polling_status,
            &polling_enable_aec,
            &polling_input_microphone,
            &polling_output_reference,
            &polling_cable_pairs,
          );
        })?;
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running MiniAEC");
}

fn rebuild_device_submenus<R: Runtime>(
  controller: &TrayEngine,
  input_microphone: &Submenu<R>,
  output_reference: &Submenu<R>,
  cable_pairs: &Submenu<R>,
) -> tauri::Result<()> {
  let inventory = controller.inventory();
  let draft = controller.draft();
  let microphone_default_selected = controller.microphone_default_selected();
  let render_default_selected = controller.render_default_selected();
  clear_submenu(input_microphone)?;
  clear_submenu(output_reference)?;
  clear_submenu(cable_pairs)?;

  let mut microphone_items = Vec::new();
  if let Some(default) = &inventory.default_microphone {
    microphone_items.push(CheckMenuItem::with_id(
      input_microphone.app_handle(),
      DEFAULT_MICROPHONE_MENU_ID,
      format!("Default ({})", default.friendly_name),
      true,
      microphone_default_selected
        && draft.microphone_endpoint_id.as_deref() == Some(default.endpoint_id.as_str()),
      None::<&str>,
    )?);
  }
  for (index, source) in inventory.microphones.iter().enumerate() {
    microphone_items.push(CheckMenuItem::with_id(
      input_microphone.app_handle(),
      format!("{SELECT_MICROPHONE_PREFIX}{index}"),
      source.friendly_name.clone(),
      true,
      !microphone_default_selected
        && draft.microphone_endpoint_id.as_deref() == Some(source.endpoint_id.as_str()),
      None::<&str>,
    )?);
  }
  append_check_items(
    input_microphone,
    &microphone_items,
    "No active physical microphones",
  )?;

  let mut render_items = Vec::new();
  if let Some(default) = &inventory.default_render {
    render_items.push(CheckMenuItem::with_id(
      output_reference.app_handle(),
      DEFAULT_RENDER_MENU_ID,
      format!("Default ({})", default.friendly_name),
      true,
      render_default_selected
        && draft.render_endpoint_id.as_deref() == Some(default.endpoint_id.as_str()),
      None::<&str>,
    )?);
  }
  for (index, source) in inventory.renders.iter().enumerate() {
    render_items.push(CheckMenuItem::with_id(
      output_reference.app_handle(),
      format!("{SELECT_RENDER_PREFIX}{index}"),
      source.friendly_name.clone(),
      true,
      !render_default_selected
        && draft.render_endpoint_id.as_deref() == Some(source.endpoint_id.as_str()),
      None::<&str>,
    )?);
  }
  append_check_items(
    output_reference,
    &render_items,
    "No active physical render references",
  )?;

  let cable_items: Vec<_> = inventory
    .cable_pairs
    .iter()
    .enumerate()
    .map(|(index, pair)| {
      CheckMenuItem::with_id(
        cable_pairs.app_handle(),
        format!("{SELECT_CABLE_PAIR_PREFIX}{index}"),
        format!(
          "{} -> {} [{}]",
          pair.playback.friendly_name, pair.recording.friendly_name, pair.playback.device_family
        ),
        true,
        draft.cable_input_endpoint_id.as_deref() == Some(pair.playback.endpoint_id.as_str())
          && draft.cable_output_endpoint_id.as_deref() == Some(pair.recording.endpoint_id.as_str()),
        None::<&str>,
      )
    })
    .collect::<tauri::Result<_>>()?;
  append_check_items(cable_pairs, &cable_items, "No active VB-CABLE pair")?;
  Ok(())
}

fn clear_submenu<R: Runtime>(submenu: &Submenu<R>) -> tauri::Result<()> {
  while !submenu.items()?.is_empty() {
    submenu.remove_at(0)?;
  }
  Ok(())
}

fn append_check_items<R: Runtime>(
  submenu: &Submenu<R>,
  items: &[CheckMenuItem<R>],
  empty_label: &str,
) -> tauri::Result<()> {
  if items.is_empty() {
    let empty = MenuItem::new(submenu.app_handle(), empty_label, false, None::<&str>)?;
    submenu.append(&empty)?;
  } else {
    let references: Vec<&dyn IsMenuItem<R>> = items
      .iter()
      .map(|item| item as &dyn IsMenuItem<R>)
      .collect();
    submenu.append_items(&references)?;
  }
  Ok(())
}

fn update_menu_state<R: Runtime>(
  controller: &TrayEngine,
  status: &MenuItem<R>,
  enable_aec: &CheckMenuItem<R>,
  input_microphone: &Submenu<R>,
  output_reference: &Submenu<R>,
  cable_pairs: &Submenu<R>,
) {
  let inventory = controller.inventory();
  let editable = controller.configuration_source() != ConfigurationSource::Environment;
  let _ = enable_aec.set_checked(controller.aec_enabled());
  let _ = enable_aec.set_enabled(
    editable
      && !inventory.microphones.is_empty()
      && !inventory.renders.is_empty()
      && !inventory.cable_pairs.is_empty(),
  );
  let _ = input_microphone.set_enabled(editable && !inventory.microphones.is_empty());
  let _ = output_reference.set_enabled(editable && !inventory.renders.is_empty());
  let _ = cable_pairs.set_enabled(editable && !inventory.cable_pairs.is_empty());
  let _ = status.set_text(controller_status_text(controller));
}

fn controller_status_text(controller: &TrayEngine) -> String {
  let snapshot = controller.snapshot();
  match snapshot.state {
    EngineState::Stopped => format!("Status: {}", stopped_status_state(controller)),
    _ => status_text(&snapshot),
  }
}

fn stopped_status_state(controller: &TrayEngine) -> &'static str {
  let inventory = controller.inventory();
  let required_devices = !inventory.microphones.is_empty()
    && !inventory.cable_pairs.is_empty()
    && (!controller.aec_enabled() || !inventory.renders.is_empty());
  if controller.configuration_error().is_some() && required_devices {
    "ERROR"
  } else {
    "OFFLINE"
  }
}

fn lifecycle_state(snapshot: &EngineSnapshot) -> &'static str {
  match snapshot.state {
    EngineState::Starting => "STARTING",
    EngineState::Stopping => "STOPPING",
    EngineState::RunningBypass | EngineState::RunningAec => "ONLINE",
    EngineState::Degraded => "DEGRADED",
    EngineState::Failed => "ERROR",
    EngineState::Stopped => "OFFLINE",
  }
}

fn status_text(snapshot: &EngineSnapshot) -> String {
  format!("Status: {}", lifecycle_state(snapshot))
}

#[cfg(test)]
mod tests {
  use mini_aec_engine::{EngineSnapshot, EngineState};

  use super::status_text;

  #[test]
  fn tray_uses_uppercase_lifecycle_states() {
    assert_eq!(
      status_text(&EngineSnapshot {
        state: EngineState::Starting,
        ..EngineSnapshot::default()
      }),
      "Status: STARTING"
    );
    assert_eq!(
      status_text(&EngineSnapshot {
        state: EngineState::RunningBypass,
        ..EngineSnapshot::default()
      }),
      "Status: ONLINE"
    );
    assert_eq!(
      status_text(&EngineSnapshot {
        state: EngineState::Degraded,
        ..EngineSnapshot::default()
      }),
      "Status: DEGRADED"
    );
    assert_eq!(
      status_text(&EngineSnapshot {
        state: EngineState::Failed,
        ..EngineSnapshot::default()
      }),
      "Status: ERROR"
    );
    assert_eq!(status_text(&EngineSnapshot::default()), "Status: OFFLINE");
  }
}
