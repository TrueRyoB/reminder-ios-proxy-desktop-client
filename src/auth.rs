//! idmsa.apple.com web-login flow (the same one icloud.com itself uses),
//! ported from pyicloud's `base.py`/`session.py`. NOT the GSA/anisette flow
//! used by AltServer/SideStore-style tools -- that is a different Apple
//! auth surface and does not apply here.
//!
//! Flow: `authorize/signin` -> SRP `signin/init` -> `signin/complete` ->
//! (if HTTP 409 + `authType: "hsa2"`) trusted-device 2FA code -> `2sv/trust`
//! -> `{setup}/accountLogin`, which returns the webservices map (including
//! the Reminders CloudKit endpoint) used by everything downstream.

use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Response, StatusCode};
use reqwest_cookie_store::CookieStoreMutex;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::srp::{derive_password, SrpClient, SrpProtocol};

const WIDGET_KEY: &str = "d39ba9916b7251055b22c7f910e2ea796ee65e98b2ddecea8f5dde8d9d1a815d";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.3.1 Safari/605.1.15";

/// Apple session-scoped values pyicloud threads through `session.data`,
/// captured from response headers across requests (not stored as cookies).
#[derive(Default, Debug, Clone)]
struct SessionState {
    scnt: Option<String>,
    session_id: Option<String>,
    auth_attributes: Option<String>,
    session_token: Option<String>,
    trust_token: Option<String>,
    account_country: Option<String>,
}

/// Result of the initial `signin/complete` attempt.
pub enum LoginOutcome {
    /// No 2FA was required (already-trusted session); login is complete.
    Complete(Value),
    /// Apple returned `409 {"authType": "hsa2"}`: a 2FA code is required.
    TwoFactorRequired,
}

pub struct AppleAuthClient {
    http: Client,
    cookie_store: Arc<CookieStoreMutex>,
    endpoint_auth: String,
    endpoint_idmsa: String,
    endpoint_setup: String,
    client_id: String,
    username: String,
    session: SessionState,
}

impl AppleAuthClient {
    pub fn new(username: &str) -> Result<Self> {
        Self::build(
            username,
            cookie_store::CookieStore::default(),
            SessionState::default(),
            None,
        )
    }

    /// Resume from previously persisted cookies + session/trust tokens
    /// (see `session_store`), instead of starting a fresh, empty session.
    /// The cookie store must be supplied at construction time, since the
    /// underlying `reqwest::Client` is bound to a specific cookie provider
    /// when built -- swapping it in afterward would silently do nothing.
    pub fn with_state(
        username: &str,
        cookie_jar: cookie_store::CookieStore,
        persisted: &crate::session_store::PersistedAuthState,
    ) -> Result<Self> {
        let session = SessionState {
            session_token: persisted.session_token.clone(),
            trust_token: persisted.trust_token.clone(),
            account_country: persisted.account_country.clone(),
            ..SessionState::default()
        };
        Self::build(username, cookie_jar, session, persisted.client_id.clone())
    }

    fn build(
        username: &str,
        cookie_jar: cookie_store::CookieStore,
        session: SessionState,
        client_id: Option<String>,
    ) -> Result<Self> {
        let cookie_store = Arc::new(CookieStoreMutex::new(cookie_jar));

        let idmsa = "https://idmsa.apple.com".to_string();
        let auth = format!("{idmsa}/appleauth/auth");
        let setup = "https://setup.icloud.com/setup/ws/1".to_string();
        let home_endpoint = "https://www.icloud.com".to_string();

        // Every request in a pyicloud session carries these two by default
        // (Origin/Referer = icloud.com); auth-endpoint calls override Referer
        // to idmsa.apple.com via auth_headers(), but calls like accountLogin
        // rely on these session-wide defaults. Apple's server rejects
        // accountLogin outright without a valid Origin header.
        let mut default_headers = HeaderMap::new();
        default_headers.insert(
            reqwest::header::ORIGIN,
            HeaderValue::from_str(&home_endpoint)?,
        );
        default_headers.insert(
            reqwest::header::REFERER,
            HeaderValue::from_str(&format!("{home_endpoint}/"))?,
        );

        let http = Client::builder()
            .cookie_provider(cookie_store.clone())
            .user_agent(USER_AGENT)
            .default_headers(default_headers)
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            http,
            cookie_store,
            endpoint_auth: auth,
            endpoint_idmsa: idmsa,
            endpoint_setup: setup,
            client_id: client_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            username: username.to_string(),
            session,
        })
    }

    /// Try to establish a session using only previously persisted state
    /// (no password, no 2FA) -- mirrors pyicloud's
    /// `_authenticate_with_token()`. Fails if there's no persisted session
    /// token, or Apple has since invalidated it (expired/HTTP 421 etc.);
    /// callers should fall back to `login()` in that case.
    pub async fn try_resume(&mut self) -> Result<Value> {
        if self.session.session_token.is_none() {
            bail!("no persisted session token to resume from");
        }
        self.account_login().await
    }

    /// The underlying HTTP client, sharing this session's cookies. Reuse
    /// this (don't build a new `reqwest::Client`) for any follow-up calls
    /// that need the same authenticated session, e.g. `RemindersService`.
    pub fn http_client(&self) -> Client {
        self.http.clone()
    }

    /// Same client_id used for `X-Apple-OAuth-State`/`X-Apple-Frame-Id`
    /// during login; CloudKit calls must also send it as a `clientId`
    /// query param on every request.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub fn snapshot_cookie_store(&self) -> Result<cookie_store::CookieStore> {
        let guard = self
            .cookie_store
            .lock()
            .map_err(|e| anyhow!("cookie store lock poisoned: {e}"))?;
        Ok(guard.clone())
    }

    /// A snapshot of the session/trust tokens, suitable for
    /// `session_store::save_auth_state`.
    pub fn persisted_state(&self) -> crate::session_store::PersistedAuthState {
        crate::session_store::PersistedAuthState {
            session_token: self.session.session_token.clone(),
            trust_token: self.session.trust_token.clone(),
            account_country: self.session.account_country.clone(),
            client_id: Some(self.client_id.clone()),
        }
    }

    fn auth_headers(&self, overrides: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let mut set = |name: &str, value: &str| {
            if let (Ok(n), Ok(v)) = (
                HeaderName::from_bytes(name.as_bytes()),
                HeaderValue::from_str(value),
            ) {
                headers.insert(n, v);
            }
        };

        set("Accept", "application/json, text/javascript");
        set("Content-Type", "application/json");
        set("X-Apple-OAuth-Client-Id", WIDGET_KEY);
        set("X-Apple-OAuth-Client-Type", "firstPartyAuth");
        set("X-Apple-OAuth-Redirect-URI", "https://www.icloud.com");
        set("X-Apple-OAuth-Require-Grant-Code", "true");
        set("X-Apple-OAuth-Response-Mode", "web_message");
        set("X-Apple-OAuth-Response-Type", "code");
        set("X-Apple-OAuth-State", &self.client_id);
        set("X-Apple-Widget-Key", WIDGET_KEY);
        set("Referer", &self.endpoint_idmsa);
        set("X-Apple-Frame-Id", &self.client_id);

        if let Some(v) = &self.session.scnt {
            set("scnt", v);
        }
        if let Some(v) = &self.session.session_id {
            set("X-Apple-ID-Session-Id", v);
        }
        if let Some(v) = &self.session.auth_attributes {
            set("X-Apple-Auth-Attributes", v);
        }

        for (name, value) in overrides {
            set(name, value);
        }

        headers
    }

    /// Mirrors pyicloud's `_update_session_data`: read Apple's session
    /// bookkeeping headers off every response, success or error alike.
    fn capture_session_headers(&mut self, resp: &Response) {
        let get = |name: &str| resp.headers().get(name).and_then(|v| v.to_str().ok().map(str::to_string));

        if let Some(v) = get("scnt") {
            self.session.scnt = Some(v);
        }
        if let Some(v) = get("X-Apple-ID-Session-Id") {
            self.session.session_id = Some(v);
        }
        if let Some(v) = get("X-Apple-Auth-Attributes") {
            self.session.auth_attributes = Some(v);
        }
        if let Some(v) = get("X-Apple-Session-Token") {
            self.session.session_token = Some(v);
        }
        if let Some(v) = get("X-Apple-TwoSV-Trust-Token") {
            self.session.trust_token = Some(v);
        }
        if let Some(v) = get("X-Apple-ID-Account-Country") {
            self.session.account_country = Some(v);
        }

        tracing::debug!(
            url = %resp.url(),
            status = %resp.status(),
            scnt = ?self.session.scnt.as_deref().map(|s| &s[..s.len().min(12)]),
            session_id = ?self.session.session_id.as_deref().map(|s| &s[..s.len().min(12)]),
            auth_attrs_present = self.session.auth_attributes.is_some(),
            session_token_present = self.session.session_token.is_some(),
            trust_token_present = self.session.trust_token.is_some(),
            account_country = ?self.session.account_country,
            "session state updated"
        );
    }

    /// Step 1: establish the initial OAuth-style signin session (sets
    /// cookies used by every subsequent request).
    async fn authorize_signin(&mut self) -> Result<()> {
        let resp = self
            .http
            .get(format!("{}/authorize/signin", self.endpoint_auth))
            .query(&[
                ("frame_id", self.client_id.as_str()),
                ("skVersion", "7"),
                ("iframeid", self.client_id.as_str()),
                ("client_id", WIDGET_KEY),
                ("response_type", "code"),
                ("redirect_uri", "https://www.icloud.com"),
                ("response_mode", "web_message"),
                ("state", self.client_id.as_str()),
                ("authVersion", "latest"),
            ])
            .headers(self.auth_headers(&[]))
            .send()
            .await
            .context("authorize/signin request failed")?;

        self.capture_session_headers(&resp);
        let status = resp.status();
        if !status.is_success() {
            bail!("authorize/signin failed with status {status}");
        }
        Ok(())
    }

    /// Steps 2+3: SRP handshake (`signin/init`) then submit the proof
    /// (`signin/complete`). Returns whether 2FA is required.
    pub async fn login(&mut self, password: &str) -> Result<LoginOutcome> {
        self.authorize_signin().await?;

        let a_secret = crate::srp::random_secret_256();
        let srp_client = SrpClient::new(&self.username, &a_secret);

        let init_body = json!({
            "a": B64.encode(srp_client.public_ephemeral()),
            "accountName": self.username,
            "protocols": ["s2k", "s2k_fo"],
        });

        let resp = self
            .http
            .post(format!("{}/signin/init", self.endpoint_auth))
            .headers(self.auth_headers(&[]))
            .json(&init_body)
            .send()
            .await
            .context("signin/init request failed")?;
        self.capture_session_headers(&resp);
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            bail!("signin/init failed ({status}): {text}");
        }
        let init_json: Value = resp.json().await.context("signin/init: invalid JSON")?;

        let salt = B64
            .decode(init_json["salt"].as_str().ok_or_else(|| anyhow!("missing salt"))?)
            .context("invalid salt base64")?;
        let b_pub = B64
            .decode(init_json["b"].as_str().ok_or_else(|| anyhow!("missing b"))?)
            .context("invalid b base64")?;
        let c = init_json["c"]
            .as_str()
            .ok_or_else(|| anyhow!("missing c"))?
            .to_string();
        let iterations = init_json["iteration"]
            .as_u64()
            .ok_or_else(|| anyhow!("missing iteration"))? as u32;
        let protocol = match init_json["protocol"].as_str() {
            Some("s2k") => SrpProtocol::S2k,
            Some("s2k_fo") => SrpProtocol::S2kFo,
            other => bail!("unexpected SRP protocol from server: {other:?}"),
        };

        let derived = derive_password(password, &salt, iterations, 32, protocol);
        let challenge = srp_client
            .process_challenge(&salt, &b_pub, &derived)
            .map_err(|e| anyhow!("SRP challenge failed: {e}"))?;

        let complete_body = json!({
            "accountName": self.username,
            "c": c,
            "m1": B64.encode(&challenge.m1),
            "m2": B64.encode(&challenge.h_amk),
            "rememberMe": true,
            "trustTokens": self.session.trust_token.clone().map(|t| vec![t]).unwrap_or_default(),
        });

        let resp = self
            .http
            .post(format!("{}/signin/complete", self.endpoint_auth))
            .query(&[("isRememberMeEnabled", "true")])
            .headers(self.auth_headers(&[]))
            .json(&complete_body)
            .send()
            .await
            .context("signin/complete request failed")?;
        self.capture_session_headers(&resp);

        let status = resp.status();
        if status == StatusCode::CONFLICT {
            let body: Value = resp.json().await.unwrap_or(Value::Null);
            if body.get("authType").and_then(Value::as_str) == Some("hsa2") {
                // Apple does not push a verification code to trusted devices
                // on its own for API-based (non-browser) clients; this call
                // is what actually triggers the push. Best-effort: a failure
                // here shouldn't block the user from entering a code that
                // may already be on its way (e.g. re-running after a prior
                // successful push).
                let _ = self.request_2fa_push().await;
                return Ok(LoginOutcome::TwoFactorRequired);
            }
            bail!("signin/complete returned 409 but not hsa2: {body:?}");
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("signin/complete failed ({status}): invalid email/password combination? {text}");
        }

        // Some already-trusted sessions skip 2FA entirely.
        let data = self.account_login().await?;
        Ok(LoginOutcome::Complete(data))
    }

    /// Explicitly request that Apple push a verification code to trusted
    /// devices. Required after a 409/hsa2 response: unlike the browser
    /// sign-in flow, Apple will not send this automatically for an
    /// API-based client.
    async fn request_2fa_push(&mut self) -> Result<()> {
        let resp = self
            .http
            .get(format!("{}/verify/trusteddevice", self.endpoint_auth))
            .headers(self.auth_headers(&[("Accept", "application/json")]))
            .send()
            .await
            .context("verify/trusteddevice request failed")?;
        self.capture_session_headers(&resp);
        if !resp.status().is_success() {
            bail!("verify/trusteddevice failed with status {}", resp.status());
        }
        Ok(())
    }

    /// Submit the 6-digit code from a trusted device push notification.
    pub async fn validate_trusted_device_code(&mut self, code: &str) -> Result<()> {
        let body = json!({ "securityCode": { "code": code } });
        let resp = self
            .http
            .post(format!("{}/verify/trusteddevice/securitycode", self.endpoint_auth))
            .headers(self.auth_headers(&[("Accept", "application/json")]))
            .json(&body)
            .send()
            .await
            .context("verify/trusteddevice/securitycode request failed")?;
        self.capture_session_headers(&resp);
        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("2FA code rejected: {text}");
        }
        Ok(())
    }

    /// Mark this session as trusted so future logins can skip 2FA, then
    /// finish with `accountLogin`.
    pub async fn trust_session(&mut self) -> Result<Value> {
        let resp = self
            .http
            .get(format!("{}/2sv/trust", self.endpoint_auth))
            .headers(self.auth_headers(&[]))
            .send()
            .await
            .context("2sv/trust request failed")?;
        self.capture_session_headers(&resp);
        if !resp.status().is_success() {
            bail!("2sv/trust failed with status {}", resp.status());
        }
        self.account_login().await
    }

    /// Final step: exchange the session token for the full account
    /// data blob, including the `webservices` map (Reminders' CloudKit
    /// endpoint lives here).
    async fn account_login(&mut self) -> Result<Value> {
        let body = json!({
            "accountCountryCode": self.session.account_country,
            "dsWebAuthToken": self.session.session_token,
            "extended_login": true,
            "trustToken": self.session.trust_token.clone().unwrap_or_default(),
        });

        tracing::debug!(
            account_country = ?self.session.account_country,
            session_token_present = self.session.session_token.is_some(),
            trust_token_present = self.session.trust_token.is_some(),
            "accountLogin request"
        );

        let resp = self
            .http
            .post(format!("{}/accountLogin", self.endpoint_setup))
            .json(&body)
            .send()
            .await
            .context("accountLogin request failed")?;
        self.capture_session_headers(&resp);
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            bail!("accountLogin failed with status {status}: {text}");
        }
        resp.json().await.context("accountLogin: invalid JSON")
    }
}
