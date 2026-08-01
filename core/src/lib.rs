//! Core logic for talking to iCloud's private CloudKit Reminders API:
//! idmsa login (SRP6a + 2FA), session persistence, CloudKit CRUD/reorder,
//! and the "topotext" CRDT text encoding Reminders titles/notes require.
//!
//! Shared by the `reminder-proxy-client` CLI and the Tauri GUI. Neither
//! consumer's interactive concerns (terminal prompts vs. UI dialogs) live
//! here -- see `bootstrap` for the prompt-free orchestration pieces both
//! consumers call into.

pub mod auth;
pub mod bootstrap;
pub mod cloudkit;
pub mod crdt;
pub mod notify;
pub mod proxy_store;
pub mod reminders;
pub mod session_store;
pub mod srp;
