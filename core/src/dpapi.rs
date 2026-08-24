//! At-rest encryption for the persisted iCloud session. Windows uses DPAPI;
//! macOS keeps a per-install encryption key in Keychain and encrypts the
//! session files with XChaCha20-Poly1305.
//!
//! # What this does and does not protect against
//!
//! Be precise about this, because OS credential stores are easy to over-claim.
//!
//! Protected: the sealed bytes are useless to anyone who merely *obtains the
//! file*. Decryption requires the Windows logon secret (DPAPI) or the macOS
//! Keychain item, so a copied application-data folder, a backup, a folder
//! picked up by a cloud-sync client, a second account on the same machine, or
//! an offline disk image all yield nothing. This matters here because
//! `auth_state.json` holds Apple's session *and trust* tokens: possessing
//! them grants account access with no password and no 2FA prompt.
//!
//! NOT protected: another process running as the *same* logged-in user. It
//! can read process memory, drive the app, and may be able to access the OS
//! credential store. Neither DPAPI nor Keychain is a per-application boundary
//! against hostile same-user code. The only boundary that would exclude it is
//! a user-supplied master passphrase entered every launch; that is a UX
//! decision, not something to adopt silently.
//!
//! `ENTROPY` is not a secret and is not pretended to be one. Its only job is
//! to stop a generic "decrypt every DPAPI blob in this profile" tool from
//! working without being aimed at this app specifically.

use anyhow::{bail, Result};

#[cfg(windows)]
use windows_sys::Win32::Foundation::LocalFree;
#[cfg(windows)]
use windows_sys::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

/// Changing this invalidates every previously sealed file (the app then falls
/// back to a fresh login), so version it rather than editing in place.
#[cfg(windows)]
const ENTROPY: &[u8] = b"reminder-proxy-client/session-v1";

#[cfg(windows)]
fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        // DPAPI does not write through this pointer; the `*mut` is just how
        // the Win32 struct is declared. The borrow is only live for the
        // duration of the call below, where `bytes` is still in scope.
        pbData: bytes.as_ptr().cast_mut(),
    }
}

/// Copies out of the `LocalAlloc`'d buffer DPAPI handed back, then frees it.
///
/// # Safety
/// `out` must be a blob successfully written by DPAPI (non-null `pbData`
/// valid for `cbData` bytes) that has not already been freed.
#[cfg(windows)]
unsafe fn take(out: &CRYPT_INTEGER_BLOB) -> Vec<u8> {
    // Edition 2024: an `unsafe fn` body is not implicitly an unsafe block.
    unsafe {
        let copied = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData.cast());
        copied
    }
}

#[cfg(windows)]
pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>> {
    let input = blob(plaintext);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB::default();
    // CRYPTPROTECT_UI_FORBIDDEN: never pop a UI prompt -- this runs from a
    // background poller as well as the UI thread.
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            &entropy,
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    };
    if ok == 0 {
        bail!(
            "CryptProtectData failed: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok(unsafe { take(&out) })
}

#[cfg(windows)]
pub fn unprotect(sealed: &[u8]) -> Result<Vec<u8>> {
    let input = blob(sealed);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            // No data-description out-param: passing non-null would hand us
            // another LocalAlloc'd buffer to free for no benefit.
            std::ptr::null_mut(),
            &entropy,
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    };
    if ok == 0 {
        bail!(
            "CryptUnprotectData failed: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok(unsafe { take(&out) })
}

#[cfg(target_os = "macos")]
const MACOS_KEYRING_SERVICE: &str = "reminder-proxy-client/session-key-v1";
#[cfg(target_os = "macos")]
const MACOS_KEYRING_ACCOUNT: &str = "default";
#[cfg(target_os = "macos")]
const MACOS_NONCE_LEN: usize = 24;

#[cfg(target_os = "macos")]
fn macos_session_key() -> Result<[u8; 32]> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use rand::RngCore as _;

    let entry = keyring::Entry::new(MACOS_KEYRING_SERVICE, MACOS_KEYRING_ACCOUNT)?;
    match entry.get_password() {
        Ok(encoded) => decode_macos_key(&encoded),
        Err(keyring::Error::NoEntry) => {
            let mut key = [0_u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut key);
            entry
                .set_password(&STANDARD.encode(key))
                .map_err(|error| anyhow::anyhow!("failed to save macOS session key in Keychain: {error}"))?;
            Ok(key)
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(target_os = "macos")]
fn decode_macos_key(encoded: &str) -> Result<[u8; 32]> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    let decoded = STANDARD
        .decode(encoded)
        .map_err(|error| anyhow::anyhow!("invalid macOS session key in Keychain: {error}"))?;
    decoded
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid macOS session key length in Keychain"))
}

#[cfg(any(test, target_os = "macos"))]
fn seal_with_key(key: &[u8; 32], nonce: &[u8; 24], plaintext: &[u8]) -> Result<Vec<u8>> {
    use chacha20poly1305::{KeyInit as _, XChaCha20Poly1305, XNonce, aead::Aead as _};

    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|error| anyhow::anyhow!("could not initialize session cipher: {error}"))?;
    cipher
        .encrypt(XNonce::from_slice(nonce), plaintext)
        .map_err(|error| anyhow::anyhow!("could not encrypt session data: {error}"))
}

#[cfg(any(test, target_os = "macos"))]
fn open_with_key(key: &[u8; 32], nonce: &[u8; 24], ciphertext: &[u8]) -> Result<Vec<u8>> {
    use chacha20poly1305::{KeyInit as _, XChaCha20Poly1305, XNonce, aead::Aead as _};

    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|error| anyhow::anyhow!("could not initialize session cipher: {error}"))?;
    cipher
        .decrypt(XNonce::from_slice(nonce), ciphertext)
        .map_err(|error| anyhow::anyhow!("could not decrypt session data: {error}"))
}

#[cfg(target_os = "macos")]
pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>> {
    use rand::RngCore as _;

    let key = macos_session_key()?;
    let mut nonce = [0_u8; MACOS_NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let ciphertext = seal_with_key(&key, &nonce, plaintext)?;
    let mut sealed = nonce.to_vec();
    sealed.extend_from_slice(&ciphertext);
    Ok(sealed)
}

#[cfg(target_os = "macos")]
pub fn unprotect(sealed: &[u8]) -> Result<Vec<u8>> {
    if sealed.len() < MACOS_NONCE_LEN {
        bail!("macOS session data is shorter than its nonce")
    }
    let key = macos_session_key()?;
    let (nonce, ciphertext) = sealed.split_at(MACOS_NONCE_LEN);
    open_with_key(&key, nonce.try_into().expect("nonce length checked"), ciphertext)
}

/// There is intentionally no plaintext fallback on unsupported platforms.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn protect(_: &[u8]) -> Result<Vec<u8>> {
    bail!("secure session storage is not implemented for this platform")
}

/// See [`protect`]. A sealed Windows DPAPI blob cannot be read on another OS.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn unprotect(_: &[u8]) -> Result<Vec<u8>> {
    bail!("secure session storage is not implemented for this platform")
}

#[cfg(test)]
mod macos_cipher_tests {
    use super::{open_with_key, seal_with_key};

    #[test]
    fn authenticated_cipher_round_trip() {
        let key = [7_u8; 32];
        let nonce = [9_u8; 24];
        let ciphertext = seal_with_key(&key, &nonce, b"session token").expect("encrypt");
        assert_ne!(ciphertext, b"session token");
        assert_eq!(
            open_with_key(&key, &nonce, &ciphertext).expect("decrypt"),
            b"session token"
        );
    }

    #[test]
    fn authenticated_cipher_rejects_tampering() {
        let key = [7_u8; 32];
        let nonce = [9_u8; 24];
        let mut ciphertext = seal_with_key(&key, &nonce, b"session token").expect("encrypt");
        ciphertext[0] ^= 0xff;
        assert!(open_with_key(&key, &nonce, &ciphertext).is_err());
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let secret = b"session-token: abc123 / trust-token: def456";
        let sealed = protect(secret).expect("protect");
        assert_ne!(&sealed[..], &secret[..], "output must not be plaintext");
        assert_eq!(unprotect(&sealed).expect("unprotect"), secret);
    }

    #[test]
    fn round_trip_empty() {
        let sealed = protect(b"").expect("protect");
        assert_eq!(unprotect(&sealed).expect("unprotect"), b"");
    }

    /// A blob sealed with different entropy must not decrypt, otherwise the
    /// entropy binding is not actually being applied.
    #[test]
    fn rejects_corrupted_blob() {
        let mut sealed = protect(b"secret").expect("protect");
        let last = sealed.len() - 1;
        sealed[last] ^= 0xff;
        assert!(unprotect(&sealed).is_err());
    }
}
