mod commands;
mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
