//! Proxy-local vocabulary store (design/idea/expression.md §1): per-reminder
//! attributes that deliberately never round-trip to CloudKit -- exception
//! classes (時報/習慣), ritual groups, 目的, parent links (分解), environment
//! tags -- plus the notifier's own persistent bookkeeping (the notified set,
//! so a restart doesn't re-fire every overdue card, and the weekly
//! meta-reminder timestamp). Card *bodies* live in CloudKit; only their extra
//! meaning lives here. That split-brain is by design (user-interaction §5),
//! so `backup_to_documents` keeps a small generational backup in the user's
//! Documents folder as the loss insurance (expression §1).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyMeta {
    /// Exception class overriding the field-derived default classification:
    /// `"signal"` (時報 -- no task, dies after firing) or `"habit"` (習慣).
    /// Absent = a plain task (the default derived from CloudKit fields).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cls: Option<String>,
    /// Ritual group id linking one event card to its N time-point cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// 目的 -- inherited into successor/child cards so the chain of tasks
    /// stays attached to why it exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// Parent reminder id (分解). One level only -- not a dependency graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Environment tag (家 / PC / 外出 / スーツ ...) -- batching material.
    /// Superseded (U3/U4, 2026-08-01): attributes now live as `[key]` meta
    /// tags inside the card title (iOS-visible, survives store loss); this
    /// field is kept only for backward compatibility of stored files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
    /// upcoming 専用(U1/U2): 発火時にタスクカードを産み込む先のリスト id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_list: Option<String>,
    /// upcoming 専用: 繰り返し間隔(日)。None/0 = 一回きり(発火後に完了)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat_days: Option<i64>,
    /// upcoming 専用: 産んだカードに付ける締切 = 発火日 + このオフセット(日)。
    /// None = 締切不明のまま産む。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_offset_days: Option<i64>,
}

impl ProxyMeta {
    pub fn is_empty(&self) -> bool {
        self.cls.is_none()
            && self.group.is_none()
            && self.purpose.is_none()
            && self.parent.is_none()
            && self.env.is_none()
            && self.target_list.is_none()
            && self.repeat_days.is_none()
            && self.due_offset_days.is_none()
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStore {
    #[serde(default)]
    pub meta: HashMap<String, ProxyMeta>,
    /// Reminder ids that already produced a Windows notification.
    #[serde(default)]
    pub notified: HashSet<String>,
    /// Last time the weekly "締切不明の課題がN件" meta-reminder fired.
    #[serde(default)]
    pub last_meta_reminder: Option<DateTime<Utc>>,
    /// Registered attribute keys (U4): the vocabulary offered by the tag
    /// picker / session declaration. The tags themselves live in card
    /// titles as `[key]`; only the *vocabulary* is local. Seeded once with
    /// defaults on first run -- afterwards this file is the single source
    /// of truth (hand-editable).
    #[serde(default)]
    pub env_keys: Vec<String>,
    /// Lists excluded from the dashboard aggregation (メモ系など、タスクに
    /// 対応しないリスト)。サイドバーのアイコンクリックでトグルされる。
    #[serde(default)]
    pub excluded_lists: HashSet<String>,
}

fn store_path(dir: &Path) -> PathBuf {
    dir.join("proxy_store.json")
}

/// Missing/corrupt file = start fresh, matching session_store's stance.
pub fn load(dir: &Path) -> ProxyStore {
    fs::read_to_string(store_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, store: &ProxyStore) -> Result<()> {
    let json = serde_json::to_string_pretty(store)?;
    fs::write(store_path(dir), json).context("failed to write proxy_store.json")
}

/// The store has two writers (Tauri commands on the UI side, the background
/// poller) doing load-modify-save cycles; this process-wide lock keeps those
/// cycles atomic so neither writer clobbers the other's fields.
static STORE_LOCK: Mutex<()> = Mutex::new(());

pub fn with_store<T>(dir: &Path, f: impl FnOnce(&mut ProxyStore) -> T) -> Result<T> {
    let _guard = STORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut store = load(dir);
    let out = f(&mut store);
    save(dir, &store)?;
    Ok(out)
}

const BACKUP_KEEP: usize = 5;

/// Generational startup backup into `Documents\reminder-proxy-client\`
/// (keeps the newest `BACKUP_KEEP`). Returns the created path, or `None`
/// when there is nothing to back up yet.
pub fn backup_to_documents(dir: &Path) -> Result<Option<PathBuf>> {
    let src = store_path(dir);
    if !src.exists() {
        return Ok(None);
    }
    let user_dirs = directories::UserDirs::new().context("could not resolve user directories")?;
    let docs = user_dirs
        .document_dir()
        .context("could not resolve the Documents directory")?;
    let target_dir = docs.join("reminder-proxy-client");
    fs::create_dir_all(&target_dir).context("failed to create backup directory")?;

    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let dest = target_dir.join(format!("proxy_store.{stamp}.json"));
    fs::copy(&src, &dest).context("failed to copy proxy store backup")?;

    // Prune old generations (lexicographic order == chronological order,
    // because the stamp format is fixed-width year-first).
    let mut backups: Vec<PathBuf> = fs::read_dir(&target_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("proxy_store.") && n.ends_with(".json"))
        })
        .collect();
    backups.sort();
    while backups.len() > BACKUP_KEEP {
        let oldest = backups.remove(0);
        let _ = fs::remove_file(oldest);
    }
    Ok(Some(dest))
}
