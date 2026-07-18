#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;

const QUIT_MENU_ID: &str = "quit";

fn main() {
  tauri::Builder::default()
    .setup(|app| {
      let status = MenuItem::with_id(
        app,
        "status",
        "Status: audio engine not connected",
        false,
        None::<&str>,
      )?;
      let aec_enabled =
        CheckMenuItem::with_id(app, "aec-enabled", "Enable AEC", false, true, None::<&str>)?;
      let microphone = MenuItem::with_id(
        app,
        "microphone",
        "Microphone: not configured",
        false,
        None::<&str>,
      )?;
      let render = MenuItem::with_id(
        app,
        "render",
        "Render device: not configured",
        false,
        None::<&str>,
      )?;
      let restart = MenuItem::with_id(
        app,
        "restart-engine",
        "Restart audio engine",
        false,
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
      tray
        .on_menu_event(|app, event| {
          if event.id().as_ref() == QUIT_MENU_ID {
            app.exit(0);
          }
        })
        .build(app)?;

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running MiniAEC");
}
