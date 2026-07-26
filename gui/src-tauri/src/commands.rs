//! Tauri commands for the login flow (GUI-3). Deciding *when* to show a
//! password/2FA prompt is the frontend's job (a Sheet Modal); these commands
//! only do the network/state work that `reminder_core::bootstrap`/`auth`
//! already expose, mirroring the CLI's `ensure_login` but without any
//! blocking stdin prompts.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use reminder_core::reminders::{Reminder, RemindersList};
use reminder_core::{auth, bootstrap, reminders, session_store};
use serde::Serialize;
use tauri::State;

use crate::state::{AppState, AuthState};

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LoginResult {
    Complete,
    TwoFactorRequired,
}

/// Read back a previously persisted Apple ID, if any, so the frontend can
/// attempt a silent `try_resume` on startup without asking the user to type
/// their email in again every launch (the CLI sidesteps this by requiring
/// `--apple-id` on every invocation; a GUI has no such argument).
#[tauri::command]
pub fn get_persisted_apple_id() -> Option<String> {
    let dir = session_store::data_dir().ok()?;
    session_store::load_auth_state(&dir).apple_id
}

/// Attempt to resume a previously persisted session with no user
/// interaction. `false` means there's nothing to resume, or Apple rejected
/// it -- the frontend should fall back to showing the login Sheet.
#[tauri::command]
pub async fn try_resume(apple_id: String, state: State<'_, AppState>) -> Result<bool, String> {
    let resumed = bootstrap::try_resume_session(&apple_id)
        .await
        .map_err(|e| e.to_string())?;

    let Some((http, client_id, account_data)) = resumed else {
        return Ok(false);
    };

    make_ready(&state, http, &client_id, &account_data)
        .await
        .map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
pub async fn login(
    apple_id: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<LoginResult, String> {
    let mut client = auth::AppleAuthClient::new(&apple_id).map_err(|e| e.to_string())?;
    let outcome = client.login(&password).await.map_err(|e| e.to_string())?;

    match outcome {
        auth::LoginOutcome::Complete(data) => {
            let http = client.http_client();
            let client_id = client.client_id().to_string();
            persist_and_store_password(&client, &apple_id, &password).map_err(|e| e.to_string())?;
            make_ready(&state, http, &client_id, &data)
                .await
                .map_err(|e| e.to_string())?;
            Ok(LoginResult::Complete)
        }
        auth::LoginOutcome::TwoFactorRequired => {
            let mut guard = state.auth.lock().await;
            *guard = AuthState::AwaitingTwoFactor {
                client: Box::new(client),
                password,
            };
            Ok(LoginResult::TwoFactorRequired)
        }
    }
}

#[tauri::command]
pub async fn submit_two_factor_code(
    code: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let (mut client, password) = {
        let mut guard = state.auth.lock().await;
        match std::mem::replace(&mut *guard, AuthState::LoggedOut) {
            AuthState::AwaitingTwoFactor { client, password } => (client, password),
            other => {
                *guard = other;
                return Err("2FAコードの入力は待機していません".to_string());
            }
        }
    };

    client
        .validate_trusted_device_code(&code)
        .await
        .map_err(|e| e.to_string())?;
    let data = client.trust_session().await.map_err(|e| e.to_string())?;

    let apple_id = client.persisted_state().apple_id.unwrap_or_default();
    let http = client.http_client();
    let client_id = client.client_id().to_string();
    persist_and_store_password(&client, &apple_id, &password).map_err(|e| e.to_string())?;
    make_ready(&state, http, &client_id, &data)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Every list/CRUD command needs the same "give me the active
/// `RemindersService` or a clear error" step; the lock is dropped before
/// returning so the (potentially slow) network call afterward doesn't hold
/// it -- `Arc::clone` is cheap and `RemindersService` needs no lock itself
/// (every method is `&self`).
async fn reminders_service(state: &State<'_, AppState>) -> Result<Arc<reminders::RemindersService>, String> {
    let guard = state.auth.lock().await;
    guard
        .reminders()
        .cloned()
        .ok_or_else(|| "ログインしていません".to_string())
}

#[tauri::command]
pub async fn list_lists(state: State<'_, AppState>) -> Result<Vec<RemindersList>, String> {
    let reminders = reminders_service(&state).await?;
    reminders.lists().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_reminders(
    list_id: String,
    include_completed: bool,
    state: State<'_, AppState>,
) -> Result<Vec<Reminder>, String> {
    let reminders = reminders_service(&state).await?;
    reminders
        .list_reminders(&list_id, include_completed)
        .await
        .map_err(|e| e.to_string())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn create_reminder(
    list_id: String,
    title: String,
    notes: String,
    priority: i64,
    flagged: bool,
    due_date: Option<DateTime<Utc>>,
    state: State<'_, AppState>,
) -> Result<Reminder, String> {
    let reminders = reminders_service(&state).await?;
    reminders
        .create(&list_id, &title, &notes, priority, flagged, due_date)
        .await
        .map_err(|e| e.to_string())
}

/// A single unified path for every in-place edit (completed/title/notes/
/// priority/flagged/due date/list move), matching `RemindersService::update`
/// -- the frontend sends back the full `Reminder` it last fetched (carrying
/// `recordChangeTag` for CloudKit's optimistic-concurrency check) with
/// whichever fields it changed.
#[tauri::command]
pub async fn update_reminder(reminder: Reminder, state: State<'_, AppState>) -> Result<Reminder, String> {
    let reminders = reminders_service(&state).await?;
    reminders.update(&reminder).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_reminder(reminder: Reminder, state: State<'_, AppState>) -> Result<(), String> {
    let reminders = reminders_service(&state).await?;
    reminders.delete(&reminder).await.map_err(|e| e.to_string())
}

async fn make_ready(
    state: &State<'_, AppState>,
    http: reqwest::Client,
    client_id: &str,
    account_data: &serde_json::Value,
) -> anyhow::Result<()> {
    let service_root = bootstrap::reminders_service_root(account_data)?;
    let service = reminders::RemindersService::new(http, &service_root, client_id);
    let mut guard = state.auth.lock().await;
    *guard = AuthState::Ready {
        reminders: Arc::new(service),
    };
    Ok(())
}

/// Best-effort, matching the CLI's `ensure_login`: stashing the password in
/// Windows Credential Manager lets a fully-expired session (both the
/// session token AND the trust token invalidated) re-login without a
/// password prompt, since only the persisted session/trust tokens go stale
/// on their own -- the account password does not.
fn persist_and_store_password(
    client: &auth::AppleAuthClient,
    apple_id: &str,
    password: &str,
) -> anyhow::Result<()> {
    let dir = session_store::data_dir()?;
    bootstrap::persist_state(client, &dir)?;

    if let Ok(entry) = keyring::Entry::new(bootstrap::KEYRING_SERVICE, apple_id) {
        let _ = entry.set_password(password);
    }
    Ok(())
}
