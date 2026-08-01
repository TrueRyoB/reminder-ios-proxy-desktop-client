//! Background axis-reading notifier (design/idea/expression.md §3), evolved
//! from the original due-reminder poller. Apple exposes no push mechanism
//! for Reminders, so polling is the only way to fire while the app runs --
//! it survives window-hide (see `lib.rs`'s `CloseRequested` handler).
//!
//! What "reading the axes" means here (v1):
//! - all-day due dates are deadlines (締切) and NEVER ring -- only timed
//!   dates (発火時刻) do;
//! - the notified set persists in the proxy store, so a restart doesn't
//!   re-fire every overdue card;
//! - fired signal cards (時報/儀式の時点, proxy-local `cls == "signal"`)
//!   are auto-completed afterward (衝突C: 完了扱い, reversible);
//! - a weekly meta-reminder reports how many deadline-less tasks are
//!   sitting in the backlog (漏れない保証の時間側).
//!
//! Deferred (recorded in handan/0033): sub-poll-interval precise timers and
//! habit-rhythm periodic prompts.

use std::sync::Arc;
use std::time::Duration;

use reminder_core::{notify, proxy_store, reminders::RemindersService, session_store};
use tauri::{AppHandle, Emitter};

const POLL_INTERVAL: Duration = Duration::from_secs(300);
const META_REMINDER_EVERY_DAYS: i64 = 7;

pub fn spawn(app: AppHandle, reminders: Arc<RemindersService>) {
    tauri::async_runtime::spawn(async move {
        loop {
            match check_due_reminders(&reminders).await {
                Ok(0) => {}
                Ok(count) => {
                    tracing::info!(count, "sent notifications");
                    // Lets the frontend re-fetch the currently open view so a
                    // reminder that just fired (or a signal card that was
                    // auto-completed) shows its new state without a manual
                    // reselect.
                    let _ = app.emit("reminders-changed", ());
                }
                Err(e) => tracing::warn!(error = %e, "polling for due reminders failed"),
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    });
}

async fn check_due_reminders(reminders: &RemindersService) -> anyhow::Result<usize> {
    let dir = session_store::data_dir()?;
    let store = proxy_store::load(&dir);

    // Incremental list fetch (same ListCache as `list_lists`) -- the old
    // `lists()` call replayed the account's entire change history every 5
    // minutes (~30s measured on a busy account, see QA-A).
    let mut cache = session_store::load_list_cache(&dir);
    let lists = reminders.lists_cached(&mut cache).await?;
    if let Err(e) = session_store::save_list_cache(&dir, &cache) {
        tracing::warn!(error = %e, "failed to persist list cache from poller");
    }

    let now = chrono::Utc::now();
    let mut newly_notified: Vec<String> = Vec::new();
    let mut fired_signals: Vec<reminder_core::reminders::Reminder> = Vec::new();
    let mut deadline_less_tasks = 0usize;

    // U1/U2: "upcoming" という名のリストは待機庫。カードは通知の対象外で、
    // 期日到来で行き先リストへ実カードを産む(process_upcoming)。
    let upcoming_id = lists
        .iter()
        .find(|l| l.title.trim().eq_ignore_ascii_case("upcoming"))
        .map(|l| l.id.clone());

    for list in &lists {
        if Some(&list.id) == upcoming_id.as_ref() {
            continue;
        }
        let items = reminders.list_reminders(&list.id, false).await?;
        for r in items {
            if r.completed {
                continue;
            }
            let meta = store.meta.get(&r.id);
            let cls = meta.and_then(|m| m.cls.as_deref());

            // 集計除外リスト(メモ系)の課題は週次メタリマインドの残高に数えない
            if r.due_date.is_none() && cls.is_none() && !store.excluded_lists.contains(&list.id) {
                deadline_less_tasks += 1;
            }

            let Some(due) = r.due_date else { continue };
            // 終日=締切: pull-side sorting material only. Never ring.
            if r.all_day {
                continue;
            }
            if due <= now && !store.notified.contains(&r.id) {
                notify::send(
                    &r.title,
                    &format!("{} - {}", list.title, due.format("%Y-%m-%d %H:%M")),
                )?;
                newly_notified.push(r.id.clone());
                if cls == Some("signal") {
                    fired_signals.push(r);
                }
            }
        }
    }

    // 衝突C: fired signal cards die by completion (reversible; they sink
    // into iOS's completed section rather than being deleted).
    let mut completed = 0usize;
    for mut r in fired_signals {
        r.completed = true;
        match reminders.update(&r).await {
            Ok(_) => completed += 1,
            Err(e) => tracing::warn!(error = %e, id = %r.id, "failed to auto-complete signal card"),
        }
    }

    // U1/U2: upcoming の発火処理。
    let mut spawned = 0usize;
    if let Some(up_id) = &upcoming_id {
        match process_upcoming(reminders, up_id, &store, now).await {
            Ok(n) => spawned = n,
            Err(e) => tracing::warn!(error = %e, "processing upcoming list failed"),
        }
    }

    // 漏れない保証(時間側): weekly nudge back to the reference surface.
    let meta_due = store
        .last_meta_reminder
        .map(|t| (now - t).num_days() >= META_REMINDER_EVERY_DAYS)
        .unwrap_or(true);
    let mut meta_fired = false;
    if meta_due && deadline_less_tasks > 0 {
        notify::send(
            "今週の見直し",
            &format!("締切不明の課題が{deadline_less_tasks}件あります。ダッシュボードで皿に混ぜましょう。"),
        )?;
        meta_fired = true;
    }

    if !newly_notified.is_empty() || meta_fired {
        let count = newly_notified.len();
        proxy_store::with_store(&dir, move |s| {
            s.notified.extend(newly_notified);
            if meta_fired {
                s.last_meta_reminder = Some(now);
            }
        })?;
        return Ok(count + completed + spawned + usize::from(meta_fired));
    }
    Ok(completed + spawned)
}

/// U1/U2 (design/artist/dashboard.md 未決事項の解決, 2026-08-01):
/// upcoming リストのカードは「開始可能時間つきのタスク定義」。期日が到来
/// したら、行き先リスト(ローカル meta の target_list)へ実カードを産む。
/// 繰り返し(repeat_days)があれば自分の期日を次回へ進めて待機庫に残り、
/// なければ完了して消化される。産むカードの締切は 発火日+due_offset_days
/// (未設定なら締切不明のまま)。習慣系はこのループで実現される。
async fn process_upcoming(
    reminders: &RemindersService,
    upcoming_list_id: &str,
    store: &reminder_core::proxy_store::ProxyStore,
    now: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<usize> {
    let items = reminders.list_reminders(upcoming_list_id, false).await?;
    let mut spawned = 0usize;
    for r in items {
        if r.completed {
            continue;
        }
        let Some(due) = r.due_date else { continue };
        if due > now {
            continue;
        }
        let meta = store.meta.get(&r.id);
        let Some(target) = meta.and_then(|m| m.target_list.clone()) else {
            tracing::warn!(id = %r.id, "upcoming card has no target list; skipping");
            continue;
        };
        let offset = meta.and_then(|m| m.due_offset_days);
        let spawn_due = offset.map(|d| due + chrono::Duration::days(d));
        reminders
            .create(
                &target,
                &r.title,
                &r.desc,
                r.priority,
                false,
                spawn_due,
                spawn_due.is_some(), // 締切は終日=鳴らない担体で持つ
            )
            .await?;
        spawned += 1;

        let repeat = meta.and_then(|m| m.repeat_days).unwrap_or(0);
        let mut card = r.clone();
        if repeat > 0 {
            // 次回へ再装填(取り逃した分を now を跨ぐまで進める)
            let mut next = due;
            while next <= now {
                next += chrono::Duration::days(repeat);
            }
            card.due_date = Some(next);
        } else {
            card.completed = true;
        }
        if let Err(e) = reminders.update(&card).await {
            tracing::warn!(error = %e, id = %card.id, "failed to re-arm/complete upcoming card");
        }
    }
    Ok(spawned)
}
