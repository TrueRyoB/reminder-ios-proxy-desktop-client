//! Prompt-free orchestration shared by every consumer (CLI, GUI): resolving
//! the CloudKit service root, persisting session state, and attempting a
//! silent session resume. Deciding *when* to prompt for a password or a 2FA
//! code is inherently interactive and stays in each consumer (terminal
//! prompts for the CLI, a Sheet dialog for the GUI).

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::auth::AppleAuthClient;
use crate::session_store;

pub const KEYRING_SERVICE: &str = "reminder-proxy-client";

/// pyicloud's RemindersService uses the shared CloudKit database webservice
/// ("ckdatabasews"), not a "reminders"-named entry -- that key (if present
/// at all) points at the legacy CalDAV-compat backend ("caldavj"), which
/// doesn't speak the CloudKit JSON protocol at all.
pub fn reminders_service_root(account_data: &Value) -> Result<String> {
    account_data
        .get("webservices")
        .and_then(|v| v.get("ckdatabasews"))
        .and_then(|v| v.get("url"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("no ckdatabasews webservice URL in accountLogin response"))
}

pub fn persist_state(client: &AppleAuthClient, dir: &std::path::Path) -> Result<()> {
    let cookies = client.snapshot_cookie_store()?;
    session_store::save_cookie_store(dir, &cookies)?;
    session_store::save_auth_state(dir, &client.persisted_state())?;
    Ok(())
}

/// Attempt to resume a previously persisted session with no user
/// interaction (mirrors pyicloud's `_authenticate_with_token()`).
///
/// `Ok(None)` means there is nothing to resume from, or Apple rejected the
/// persisted tokens (expired/invalidated) -- the caller should fall back to
/// an interactive `AppleAuthClient::new(apple_id)` + `login()` flow in that
/// case, exactly as it would on a first run. This is not an error condition.
pub async fn try_resume_session(
    apple_id: &str,
) -> Result<Option<(reqwest::Client, String, Value)>> {
    let dir = session_store::data_dir()?;
    let cookie_jar = session_store::load_cookie_store(&dir);
    let persisted = session_store::load_auth_state(&dir);

    if persisted.session_token.is_none() {
        return Ok(None);
    }

    let mut client = AppleAuthClient::with_state(apple_id, cookie_jar, &persisted)?;
    match client.try_resume().await {
        Ok(data) => {
            persist_state(&client, &dir)?;
            Ok(Some((client.http_client(), client.client_id().to_string(), data)))
        }
        Err(_) => Ok(None),
    }
}
