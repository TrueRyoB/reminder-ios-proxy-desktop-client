//! Windows toast notifications via `notify-rust` (wraps the WinRT toast
//! notification API). Requires the calling process to be running while the
//! notification fires -- there is no way to wake up when fully closed, since
//! Apple exposes no push mechanism for Reminders (see project notes).

use anyhow::{Context, Result};

pub fn send(title: &str, body: &str) -> Result<()> {
    notify_rust::Notification::new()
        .appname("iCloud Reminders")
        .summary(title)
        .body(body)
        .show()
        .context("failed to show Windows toast notification")?;
    Ok(())
}
