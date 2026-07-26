mod commands;
mod state;
mod watch;

use state::AppState;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Defaults to "info" (rather than requiring RUST_LOG) so the QA-A
    // load-time instrumentation (see commands.rs) is visible in `cargo
    // tauri dev`'s terminal output without extra setup.
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_persisted_apple_id,
            commands::try_resume,
            commands::login,
            commands::submit_two_factor_code,
            commands::list_lists,
            commands::list_reminders,
            commands::create_reminder,
            commands::update_reminder,
            commands::delete_reminder,
            commands::reorder_list,
        ])
        .setup(|app| {
            let quit_item = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit_item])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().unwrap())
                .menu(&menu)
                .on_menu_event(|app, event| {
                    if event.id() == "quit" {
                        app.exit(0);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    // Native Reminders/most tray apps show the window on a
                    // plain left-click, reserving the menu for right-click
                    // (the right-click-opens-menu behavior is automatic).
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Adding a tray icon appears to make the main window start
            // hidden by default on Windows (observed: the window exists
            // with the configured size but `IsWindowVisible` is false until
            // shown) -- show it explicitly rather than relying on
            // whatever default behavior a tray-resident app gets.
            if let Some(window) = app.get_webview_window("main") {
                window.show()?;
                window.set_focus()?;
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window would otherwise drop the background poller
            // along with it; hide instead so due-reminder notifications
            // keep firing until the user explicitly quits from the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
