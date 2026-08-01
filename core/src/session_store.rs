//! Persist cookies + Apple's session/trust tokens to disk so a restart
//! doesn't require re-entering a password or 2FA code, mirroring pyicloud's
//! `cookie_directory` + `_authenticate_with_token()` resume path.

use std::fs;
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

/// Prefix on a DPAPI-sealed file. Anything without it is a pre-sealing
/// plaintext file and gets re-sealed on first read (see `read_sealed`).
///
/// `list_cache.json` and `proxy_store.json` deliberately stay plaintext: they
/// hold list/reminder metadata, not credentials, and `proxy_store.json` is
/// documented as hand-editable.
const SEAL_MAGIC: &[u8; 4] = b"RPC1";

fn seal(plaintext: &[u8]) -> Result<Vec<u8>> {
    let mut out = SEAL_MAGIC.to_vec();
    out.extend_from_slice(&crate::dpapi::protect(plaintext)?);
    Ok(out)
}

fn write_sealed(path: &Path, plaintext: &[u8]) -> Result<()> {
    let sealed = seal(plaintext)?;
    // Same path, so this truncates the old contents rather than leaving a
    // second copy behind.
    fs::write(path, sealed).with_context(|| format!("failed to write {}", path.display()))
}

/// Reads a sealed file. A file still in the clear is migrated in place before
/// returning, so upgrading the app stops leaving the tokens readable without
/// requiring a re-login.
///
/// `None` means "nothing usable here" -- absent, or sealed by a different
/// Windows user/machine. That is a start-fresh condition (the caller falls
/// back to an interactive login), not an error.
fn read_sealed(path: &Path) -> Option<Vec<u8>> {
    let raw = fs::read(path).ok()?;
    match raw.strip_prefix(SEAL_MAGIC) {
        Some(blob) => match crate::dpapi::unprotect(blob) {
            Ok(plain) => Some(plain),
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "could not decrypt persisted session (sealed by a different \
                     Windows user or machine?); starting fresh"
                );
                None
            }
        },
        None => {
            if let Err(e) = write_sealed(path, &raw) {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "failed to encrypt the pre-existing plaintext session file; \
                     it is still readable on disk"
                );
            } else {
                tracing::info!(
                    path = %path.display(),
                    "encrypted a pre-existing plaintext session file in place"
                );
            }
            Some(raw)
        }
    }
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
    match read_sealed(&cookies_path(dir)) {
        Some(plain) => cookie_store::serde::json::load(plain.as_slice()).unwrap_or_default(),
        None => CookieStore::default(),
    }
}

pub fn save_cookie_store(dir: &Path, store: &CookieStore) -> Result<()> {
    let mut json: Vec<u8> = Vec::new();
    cookie_store::serde::json::save(store, &mut json)
        .map_err(|e| anyhow::anyhow!("failed to serialize cookie store: {e}"))?;
    write_sealed(&cookies_path(dir), &json)
}

pub fn load_auth_state(dir: &Path) -> PersistedAuthState {
    read_sealed(&auth_state_path(dir))
        .and_then(|plain| serde_json::from_slice(&plain).ok())
        .unwrap_or_default()
}

pub fn save_auth_state(dir: &Path, state: &PersistedAuthState) -> Result<()> {
    let json = serde_json::to_vec_pretty(state)?;
    write_sealed(&auth_state_path(dir), &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique scratch dir without pulling in a temp-file dependency; the
    /// counter keeps parallel tests in the same process from colliding.
    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rpc-session-test-{}-{tag}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample() -> PersistedAuthState {
        PersistedAuthState {
            apple_id: Some("someone@example.com".into()),
            session_token: Some("session-token-value".into()),
            trust_token: Some("trust-token-value".into()),
            account_country: Some("JPN".into()),
            client_id: Some("client-id-value".into()),
        }
    }

    #[test]
    fn auth_state_is_not_written_in_the_clear() {
        let dir = scratch("write");
        save_auth_state(&dir, &sample()).unwrap();

        let raw = fs::read(auth_state_path(&dir)).unwrap();
        assert!(raw.starts_with(SEAL_MAGIC), "file should be sealed");
        assert!(
            !String::from_utf8_lossy(&raw).contains("session-token-value"),
            "the token must not be recoverable from the file bytes"
        );
        assert_eq!(load_auth_state(&dir).session_token, sample().session_token);

        fs::remove_dir_all(&dir).ok();
    }

    /// The upgrade path: a file left in the clear by an older version is
    /// re-sealed on first read, without losing the session it holds.
    #[test]
    fn plaintext_auth_state_is_migrated_in_place() {
        let dir = scratch("migrate");
        let path = auth_state_path(&dir);
        fs::write(&path, serde_json::to_vec_pretty(&sample()).unwrap()).unwrap();

        let loaded = load_auth_state(&dir);
        assert_eq!(loaded.session_token, sample().session_token);

        let raw = fs::read(&path).unwrap();
        assert!(raw.starts_with(SEAL_MAGIC), "should be sealed after read");
        assert!(!String::from_utf8_lossy(&raw).contains("session-token-value"));

        // Still readable on the next launch, now via the sealed path.
        assert_eq!(load_auth_state(&dir).trust_token, sample().trust_token);

        fs::remove_dir_all(&dir).ok();
    }

    /// A blob we cannot decrypt (sealed by another Windows user, or damaged)
    /// must degrade to "log in again", not propagate an error.
    #[test]
    fn undecryptable_auth_state_starts_fresh() {
        let dir = scratch("corrupt");
        let path = auth_state_path(&dir);
        let mut sealed = seal(b"{}").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0xff;
        fs::write(&path, sealed).unwrap();

        assert!(load_auth_state(&dir).session_token.is_none());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cookie_store_round_trips_sealed() {
        let dir = scratch("cookies");
        let store = CookieStore::default();
        save_cookie_store(&dir, &store).unwrap();

        let raw = fs::read(cookies_path(&dir)).unwrap();
        assert!(raw.starts_with(SEAL_MAGIC));
        assert_eq!(load_cookie_store(&dir).iter_any().count(), 0);

        fs::remove_dir_all(&dir).ok();
    }
}
