//! Shared app state for Tauri commands: the current authentication phase.
//! `AppleAuthClient`'s login-related methods are all `&mut self` (session
//! bookkeeping updates from response headers), so it only needs to sit
//! behind this mutex during the brief login/2FA window. Once `Ready`,
//! `RemindersService` needs no lock at all (every method is `&self`) and
//! can be shared via `Arc` with a background poller in a later milestone.

use std::sync::Arc;

use reminder_core::auth::AppleAuthClient;
use reminder_core::reminders::RemindersService;

#[derive(Default)]
pub enum AuthState {
    #[default]
    LoggedOut,
    AwaitingTwoFactor {
        client: Box<AppleAuthClient>,
        password: String,
    },
    Ready { reminders: Arc<RemindersService> },
}

impl AuthState {
    /// Borrow the `RemindersService` if a session is fully established.
    /// Every list/CRUD command needs this and should surface a clear error
    /// (rather than panicking) if called before login completes.
    pub fn reminders(&self) -> Option<&Arc<RemindersService>> {
        match self {
            AuthState::Ready { reminders } => Some(reminders),
            _ => None,
        }
    }
}

#[derive(Default)]
pub struct AppState {
    pub auth: tokio::sync::Mutex<AuthState>,
}
