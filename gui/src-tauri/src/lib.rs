mod commands;
mod state;
mod watch;

use state::AppState;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};
use tauri_plugin_window_state::{AppHandleExt as _, StateFlags};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Defaults to "info" (rather than requiring RUST_LOG) so the QA-A
    // load-time instrumentation (see commands.rs) is visible in `cargo
    // tauri dev`'s terminal output without extra setup.
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .init();

    // Startup generational backup of the proxy-local store (expression §1):
    // the store is the only place a card's *meaning* (時報/習慣・儀式グループ・
    // 目的・環境) lives, and losing it is silent -- cards themselves survive
    // in CloudKit. Documents is a folder users naturally back up.
    if let Ok(dir) = reminder_core::session_store::data_dir() {
        match reminder_core::proxy_store::backup_to_documents(&dir) {
            Ok(Some(path)) => tracing::info!(path = %path.display(), "proxy store backed up"),
            Ok(None) => {}
            Err(e) => tracing::warn!(error = %e, "proxy store backup failed"),
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // Persists/restores window position, size, and maximized state
        // across restarts (QA-F) -- without this, the window always opens
        // at the fixed 800x600 default from tauri.conf.json regardless of
        // how the user last left it.
        .plugin(tauri_plugin_window_state::Builder::default().build())
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
            commands::get_proxy_store,
            commands::set_proxy_meta,
            commands::set_env_keys,
            commands::set_list_excluded,
        ])
        .setup(|app| {
            let quit_item = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit_item])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().unwrap())
                .menu(&menu)
                .on_menu_event(|app, event| {
                    if event.id() == "quit" {
                        // The window-state plugin only persists on a clean
                        // exit; our own hide-to-tray CloseRequested handler
                        // means the window is never actually closed until
                        // this explicit quit path, so save here rather than
                        // trusting an exit hook to fire in time.
                        let _ = app.save_window_state(StateFlags::all());
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

            // NOTE: previously this called window.show()/set_focus() here
            // explicitly, because adding a tray icon made the main window
            // start hidden by default. Since adding tauri-plugin-window-state
            // (which restores position/size *and* shows the window itself
            // as part of that restore -- see its docs), calling show() here
            // too raced with its restore and left the window positioned at
            // whatever transient/staging spot it was created at instead of
            // the correct restored position (observed empirically: window
            // ended up off-screen at (-25600, -25600) while the persisted
            // state file itself had the correct, valid coordinates).
            // window-state's restore-then-show now handles this instead.
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window would otherwise drop the background poller
            // along with it; hide instead so due-reminder notifications
            // keep firing until the user explicitly quits from the tray.
            // (Verified separately -- handan/0028 -- that window-state's
            // save-on-exit mechanism itself works correctly when the window
            // is allowed to close normally; our explicit
            // save_window_state() call in the tray quit handler covers the
            // hide-to-tray path where that natural exit never happens.)
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
