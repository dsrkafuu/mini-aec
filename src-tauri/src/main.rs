#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use audio::{TrayConfiguration, TrayEngine, MICROPHONE_ENV, RENDER_ENV};
use mini_aec_engine::{EngineSnapshot, EngineState, ProcessingMode};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;

const QUIT_MENU_ID: &str = "quit";
const AEC_MENU_ID: &str = "aec-enabled";
const RESTART_MENU_ID: &str = "restart-engine";

#[allow(
  clippy::too_many_lines,
  reason = "the windowless tray setup keeps menu actions, polling and orderly shutdown in one host boundary"
)]
fn main() {
  tauri::Builder::default()
    .setup(|app| {
      let configuration = TrayConfiguration::from_environment();
      let controller = configuration.clone().map(TrayEngine::new).map(Arc::new);
      let configured = controller.is_some();
      let status = MenuItem::with_id(
        app,
        "status",
        if configured {
          "Status: stopped (development configuration loaded)"
        } else {
          "Status: not configured"
        },
        false,
        None::<&str>,
      )?;
      let aec_enabled = CheckMenuItem::with_id(
        app,
        AEC_MENU_ID,
        "Enable AEC",
        configured,
        true,
        None::<&str>,
      )?;
      let microphone = MenuItem::with_id(
        app,
        "microphone",
        configuration.as_ref().map_or_else(
          || format!("Microphone: set {MICROPHONE_ENV}"),
          |config| format!("Microphone ID: {}", config.microphone_endpoint_id),
        ),
        false,
        None::<&str>,
      )?;
      let render = MenuItem::with_id(
        app,
        "render",
        configuration.as_ref().map_or_else(
          || format!("Render device: set {RENDER_ENV}"),
          |config| format!("Render ID: {}", config.render_endpoint_id),
        ),
        false,
        None::<&str>,
      )?;
      let restart = MenuItem::with_id(
        app,
        "restart-engine",
        "Restart audio engine",
        configured,
        None::<&str>,
      )?;
      let start_with_windows = CheckMenuItem::with_id(
        app,
        "start-with-windows",
        "Start with Windows",
        false,
        false,
        None::<&str>,
      )?;
      let open_logs = MenuItem::with_id(app, "open-logs", "Open logs", false, None::<&str>)?;
      let separator = PredefinedMenuItem::separator(app)?;
      let quit = MenuItem::with_id(app, QUIT_MENU_ID, "Quit MiniAEC", true, None::<&str>)?;
      let menu = Menu::with_items(
        app,
        &[
          &status,
          &aec_enabled,
          &microphone,
          &render,
          &restart,
          &start_with_windows,
          &open_logs,
          &separator,
          &quit,
        ],
      )?;

      let mut tray = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("MiniAEC");
      if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
      }
      let menu_controller = controller.clone();
      let menu_status = status.clone();
      let menu_aec = aec_enabled.clone();
      tray
        .on_menu_event(move |app, event| match event.id().as_ref() {
          QUIT_MENU_ID => {
            if let Some(controller) = &menu_controller {
              let _ = controller.stop();
            }
            app.exit(0);
          }
          AEC_MENU_ID => {
            if let Some(controller) = &menu_controller {
              let enabled = menu_aec.is_checked().unwrap_or(true);
              let mode = if enabled {
                ProcessingMode::Aec
              } else {
                ProcessingMode::Bypass
              };
              if let Err(error) = controller.select_and_start(mode) {
                let _ = menu_status.set_text(format!("Status: failed - {error}"));
              } else {
                let _ = menu_status.set_text(status_text(&controller.snapshot()));
              }
            }
          }
          RESTART_MENU_ID => {
            if let Some(controller) = &menu_controller {
              if let Err(error) = controller.restart() {
                let _ = menu_status.set_text(format!("Status: failed - {error}"));
              } else {
                let _ = menu_status.set_text(status_text(&controller.snapshot()));
              }
            }
          }
          _ => {}
        })
        .build(app)?;

      if let Some(controller) = controller {
        let polling_status = status.clone();
        thread::Builder::new()
          .name("mini-aec-tray-status".to_owned())
          .spawn(move || loop {
            thread::sleep(Duration::from_secs(1));
            let _ = polling_status.set_text(status_text(&controller.snapshot()));
          })?;
      }
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running MiniAEC");
}

fn status_text(snapshot: &EngineSnapshot) -> String {
  let state = match snapshot.state {
    EngineState::Stopped => "stopped",
    EngineState::Starting => "starting",
    EngineState::RunningBypass => "running bypass",
    EngineState::RunningAec => "running AEC",
    EngineState::Degraded => "AEC degraded",
    EngineState::Stopping => "stopping",
    EngineState::Failed => "failed",
  };
  snapshot.last_error.as_ref().map_or_else(
    || format!("Status: {state}"),
    |error| format!("Status: {state} - {error}"),
  )
}

#[cfg(test)]
mod tests {
  use mini_aec_engine::{DegradationReason, EngineSnapshot, EngineState, ProcessingMode};

  use super::status_text;

  #[test]
  fn tray_distinguishes_aec_degraded_and_explicit_bypass() {
    assert_eq!(
      status_text(&EngineSnapshot {
        state: EngineState::RunningAec,
        mode: Some(ProcessingMode::Aec),
        ..EngineSnapshot::default()
      }),
      "Status: running AEC"
    );
    assert_eq!(
      status_text(&EngineSnapshot {
        state: EngineState::Degraded,
        mode: Some(ProcessingMode::Aec),
        degradation_reason: Some(DegradationReason::RenderReferenceMissing),
        ..EngineSnapshot::default()
      }),
      "Status: AEC degraded"
    );
    assert_eq!(
      status_text(&EngineSnapshot {
        state: EngineState::RunningBypass,
        mode: Some(ProcessingMode::Bypass),
        ..EngineSnapshot::default()
      }),
      "Status: running bypass"
    );
  }
}
