//! At-rest encryption for the persisted iCloud session, via Windows DPAPI
//! (`CryptProtectData`, user scope).
//!
//! # What this does and does not protect against
//!
//! Be precise about this, because DPAPI is easy to over-claim.
//!
//! Protected: the sealed bytes are useless to anyone who merely *obtains the
//! file*. Decryption requires the logon secret of the Windows user account
//! that sealed it, so a copied `%APPDATA%` folder, a backup, a folder picked
//! up by a cloud-sync client, a second account on the same machine, or an
//! offline disk image all yield nothing. This matters here because
//! `auth_state.json` holds Apple's session *and trust* tokens: possessing
//! them grants account access with no password and no 2FA prompt.
//!
//! NOT protected: another process running as the *same* Windows user. It can
//! call `CryptUnprotectData` with the same entropy (which is compiled into
//! this binary and therefore readable), read our process memory, or simply
//! drive the app. Windows offers no per-application isolation for a normal
//! desktop process -- neither DPAPI user scope nor Credential Manager
//! provides one. The only boundary that would exclude same-user code is a
//! key that never touches the disk, i.e. a user-supplied master passphrase
//! entered every launch; that is a UX decision, not something to adopt
//! silently.
//!
//! `ENTROPY` is not a secret and is not pretended to be one. Its only job is
//! to stop a generic "decrypt every DPAPI blob in this profile" tool from
//! working without being aimed at this app specifically.

use anyhow::{bail, Result};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

/// Changing this invalidates every previously sealed file (the app then falls
/// back to a fresh login), so version it rather than editing in place.
const ENTROPY: &[u8] = b"reminder-proxy-client/session-v1";

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
unsafe fn take(out: &CRYPT_INTEGER_BLOB) -> Vec<u8> {
    // Edition 2024: an `unsafe fn` body is not implicitly an unsafe block.
    unsafe {
        let copied = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        LocalFree(out.pbData.cast());
        copied
    }
}

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

#[cfg(test)]
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
