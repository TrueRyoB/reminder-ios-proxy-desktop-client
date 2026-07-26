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

/// One retry after a short backoff for resume attempts that fail in a way
/// that looks transient (network-level) rather than a genuine session
/// rejection. A single retry is enough to ride out a momentary blip without
/// forcing a full interactive re-login for something that wasn't really an
/// expired session.
const RESUME_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(2);

/// Attempt to resume a previously persisted session with no user
/// interaction (mirrors pyicloud's `_authenticate_with_token()`).
///
/// `Ok(None)` means there is nothing to resume from, or Apple rejected the
/// persisted tokens (expired/invalidated) -- the caller should fall back to
/// an interactive `AppleAuthClient::new(apple_id)` + `login()` flow in that
/// case, exactly as it would on a first run. This is not an error condition.
///
/// The failure reason is classified and logged (via `tracing`) rather than
/// silently discarded, and a network-looking failure gets one retry before
/// giving up -- otherwise a momentary connectivity blip looks identical to a
/// genuinely expired session and forces an unnecessary password/2FA prompt.
pub async fn try_resume_session(
    apple_id: &str,
) -> Result<Option<(reqwest::Client, String, Value)>> {
    let dir = session_store::data_dir()?;
    let cookie_jar = session_store::load_cookie_store(&dir);
    let persisted = session_store::load_auth_state(&dir);

    if persisted.session_token.is_none() {
        tracing::debug!("no persisted session token; nothing to resume");
        return Ok(None);
    }

    let mut client = AppleAuthClient::with_state(apple_id, cookie_jar, &persisted)?;

    let first_attempt = client.try_resume().await;
    let outcome = match first_attempt {
        Ok(data) => Ok(data),
        Err(e) if is_likely_transient(&e) => {
            tracing::warn!(
                error = %e,
                "session resume failed with a network-looking error; retrying once"
            );
            tokio::time::sleep(RESUME_RETRY_DELAY).await;
            client.try_resume().await
        }
        Err(e) => Err(e),
    };

    match outcome {
        Ok(data) => {
            persist_state(&client, &dir)?;
            Ok(Some((client.http_client(), client.client_id().to_string(), data)))
        }
        Err(e) if is_likely_transient(&e) => {
            tracing::warn!(
                error = %e,
                "session resume still failing after retry (network-looking); \
                 falling back to interactive login"
            );
            Ok(None)
        }
        Err(e) => {
            // A clean rejection (e.g. HTTP 421 -- Apple's "session token
            // expired" signal) is the expected, routine reason a resume
            // fails; log at info rather than warn so normal operation
            // doesn't look like an error in the logs.
            tracing::info!(error = %e, "session token rejected; falling back to interactive login");
            Ok(None)
        }
    }
}

/// Heuristic: does this look like a connectivity problem rather than Apple
/// actively rejecting the session? Every error in this codebase is a plain
/// `anyhow::Error` built from `.context(...)`/`bail!` strings (no typed
/// error hierarchy), so this matches on the wording those call sites use
/// rather than a proper error enum. A false negative here just means one
/// fewer retry attempt, not a functional break -- keep the heuristic simple.
fn is_likely_transient(err: &anyhow::Error) -> bool {
    let text = err.to_string().to_lowercase();
    text.contains("request failed")
        || text.contains("timed out")
        || text.contains("timeout")
        || text.contains("connection")
}
