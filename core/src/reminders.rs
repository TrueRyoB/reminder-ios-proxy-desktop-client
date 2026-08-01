//! Reminders CRUD + reorder, on top of the raw CloudKit client.
//! Ported from pyicloud's `RemindersReadAPI`/`RemindersWriteAPI`, scoped to
//! what this project actually needs: list discovery, per-list reminder
//! fetch, single-reminder CRUD, and manual reordering.

use anyhow::{anyhow, Result};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::cloudkit::{
    field_bool, field_int, field_reference_name, field_string, field_text, record_change_tag,
    record_name, record_type, CloudKitClient,
};
use crate::crdt::{decode_crdt_document, encode_crdt_document};

const ZONE_NAME: &str = "Reminders";
const ZONE_TYPE: &str = "REGULAR_CUSTOM_ZONE";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemindersList {
    pub id: String,
    pub title: String,
    /// The manual sort order within this list -- the actual field CloudKit
    /// stores (`ReminderIDs`, a JSON-array-encoded string) is what iCloud's
    /// own web/native UI reorders when you drag a reminder.
    pub reminder_ids: Vec<String>,
    pub record_change_tag: Option<String>,
    /// The user's chosen list color, as a `#RRGGBB` hex string. CloudKit
    /// stores this as a JSON-encoded string in the `Color` field (itself
    /// containing `daHexString` alongside RGB floats/symbolic names); we
    /// only need the hex string for the sidebar's colored list badge.
    pub color_hex: Option<String>,
    /// Apple's internal icon-set identifier for this list's badge (e.g.
    /// `"people2"`, `"sport6"`) -- an SF Symbol-adjacent name, not an SF
    /// Symbol itself (which can't be embedded/licensed). The frontend maps
    /// known prefixes to a plain Unicode glyph and falls back to the list's
    /// initial letter for anything unrecognized.
    pub badge_emblem: Option<String>,
}

/// A caller-persisted cache backing `RemindersService::lists_cached`: the
/// last-seen sync token plus every raw List record seen so far, keyed by
/// `recordName`. Callers are responsible for loading/saving this across
/// restarts (see `session_store::load_list_cache`/`save_list_cache`) --
/// this type only knows how to merge an incremental diff into itself.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ListCache {
    pub sync_token: Option<String>,
    pub records: std::collections::HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: String,
    pub list_id: String,
    pub title: String,
    pub desc: String,
    pub completed: bool,
    pub due_date: Option<DateTime<Utc>>,
    /// CloudKit `CreationDate` -- read-only here; drives the aging boost
    /// that floats long-neglected deadline-less tasks into the plate
    /// (design/idea/dashboard.md の編成スコア).
    pub created: Option<DateTime<Utc>>,
    pub priority: i64,
    pub flagged: bool,
    pub all_day: bool,
    #[allow(dead_code)]
    pub deleted: bool,
    pub record_change_tag: Option<String>,
}

fn to_record_name(id: &str, prefix: &str) -> String {
    let token = format!("{prefix}/");
    if id.starts_with(&token) {
        id.to_string()
    } else {
        format!("{token}{id}")
    }
}

fn millis_to_datetime(ms: i64) -> Option<DateTime<Utc>> {
    Utc.timestamp_millis_opt(ms).single()
}

pub struct RemindersService {
    client: CloudKitClient,
}

impl RemindersService {
    pub fn new(http: reqwest::Client, service_root: &str, client_id: &str) -> Self {
        let base_url = format!(
            "{}/database/1/com.apple.reminders/production/private",
            service_root.trim_end_matches('/')
        );
        Self {
            client: CloudKitClient::new(http, base_url, ZONE_NAME, ZONE_TYPE, client_id),
        }
    }

    /// All reminder lists. CloudKit has no `/records/query` support for the
    /// bare `List` record type; it must be fetched via `/changes/zone`.
    ///
    /// This always replays the *entire* zone change history (no sync
    /// token), which is correct for a one-shot CLI invocation but is far
    /// too slow to call on every app launch on an account with a lot of
    /// history (~30s measured -- see QA-A). Long-lived callers like the
    /// GUI should use `lists_cached` instead.
    pub async fn lists(&self) -> Result<Vec<RemindersList>> {
        let (records, _token) = self.client.changes_all(&["List"], None).await?;
        Ok(records
            .iter()
            .filter(|r| record_type(r).as_deref() == Some("List"))
            .filter_map(record_to_list)
            .collect())
    }

    /// Incremental variant of `lists()` for callers that persist a
    /// `ListCache` across restarts (the GUI): on repeat calls this only
    /// asks CloudKit for what changed since `cache.sync_token`, merges the
    /// diff (upserts and deletions) into the cache, and returns the
    /// resulting full current set -- avoiding a full change-history replay
    /// every time `lists()` would otherwise require.
    pub async fn lists_cached(&self, cache: &mut ListCache) -> Result<Vec<RemindersList>> {
        let (changed, new_token) = self
            .client
            .changes_all(&["List"], cache.sync_token.as_deref())
            .await?;

        for record in changed {
            let Some(name) = record_name(&record) else { continue };
            if record.get("deleted").and_then(Value::as_bool) == Some(true) {
                cache.records.remove(&name);
            } else {
                cache.records.insert(name, record);
            }
        }
        cache.sync_token = new_token;

        Ok(cache
            .records
            .values()
            .filter(|r| record_type(r).as_deref() == Some("List"))
            .filter_map(record_to_list)
            .collect())
    }

    /// Reminders belonging to one list, via CloudKit's compound
    /// `reminderList` query (it also returns related Alarm/Attachment/
    /// Hashtag/RecurrenceRule records, which we currently ignore).
    pub async fn list_reminders(
        &self,
        list_id: &str,
        include_completed: bool,
    ) -> Result<Vec<Reminder>> {
        let filter_by = json!([
            {
                "comparator": "EQUALS",
                "fieldName": "List",
                "fieldValue": {
                    "type": "REFERENCE",
                    "value": { "recordName": list_id, "action": "VALIDATE" },
                },
            },
            {
                "comparator": "EQUALS",
                "fieldName": "includeCompleted",
                "fieldValue": { "type": "INT64", "value": i64::from(include_completed) },
            },
            {
                "comparator": "EQUALS",
                "fieldName": "LookupValidatingReference",
                "fieldValue": { "type": "INT64", "value": 1 },
            },
        ]);

        let records = self
            .client
            .query("reminderList", Some(filter_by), Some(200))
            .await?;

        Ok(records
            .iter()
            .filter(|r| record_type(r).as_deref() == Some("Reminder"))
            .filter_map(record_to_reminder)
            .filter(|r| r.list_id == list_id)
            .collect())
    }

    pub async fn get(&self, reminder_id: &str) -> Result<Reminder> {
        let name = to_record_name(reminder_id, "Reminder");
        let records = self.client.lookup(std::slice::from_ref(&name)).await?;
        records
            .iter()
            .find(|r| record_name(r).as_deref() == Some(name.as_str()))
            .and_then(record_to_reminder)
            .ok_or_else(|| anyhow!("reminder not found: {reminder_id}"))
    }

    /// `all_day` distinguishes the two meanings a due date can carry
    /// (design/idea/expression.md §1): a timed date fires notifications on
    /// both iOS and this proxy (発火時刻), while an all-day date is a silent
    /// deadline used only as pull-side sorting material (締切).
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        list_id: &str,
        title: &str,
        desc: &str,
        priority: i64,
        flagged: bool,
        due_date: Option<DateTime<Utc>>,
        all_day: bool,
    ) -> Result<Reminder> {
        let reminder_uuid = Uuid::new_v4().to_string().to_uppercase();
        let record_name_str = format!("Reminder/{reminder_uuid}");

        let title_doc = encode_crdt_document(title);
        let notes_doc = encode_crdt_document(desc);
        let now_ms = Utc::now().timestamp_millis();

        let mut fields = json!({
            "AllDay": { "type": "INT64", "value": i64::from(all_day) },
            "Completed": { "type": "INT64", "value": 0 },
            "CreationDate": { "type": "TIMESTAMP", "value": now_ms },
            "Deleted": { "type": "INT64", "value": 0 },
            "Flagged": { "type": "INT64", "value": i64::from(flagged) },
            "Imported": { "type": "INT64", "value": 0 },
            "LastModifiedDate": { "type": "TIMESTAMP", "value": now_ms },
            "List": {
                "type": "REFERENCE",
                "value": { "recordName": list_id, "action": "VALIDATE" },
            },
            "NotesDocument": { "type": "STRING", "value": notes_doc },
            "Priority": { "type": "INT64", "value": priority },
            "TitleDocument": { "type": "STRING", "value": title_doc },
        });
        if let Some(due) = due_date {
            fields["DueDate"] = json!({ "type": "TIMESTAMP", "value": due.timestamp_millis() });
        }

        let created = self
            .client
            .modify_one(
                "create",
                &record_name_str,
                "Reminder",
                fields,
                None,
                Some(list_id),
            )
            .await?;
        record_to_reminder(&created).ok_or_else(|| anyhow!("create returned an unparseable record"))
    }

    /// Persist changes to `title`/`desc`/`completed`/`priority`/`flagged`/
    /// `all_day`/`due_date`/`list_id` back to iCloud. Unlike pyicloud's
    /// `update()`, this also submits `List`, so moving a reminder to a
    /// different list is supported.
    pub async fn update(&self, reminder: &Reminder) -> Result<Reminder> {
        let record_name_str = to_record_name(&reminder.id, "Reminder");
        let title_doc = encode_crdt_document(&reminder.title);
        let notes_doc = encode_crdt_document(&reminder.desc);
        let now_ms = Utc::now().timestamp_millis();

        let mut fields = json!({
            "TitleDocument": { "type": "STRING", "value": title_doc },
            "NotesDocument": { "type": "STRING", "value": notes_doc },
            "Completed": { "type": "INT64", "value": i64::from(reminder.completed) },
            "Priority": { "type": "INT64", "value": reminder.priority },
            "Flagged": { "type": "INT64", "value": i64::from(reminder.flagged) },
            "AllDay": { "type": "INT64", "value": i64::from(reminder.all_day) },
            "LastModifiedDate": { "type": "TIMESTAMP", "value": now_ms },
            "List": {
                "type": "REFERENCE",
                "value": { "recordName": reminder.list_id, "action": "VALIDATE" },
            },
        });
        fields["DueDate"] = match reminder.due_date {
            Some(due) => json!({ "type": "TIMESTAMP", "value": due.timestamp_millis() }),
            None => json!({ "type": "TIMESTAMP", "value": Value::Null }),
        };

        let updated = self
            .client
            .modify_one(
                "update",
                &record_name_str,
                "Reminder",
                fields,
                reminder.record_change_tag.as_deref(),
                None,
            )
            .await?;
        record_to_reminder(&updated).ok_or_else(|| anyhow!("update returned an unparseable record"))
    }

    /// Soft-delete (matches iCloud's own semantics: `Deleted: 1`, not a
    /// CloudKit-level record delete).
    pub async fn delete(&self, reminder: &Reminder) -> Result<()> {
        let record_name_str = to_record_name(&reminder.id, "Reminder");
        let now_ms = Utc::now().timestamp_millis();
        let fields = json!({
            "Deleted": { "type": "INT64", "value": 1 },
            "LastModifiedDate": { "type": "TIMESTAMP", "value": now_ms },
        });
        self.client
            .modify_one(
                "update",
                &record_name_str,
                "Reminder",
                fields,
                reminder.record_change_tag.as_deref(),
                None,
            )
            .await?;
        Ok(())
    }

    /// Manually reorder reminders within a list by rewriting the List
    /// record's `ReminderIDs` field (a JSON-array-encoded string field, not
    /// a native `STRING_LIST`). Not implemented by pyicloud or any other
    /// known third-party client; verified working against the real server.
    pub async fn reorder(&self, list: &RemindersList, new_order: &[String]) -> Result<()> {
        let ids_json = serde_json::to_string(new_order)?;
        let fields = json!({ "ReminderIDs": { "type": "STRING", "value": ids_json } });
        self.client
            .modify_one(
                "update",
                &list.id,
                "List",
                fields,
                list.record_change_tag.as_deref(),
                None,
            )
            .await?;
        Ok(())
    }
}

fn record_to_list(record: &Value) -> Option<RemindersList> {
    let id = record_name(record)?;
    let title = field_text(record, "Name").unwrap_or_else(|| "Untitled".to_string());
    let reminder_ids = field_string(record, "ReminderIDs")
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default();
    let color_hex = field_string(record, "Color")
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v["daHexString"].as_str().map(str::to_string));
    let badge_emblem = field_string(record, "BadgeEmblem");
    Some(RemindersList {
        id,
        title,
        reminder_ids,
        record_change_tag: record_change_tag(record),
        color_hex,
        badge_emblem,
    })
}

fn record_to_reminder(record: &Value) -> Option<Reminder> {
    let id = record_name(record)?;
    let list_id = field_reference_name(record, "List").unwrap_or_default();

    let title = field_string(record, "TitleDocument")
        .and_then(|doc| decode_crdt_document(&doc).ok())
        .unwrap_or_else(|| "Untitled".to_string());
    let desc = field_string(record, "NotesDocument")
        .and_then(|doc| decode_crdt_document(&doc).ok())
        .unwrap_or_default();

    Some(Reminder {
        id,
        list_id,
        title,
        desc,
        completed: field_bool(record, "Completed"),
        due_date: field_int(record, "DueDate").and_then(millis_to_datetime),
        created: field_int(record, "CreationDate").and_then(millis_to_datetime),
        priority: field_int(record, "Priority").unwrap_or(0),
        flagged: field_bool(record, "Flagged"),
        all_day: field_bool(record, "AllDay"),
        deleted: field_bool(record, "Deleted"),
        record_change_tag: record_change_tag(record),
    })
}
