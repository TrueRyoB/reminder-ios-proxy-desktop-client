//! Minimal CloudKit Web Services wire client, scoped to what the Reminders
//! container needs: `/records/query`, `/records/lookup`, `/records/modify`,
//! `/changes/zone`. Ported from pyicloud's typed `CloudKitContainerClient`,
//! but using raw `serde_json::Value` instead of a full typed model set --
//! we only need a handful of field types (INT64, TIMESTAMP, STRING,
//! STRING_LIST, REFERENCE), not the full CloudKit field type zoo.

use anyhow::{anyhow, bail, Context, Result};
use reqwest::Client;
use serde_json::{json, Value};

pub struct CloudKitClient {
    http: Client,
    /// e.g. `{service_root}/database/1/com.apple.reminders/production/private`
    base_url: String,
    zone_name: String,
    zone_type: String,
    /// Query params sent on every request (clientBuildNumber, remapEnums, etc).
    base_params: Vec<(String, String)>,
}

impl CloudKitClient {
    /// `client_id` must be the same one used for `X-Apple-OAuth-State`
    /// during idmsa login (see `AppleAuthClient::client_id()`) -- CloudKit
    /// requests are rejected (HTTP 400) without a matching `clientId`,
    /// `clientBuildNumber`, and `clientMasteringNumber` on every request.
    pub fn new(http: Client, base_url: String, zone_name: &str, zone_type: &str, client_id: &str) -> Self {
        Self {
            http,
            base_url,
            zone_name: zone_name.to_string(),
            zone_type: zone_type.to_string(),
            base_params: vec![
                ("remapEnums".to_string(), "true".to_string()),
                ("getCurrentSyncToken".to_string(), "true".to_string()),
                ("clientBuildNumber".to_string(), "2534Project66".to_string()),
                ("clientMasteringNumber".to_string(), "2534B22".to_string()),
                ("clientId".to_string(), client_id.to_string()),
            ],
        }
    }

    fn zone_id_json(&self) -> Value {
        json!({ "zoneName": self.zone_name, "zoneType": self.zone_type })
    }

    async fn post(&self, path: &str, payload: Value) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .http
            .post(&url)
            .query(&self.base_params)
            .json(&payload)
            .send()
            .await
            .with_context(|| format!("CloudKit {path} request failed"))?;
        let status = resp.status();
        if !status.is_success() {
            let built = self
                .http
                .post(&url)
                .query(&self.base_params)
                .json(&payload)
                .build()
                .ok()
                .map(|r| r.url().to_string());
            let headers = resp.headers().clone();
            let text = resp.text().await.unwrap_or_default();
            eprintln!(
                "[debug] CloudKit POST {path} failed | full_url={built:?} | payload={payload} | headers={headers:?}"
            );
            bail!("CloudKit {path} failed ({status}): {text}");
        }
        resp.json()
            .await
            .with_context(|| format!("CloudKit {path}: invalid JSON response"))
    }

    /// Records matching `record_type`, optionally filtered/sorted, paging
    /// through `continuationMarker` until exhausted.
    pub async fn query(
        &self,
        record_type: &str,
        filter_by: Option<Value>,
        results_limit: Option<u32>,
    ) -> Result<Vec<Value>> {
        let mut all_records = Vec::new();
        let mut continuation: Option<String> = None;

        loop {
            let mut query = json!({ "recordType": record_type });
            if let Some(f) = &filter_by {
                query["filterBy"] = f.clone();
            }
            let mut payload = json!({ "query": query, "zoneID": self.zone_id_json() });
            if let Some(limit) = results_limit {
                payload["resultsLimit"] = json!(limit);
            }
            if let Some(c) = &continuation {
                payload["continuationMarker"] = json!(c);
            }

            let data = self.post("/records/query", payload).await?;
            assert_no_record_errors(&data, "query")?;
            if let Some(records) = data.get("records").and_then(Value::as_array) {
                all_records.extend(records.iter().cloned());
            }

            continuation = data
                .get("continuationMarker")
                .and_then(Value::as_str)
                .map(str::to_string);
            if continuation.is_none() {
                break;
            }
        }

        Ok(all_records)
    }

    pub async fn lookup(&self, record_names: &[String]) -> Result<Vec<Value>> {
        let records: Vec<Value> = record_names
            .iter()
            .map(|n| json!({ "recordName": n }))
            .collect();
        let payload = json!({ "records": records, "zoneID": self.zone_id_json() });
        let data = self.post("/records/lookup", payload).await?;
        assert_no_record_errors(&data, "lookup")?;
        Ok(data
            .get("records")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    /// Fetch all records of the given types via `/changes/zone`, paging
    /// through `moreComing`/`syncToken` until the server reports no more.
    /// This is how Lists are fetched (there is no `/records/query` support
    /// for the bare `List` record type).
    pub async fn changes_all(&self, desired_record_types: &[&str]) -> Result<Vec<Value>> {
        let mut all_records = Vec::new();
        let mut sync_token: Option<String> = None;

        loop {
            let mut zone_req = json!({
                "zoneID": self.zone_id_json(),
                "desiredRecordTypes": desired_record_types,
            });
            if let Some(t) = &sync_token {
                zone_req["syncToken"] = json!(t);
            }
            let payload = json!({ "zones": [zone_req] });

            let data = self.post("/changes/zone", payload).await?;
            let zones = data
                .get("zones")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let Some(zone) = zones.into_iter().next() else {
                break;
            };
            assert_no_record_errors(&zone, "changes/zone")?;
            if let Some(records) = zone.get("records").and_then(Value::as_array) {
                all_records.extend(records.iter().cloned());
            }

            sync_token = zone
                .get("syncToken")
                .and_then(Value::as_str)
                .map(str::to_string);
            let more_coming = zone
                .get("moreComing")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if !more_coming {
                break;
            }
        }

        Ok(all_records)
    }

    /// Create/update/delete a single record. `fields` must already be in
    /// CloudKit wire shape: `{"FieldName": {"type": "...", "value": ...}}`.
    pub async fn modify_one(
        &self,
        operation_type: &str,
        record_name: &str,
        record_type: &str,
        fields: Value,
        record_change_tag: Option<&str>,
        parent_record_name: Option<&str>,
    ) -> Result<Value> {
        let mut record = json!({
            "recordName": record_name,
            "recordType": record_type,
            "fields": fields,
        });
        if let Some(tag) = record_change_tag {
            record["recordChangeTag"] = json!(tag);
        }
        if let Some(parent) = parent_record_name {
            record["parent"] = json!({ "recordName": parent });
        }

        let payload = json!({
            "operations": [{ "operationType": operation_type, "record": record }],
            "zoneID": self.zone_id_json(),
        });
        let data = self.post("/records/modify", payload).await?;
        assert_no_record_errors(&data, "modify")?;
        data.get("records")
            .and_then(Value::as_array)
            .and_then(|r| r.first())
            .cloned()
            .ok_or_else(|| anyhow!("modify returned no records"))
    }
}

/// CloudKit mixes per-record error items into `records[]` on partial
/// failure (identified by having `serverErrorCode` but no `recordType`).
fn assert_no_record_errors(container: &Value, operation_name: &str) -> Result<()> {
    let Some(records) = container.get("records").and_then(Value::as_array) else {
        return Ok(());
    };
    let errors: Vec<&Value> = records
        .iter()
        .filter(|r| r.get("serverErrorCode").is_some() && r.get("recordType").is_none())
        .collect();
    if errors.is_empty() {
        return Ok(());
    }
    let details: Vec<String> = errors
        .iter()
        .map(|e| {
            format!(
                "{}: {} ({})",
                e.get("recordName").and_then(Value::as_str).unwrap_or("<unknown>"),
                e.get("serverErrorCode").and_then(Value::as_str).unwrap_or("?"),
                e.get("reason").and_then(Value::as_str).unwrap_or("no reason provided"),
            )
        })
        .collect();
    bail!(
        "{operation_name} failed for {} record(s): {}",
        errors.len(),
        details.join("; ")
    );
}

// --- Field read helpers -----------------------------------------------

pub fn field_value<'a>(record: &'a Value, name: &str) -> Option<&'a Value> {
    record.get("fields")?.get(name)?.get("value")
}

pub fn field_string(record: &Value, name: &str) -> Option<String> {
    field_value(record, name)?.as_str().map(str::to_string)
}

/// Some text fields (e.g. List `Name`) are wire-encoded as `ENCRYPTED_BYTES`
/// (base64 of UTF-8 bytes) rather than plain `STRING`. Handle both shapes
/// rather than assuming one.
pub fn field_text(record: &Value, name: &str) -> Option<String> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let field = record.get("fields")?.get(name)?;
    let type_tag = field.get("type").and_then(Value::as_str).unwrap_or("");
    let value = field.get("value")?;
    if type_tag == "ENCRYPTED_BYTES" {
        let b64 = value.as_str()?;
        let bytes = STANDARD.decode(b64).ok()?;
        String::from_utf8(bytes).ok()
    } else {
        value.as_str().map(str::to_string)
    }
}

pub fn field_int(record: &Value, name: &str) -> Option<i64> {
    field_value(record, name)?.as_i64()
}

pub fn field_bool(record: &Value, name: &str) -> bool {
    field_int(record, name).unwrap_or(0) != 0
}

pub fn field_reference_name(record: &Value, name: &str) -> Option<String> {
    field_value(record, name)?
        .get("recordName")?
        .as_str()
        .map(str::to_string)
}

#[allow(dead_code)]
pub fn field_string_list(record: &Value, name: &str) -> Vec<String> {
    field_value(record, name)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

pub fn record_change_tag(record: &Value) -> Option<String> {
    record
        .get("recordChangeTag")
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub fn record_name(record: &Value) -> Option<String> {
    record
        .get("recordName")
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub fn record_type(record: &Value) -> Option<String> {
    record
        .get("recordType")
        .and_then(Value::as_str)
        .map(str::to_string)
}
