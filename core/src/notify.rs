//! Desktop notifications via `notify-rust`: WinRT on Windows and
//! UserNotifications on macOS. The process must be running while a notification
//! fires -- Apple exposes no Reminders push mechanism that can wake it when
//! fully closed.

use anyhow::Result;
#[cfg(windows)]
use anyhow::Context;

/// Stable identity for unpackaged Windows toasts.  Without this, notify-rust
/// falls back to PowerShell's AUMID and Windows may route or suppress the
/// toast as belonging to PowerShell instead of this application.
#[cfg(windows)]
const WINDOWS_AUMID: &str = "com.trueryob.reminderproxyclient";
#[cfg(windows)]
const WINDOWS_DISPLAY_NAME: &str = "iCloud Reminders";

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Register this unpackaged application's AUMID for the current user.  The
/// executable is a valid icon source on Windows, so one path supplies both
/// the icon resource and a useful diagnostic anchor in the registry.
#[cfg(windows)]
fn register_windows_aumid() -> Result<()> {
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegSetValueExW,
    };

    let exe =
        std::env::current_exe().context("could not determine notification executable path")?;
    let exe = exe.to_string_lossy();
    let key_path = format!("Software\\Classes\\AppUserModelId\\{WINDOWS_AUMID}");
    let mut key: HKEY = std::ptr::null_mut();
    let disposition = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            wide(&key_path).as_ptr(),
            0,
            std::ptr::null_mut(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            std::ptr::null(),
            &mut key,
            std::ptr::null_mut(),
        )
    };
    if disposition != 0 {
        anyhow::bail!("RegCreateKeyExW failed with Windows error {disposition}");
    }
    let write_string = |name: &str, value: &str| -> Result<()> {
        let value = wide(value);
        let status = unsafe {
            RegSetValueExW(
                key,
                wide(name).as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * std::mem::size_of::<u16>()) as u32,
            )
        };
        if status != 0 {
            anyhow::bail!("RegSetValueExW({name}) failed with Windows error {status}");
        }
        Ok(())
    };
    let result = (|| {
        write_string("DisplayName", WINDOWS_DISPLAY_NAME)?;
        write_string("IconUri", &exe)?;
        write_string("Executable", &exe)?;
        Ok(())
    })();
    unsafe { RegCloseKey(key) };
    result
}

#[cfg(any(windows, target_os = "macos"))]
pub fn send(title: &str, body: &str) -> Result<()> {
    #[cfg(windows)]
    {
        if let Err(error) = register_windows_aumid() {
            tracing::error!(error = %error, aumid = WINDOWS_AUMID, "Windows toast AUMID registration failed");
            return Err(error).context("failed to register Windows toast identity");
        }
    }

    let mut notification = notify_rust::Notification::new();
    notification
        .appname("iCloud Reminders")
        .summary(title)
        .body(body);
    #[cfg(windows)]
    notification.app_id(WINDOWS_AUMID);
    if let Err(error) = notification.show() {
        tracing::error!(error = %error, "desktop notification delivery failed");
        return Err(anyhow::anyhow!("failed to show desktop notification: {error}"));
    }
    Ok(())
}

/// Linux and other platforms have no supported notification backend yet.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn send(_: &str, _: &str) -> Result<()> {
    anyhow::bail!("desktop notifications are not implemented for this platform")
}
