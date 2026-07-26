//! Persist cookies + Apple's session/trust tokens to disk so a restart
//! doesn't require re-entering a password or 2FA code, mirroring pyicloud's
//! `cookie_directory` + `_authenticate_with_token()` resume path.

use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cookie_store::CookieStore;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PersistedAuthState {
    /// Needed so a GUI consumer can silently resume a session on startup
    /// without any username input (the CLI sidesteps this by requiring
    /// `--apple-id` on every invocation, but that's not an option for a GUI).
    pub apple_id: Option<String>,
    pub session_token: Option<String>,
    pub trust_token: Option<String>,
    pub account_country: Option<String>,
    /// pyicloud persists and reuses this across runs (see `self._client_id`
    /// restored from `session.data`) rather than minting a new one every
    /// time; CloudKit calls send it as the `clientId` query param.
    pub client_id: Option<String>,
}

pub fn data_dir() -> Result<PathBuf> {
    let proj = ProjectDirs::from("", "", "reminder-proxy-client")
        .context("could not resolve a config directory for this platform")?;
    let dir = proj.data_dir().to_path_buf();
    fs::create_dir_all(&dir).context("failed to create config directory")?;
    Ok(dir)
}

fn cookies_path(dir: &Path) -> PathBuf {
    dir.join("cookies.json")
}

fn auth_state_path(dir: &Path) -> PathBuf {
    dir.join("auth_state.json")
}

fn list_cache_path(dir: &Path) -> PathBuf {
    dir.join("list_cache.json")
}

/// Backs `RemindersService::lists_cached` (see QA-A): without this, every
/// app launch replayed the account's *entire* List change history via
/// CloudKit's `/changes/zone` (no sync token = "everything since zone
/// creation"), measured at ~30s on an account with a lot of history.
/// Persisting the merged cache + sync token here makes every launch after
/// the first an incremental diff instead.
pub fn load_list_cache(dir: &Path) -> crate::reminders::ListCache {
    fs::read_to_string(list_cache_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_list_cache(dir: &Path, cache: &crate::reminders::ListCache) -> Result<()> {
    let json = serde_json::to_string_pretty(cache)?;
    fs::write(list_cache_path(dir), json).context("failed to write list_cache.json")
}

/// Returns an empty store if nothing is persisted yet, or the persisted
/// file is unreadable/corrupt (treated as "start fresh", not a hard error).
pub fn load_cookie_store(dir: &Path) -> CookieStore {
    match File::open(cookies_path(dir)) {
        Ok(f) => cookie_store::serde::json::load(BufReader::new(f)).unwrap_or_default(),
        Err(_) => CookieStore::default(),
    }
}

pub fn save_cookie_store(dir: &Path, store: &CookieStore) -> Result<()> {
    let mut f = File::create(cookies_path(dir)).context("failed to create cookies.json")?;
    cookie_store::serde::json::save(store, &mut f)
        .map_err(|e| anyhow::anyhow!("failed to serialize cookie store: {e}"))
}

pub fn load_auth_state(dir: &Path) -> PersistedAuthState {
    fs::read_to_string(auth_state_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_auth_state(dir: &Path, state: &PersistedAuthState) -> Result<()> {
    let json = serde_json::to_string_pretty(state)?;
    fs::write(auth_state_path(dir), json).context("failed to write auth_state.json")
}
