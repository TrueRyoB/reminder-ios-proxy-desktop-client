//! Background due-reminder polling, ported from the CLI's `watch` command.
//! Apple exposes no push mechanism for Reminders, so this is the only way
//! to surface a due reminder while the app is running -- it keeps running
//! after the window is hidden (see `lib.rs`'s `CloseRequested` handler),
//! which is the whole point of also having a system tray icon.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use reminder_core::{notify, reminders::RemindersService};
use tauri::{AppHandle, Emitter};

const POLL_INTERVAL: Duration = Duration::from_secs(300);

pub fn spawn(app: AppHandle, reminders: Arc<RemindersService>) {
    tauri::async_runtime::spawn(async move {
        let mut notified: HashSet<String> = HashSet::new();
        loop {
            match check_due_reminders(&reminders, &mut notified).await {
                Ok(0) => {}
                Ok(count) => {
                    tracing::info!(count, "sent due-reminder notifications");
                    // Lets the frontend re-fetch the currently open view so a
                    // reminder that just fired a notification also shows as
                    // overdue/updated without the user having to reselect it.
                    let _ = app.emit("reminders-changed", ());
                }
                Err(e) => tracing::warn!(error = %e, "polling for due reminders failed"),
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    });
}

async fn check_due_reminders(
    reminders: &RemindersService,
    notified: &mut HashSet<String>,
) -> anyhow::Result<usize> {
    let lists = reminders.lists().await?;
    let now = chrono::Utc::now();
    let mut count = 0;

    for list in &lists {
        let items = reminders.list_reminders(&list.id, false).await?;
        for r in items {
            if r.completed {
                continue;
            }
            let Some(due) = r.due_date else { continue };
            if due <= now && !notified.contains(&r.id) {
                notify::send(
                    &r.title,
                    &format!("{} - 期限: {}", list.title, due.format("%Y-%m-%d %H:%M")),
                )?;
                notified.insert(r.id.clone());
                count += 1;
            }
        }
    }
    Ok(count)
}
